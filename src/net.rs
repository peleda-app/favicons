use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use reqwest::header::ACCEPT;
use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::config::Fetch;
use crate::dns::{ReqwestResolver, SafeDns};

pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 64)
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        || (o[0] == 198 && (o[1] & 0xfe) == 18)
        || o[0] >= 240)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let s = ip.segments();
    if (s[0] & 0xe000) != 0x2000 {
        return false;
    }
    let embedded_v4 =
        |hi: u16, lo: u16| Ipv4Addr::new((hi >> 8) as u8, hi as u8, (lo >> 8) as u8, lo as u8);
    match s[0] {
        0x2001 if s[1] == 0 => false,
        0x2001 if s[1] == 0x0db8 => false,
        0x2001 if s[1] < 0x0200 => false,
        0x2002 => is_public_v4(embedded_v4(s[1], s[2])),
        _ => true,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UrlPolicy {
    pub allow_http: bool,
    pub allow_ip_hosts: bool,
    pub ports: Vec<u16>,
}

impl UrlPolicy {
    pub fn new(cfg: &Fetch) -> Self {
        Self {
            allow_http: cfg.allow_http,
            allow_ip_hosts: cfg.allow_ip_hosts,
            ports: cfg.allowed_ports.clone(),
        }
    }

    pub fn allows(&self, url: &Url) -> bool {
        let scheme_ok = match url.scheme() {
            "https" => true,
            "http" => self.allow_http,
            _ => false,
        };
        if !scheme_ok || !url.username().is_empty() || url.password().is_some() {
            return false;
        }
        if !url
            .port_or_known_default()
            .is_some_and(|p| self.ports.contains(&p))
        {
            return false;
        }
        match url.host() {
            Some(Host::Domain(d)) => crate::domain::is_valid_hostname(d.trim_end_matches('.')),
            Some(Host::Ipv4(ip)) => self.allow_ip_hosts && is_public_ip(IpAddr::V4(ip)),
            Some(Host::Ipv6(ip)) => self.allow_ip_hosts && is_public_ip(IpAddr::V6(ip)),
            None => false,
        }
    }
}

#[derive(Debug)]
pub struct Fetched {
    pub bytes: Vec<u8>,
    pub final_url: Url,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("url not allowed")]
    Forbidden,
    #[error("http status {0}")]
    Status(u16),
    #[error("redirect to {0}")]
    Redirect(Url),
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("response exceeds {0} bytes")]
    TooLarge(usize),
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Redirects {
    Manual,
    Follow,
}

#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    urls: Arc<UrlPolicy>,
}

fn tls_config() -> anyhow::Result<rustls::ClientConfig> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .context("tls protocol versions")?
    .with_root_certificates(roots)
    .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

impl HttpClient {
    pub fn new(cfg: &Fetch, dns: Arc<SafeDns>, redirects: Redirects) -> anyhow::Result<Self> {
        let max_redirects = cfg.max_redirects;
        let urls = Arc::new(UrlPolicy::new(cfg));
        let follow = urls.clone();
        let policy = match redirects {
            Redirects::Manual => reqwest::redirect::Policy::none(),
            Redirects::Follow => reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= max_redirects {
                    attempt.error("too many redirects")
                } else if !follow.allows(attempt.url()) {
                    attempt.error("redirect target not allowed")
                } else {
                    attempt.follow()
                }
            }),
        };
        let client = reqwest::Client::builder()
            .user_agent(&cfg.user_agent)
            .no_proxy()
            .dns_resolver(Arc::new(ReqwestResolver(dns)))
            .redirect(policy)
            .connect_timeout(cfg.connect_timeout())
            .timeout(cfg.request_timeout())
            .pool_max_idle_per_host(1)
            .pool_idle_timeout(Duration::from_secs(15))
            .https_only(!urls.allow_http)
            .referer(false)
            .tls_backend_preconfigured(tls_config()?)
            .build()
            .context("building http client")?;
        Ok(Self { client, urls })
    }

    pub fn urls(&self) -> &UrlPolicy {
        &self.urls
    }

    pub async fn get(&self, url: &Url, accept: &str, max: usize) -> Result<Fetched, FetchError> {
        self.fetch(url, accept, max, false).await
    }

    pub async fn get_head(
        &self,
        url: &Url,
        accept: &str,
        max: usize,
    ) -> Result<Fetched, FetchError> {
        self.fetch(url, accept, max, true).await
    }

    async fn fetch(
        &self,
        url: &Url,
        accept: &str,
        max_bytes: usize,
        truncate: bool,
    ) -> Result<Fetched, FetchError> {
        if !self.urls.allows(url) {
            return Err(FetchError::Forbidden);
        }
        let mut resp = self
            .client
            .get(url.clone())
            .header(ACCEPT, accept)
            .send()
            .await?;
        if resp.status().is_redirection() {
            let next = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|loc| url.join(loc.trim()).ok());
            return match next {
                Some(next) if self.urls.allows(&next) => Err(FetchError::Redirect(next)),
                Some(_) => Err(FetchError::Forbidden),
                None => Err(FetchError::Status(resp.status().as_u16())),
            };
        }
        if !resp.status().is_success() {
            return Err(FetchError::Status(resp.status().as_u16()));
        }
        if !truncate && resp.content_length().is_some_and(|l| l > max_bytes as u64) {
            return Err(FetchError::TooLarge(max_bytes));
        }
        let final_url = resp.url().clone();
        let mut bytes = Vec::with_capacity(
            resp.content_length()
                .map_or(16 * 1024, |l| l as usize)
                .min(max_bytes),
        );
        while let Some(chunk) = resp.chunk().await? {
            let scan_from = bytes.len().saturating_sub(HEAD_MARKER_MAX - 1);
            if bytes.len() + chunk.len() > max_bytes {
                if !truncate {
                    return Err(FetchError::TooLarge(max_bytes));
                }
                bytes.extend_from_slice(&chunk[..max_bytes - bytes.len()]);
                break;
            }
            bytes.extend_from_slice(&chunk);
            if truncate && contains_head_end(&bytes[scan_from..]) {
                break;
            }
        }
        Ok(Fetched { bytes, final_url })
    }
}

const HEAD_MARKERS: [&[u8]; 2] = [b"</head", b"<body"];
const HEAD_MARKER_MAX: usize = 6;

pub fn contains_head_end(buf: &[u8]) -> bool {
    HEAD_MARKERS
        .iter()
        .any(|m| buf.windows(m.len()).any(|w| w.eq_ignore_ascii_case(m)))
}

pub async fn read_source(
    client: &HttpClient,
    source: &str,
    max_bytes: usize,
) -> anyhow::Result<Vec<u8>> {
    if source.starts_with("https://") || source.starts_with("http://") {
        let url = Url::parse(source)?;
        return Ok(client.get(&url, "*/*", max_bytes).await?.bytes);
    }
    let path = source.strip_prefix("file://").unwrap_or(source);
    let meta = tokio::fs::metadata(path)
        .await
        .with_context(|| format!("reading {path}"))?;
    if meta.len() > max_bytes as u64 {
        bail!("{path} exceeds {max_bytes} bytes");
    }
    Ok(tokio::fs::read(path).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_internal_addresses() {
        for ip in [
            "0.0.0.0",
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "192.0.0.8",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::a9fe:a9fe",
            "2001:db8::1",
            "2001::1",
            "2002:7f00:1::",
            "2002:a00:1::",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip} must be blocked");
        }
    }

    #[test]
    fn allows_public_addresses() {
        for ip in [
            "1.1.1.1",
            "93.184.215.14",
            "2606:4700:4700::1111",
            "2a00:1450:4001::200e",
            "2002:0101:0101::",
        ] {
            assert!(is_public_ip(ip.parse().unwrap()), "{ip} must be allowed");
        }
    }

    #[test]
    fn detects_end_of_head() {
        assert!(contains_head_end(b"<html><head><title>x</title></HEAD>"));
        assert!(contains_head_end(b"<html><BODY class=a>"));
        assert!(!contains_head_end(
            b"<html><head><link rel=icon href=/a.png>"
        ));
        assert!(!contains_head_end(b"<html><header>"));
    }

    #[test]
    fn validates_urls() {
        let strict = UrlPolicy::new(&Fetch::default());
        let ok = |s: &str| strict.allows(&Url::parse(s).unwrap());
        assert!(ok("https://example.com/favicon.ico"));
        assert!(ok("https://example.com:443/favicon.ico"));
        assert!(!ok("http://example.com/favicon.ico"));
        assert!(!ok("https://example.com:8443/favicon.ico"));
        assert!(!ok("https://example.com:80/"));
        assert!(!ok("https://example.com:6379/"));
        assert!(!ok("https://1.1.1.1/favicon.ico"));
        assert!(!ok("https://[2606:4700:4700::1111]/favicon.ico"));
        assert!(!ok("https://2130706433/"));
        assert!(!ok("https://0x7f.1/"));
        assert!(!ok("https://localhost/"));
        assert!(!ok("https://user:pw@example.com/"));
        assert!(!ok("ftp://example.com/"));
        assert!(!ok("file:///etc/passwd"));

        let loose = UrlPolicy {
            allow_http: true,
            allow_ip_hosts: true,
            ports: vec![80, 443],
        };
        let ok = |s: &str| loose.allows(&Url::parse(s).unwrap());
        assert!(ok("http://example.com/favicon.ico"));
        assert!(ok("https://1.1.1.1/favicon.ico"));
        assert!(ok("https://[2606:4700:4700::1111]/"));
        assert!(!ok("https://127.0.0.1/"));
        assert!(!ok("https://[::1]/"));
        assert!(!ok("http://169.254.169.254/latest/meta-data/"));
        assert!(!ok("https://2130706433/"));
        assert!(!ok("https://example.com:6379/"));
    }
}
