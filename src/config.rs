use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use figment::Figment;
use figment::providers::{Env, Format, Toml};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: Server,
    pub client_ip: ClientIp,
    pub rate_limit: RateLimit,
    pub fetch: Fetch,
    pub render: Render,
    pub cache: Cache,
    pub http_cache: HttpCache,
    pub policy: Policy,
    pub dns: Dns,
    pub identicon: Identicon,
    pub upstream: Upstream,
    pub bimi: Bimi,
    pub catalog: CatalogCfg,
    pub tls: Tls,
}

impl Config {
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let mut fig = Figment::new();
        if let Some(p) = path {
            fig = fig.merge(Toml::file_exact(p));
        }
        let cfg: Config = fig
            .merge(
                Env::prefixed("FAVICONS_")
                    .split("__")
                    .ignore(&["log", "log_format", "config"]),
            )
            .extract()?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> anyhow::Result<()> {
        self.tls.validate()?;
        let urls = crate::net::UrlPolicy::new(&self.fetch);
        for template in &self.upstream.providers {
            anyhow::ensure!(
                template.matches("{domain}").count() == 1,
                "upstream provider {template:?} must contain {{domain}} exactly once"
            );
            let sample = Upstream::url(template, "example.com").ok_or_else(|| {
                anyhow::anyhow!("upstream provider {template:?} is not a valid URL")
            })?;
            anyhow::ensure!(
                sample.scheme() == "https",
                "upstream provider {template:?} must use https"
            );
            anyhow::ensure!(
                urls.allows(&sample),
                "upstream provider {template:?} is blocked by fetch.allowed_ports or fetch.allow_ip_hosts"
            );
        }
        anyhow::ensure!(
            !self.fetch.allowed_ports.is_empty() && !self.fetch.allowed_ports.contains(&0),
            "fetch.allowed_ports must list at least one port, none of them 0"
        );
        let remote = self
            .policy
            .blocklists
            .iter()
            .map(|s| ("policy.blocklists", s.as_str()))
            .chain([
                (
                    "policy.rdap_bootstrap_url",
                    self.policy.rdap_bootstrap_url.as_str(),
                ),
                (
                    "catalog.thesvg.index_url",
                    self.catalog.thesvg.index_url.as_str(),
                ),
                (
                    "catalog.thesvg.asset_base",
                    self.catalog.thesvg.asset_base.as_str(),
                ),
            ])
            .filter(|(_, s)| s.starts_with("http://") || s.starts_with("https://"));
        for (key, raw) in remote {
            let ok = url::Url::parse(raw).is_ok_and(|u| urls.allows(&u));
            anyhow::ensure!(
                ok,
                "{key} {raw:?} is blocked by fetch.allow_http, fetch.allowed_ports or fetch.allow_ip_hosts"
            );
        }
        anyhow::ensure!(
            self.server.worker_threads != Some(0),
            "server.worker_threads must be > 0"
        );
        anyhow::ensure!(self.fetch.workers > 0, "fetch.workers must be > 0");
        anyhow::ensure!(
            self.fetch.queue_capacity > 0,
            "fetch.queue_capacity must be > 0"
        );
        anyhow::ensure!(
            self.render.decode_concurrency > 0,
            "render.decode_concurrency must be > 0"
        );
        anyhow::ensure!(
            self.client_ip.ipv4_prefix <= 32,
            "client_ip.ipv4_prefix must be <= 32"
        );
        anyhow::ensure!(
            self.client_ip.ipv6_prefix <= 128,
            "client_ip.ipv6_prefix must be <= 128"
        );
        anyhow::ensure!(
            self.render.max_source_dimension >= 16 && self.render.max_source_dimension <= 4096,
            "render.max_source_dimension must be within 16..=4096"
        );
        if self.cache.redirect_to_public_url {
            anyhow::ensure!(
                self.cache.public_base_url.is_some(),
                "cache.redirect_to_public_url requires cache.public_base_url"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    pub listen: SocketAddr,
    pub admin_listen: Option<SocketAddr>,
    pub max_connections: usize,
    pub max_concurrent_requests: usize,
    pub request_timeout_ms: u64,
    pub header_read_timeout_ms: u64,
    pub shutdown_grace_ms: u64,
    pub worker_threads: Option<usize>,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(([0, 0, 0, 0], 8080)),
            admin_listen: None,
            max_connections: 4096,
            max_concurrent_requests: 512,
            request_timeout_ms: 5_000,
            header_read_timeout_ms: 5_000,
            shutdown_grace_ms: 10_000,
            worker_threads: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum IpSource {
    Peer,
    XForwardedFor,
    XRealIp,
    CfConnectingIp,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClientIp {
    pub source: IpSource,
    pub trusted_hops: usize,
    pub ipv4_prefix: u8,
    pub ipv6_prefix: u8,
}

impl Default for ClientIp {
    fn default() -> Self {
        Self {
            source: IpSource::Peer,
            trusted_hops: 1,
            ipv4_prefix: 24,
            ipv6_prefix: 56,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RateLimit {
    pub requests_per_second: f64,
    pub burst: f64,
    pub max_tracked_clients: u64,
}

impl Default for RateLimit {
    fn default() -> Self {
        Self {
            requests_per_second: 50.0,
            burst: 400.0,
            max_tracked_clients: 100_000,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fetch {
    pub workers: usize,
    pub queue_capacity: usize,
    pub per_client_queue_capacity: usize,
    pub wait_ms: u64,
    pub connect_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub dns_timeout_ms: u64,
    pub job_timeout_ms: u64,
    pub max_html_bytes: usize,
    pub max_manifest_bytes: usize,
    pub max_icon_bytes: usize,
    pub max_redirects: usize,
    pub max_candidates: usize,
    pub allow_http: bool,
    pub allow_ip_hosts: bool,
    pub allowed_ports: Vec<u16>,
    pub user_agent: String,
}

impl Default for Fetch {
    fn default() -> Self {
        Self {
            workers: 8,
            queue_capacity: 10_000,
            per_client_queue_capacity: 64,
            wait_ms: 1_500,
            connect_timeout_ms: 3_000,
            request_timeout_ms: 8_000,
            dns_timeout_ms: 2_000,
            job_timeout_ms: 25_000,
            max_html_bytes: 128 * 1024,
            max_manifest_bytes: 64 * 1024,
            max_icon_bytes: 1024 * 1024,
            max_redirects: 4,
            max_candidates: 6,
            allow_http: false,
            allow_ip_hosts: false,
            allowed_ports: vec![443],
            user_agent: concat!(
                "Mozilla/5.0 (compatible; favicons/",
                env!("CARGO_PKG_VERSION"),
                "; +https://github.com/peleda-app/favicons)"
            )
            .to_string(),
        }
    }
}

impl Fetch {
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_millis(self.connect_timeout_ms)
    }
    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.request_timeout_ms)
    }
    pub fn dns_timeout(&self) -> Duration {
        Duration::from_millis(self.dns_timeout_ms)
    }
    pub fn job_timeout(&self) -> Duration {
        Duration::from_millis(self.job_timeout_ms)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SandboxMode {
    Off,
    Process,
    Socket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Render {
    pub sandbox: SandboxMode,
    pub socket_path: std::path::PathBuf,
    pub worker_memory_bytes: u64,
    pub decode_concurrency: usize,
    pub decode_timeout_ms: u64,
    pub max_source_dimension: u32,
    pub max_alloc_bytes: u64,
    pub max_svg_bytes: usize,
    pub max_svg_depth: usize,
    pub svg: bool,
    pub trace_max_px: u32,
    pub max_output_svg_bytes: usize,
    pub svg_fidelity: f32,
}

impl Default for Render {
    fn default() -> Self {
        Self {
            sandbox: SandboxMode::Process,
            socket_path: "/run/favicons/render.sock".into(),
            worker_memory_bytes: 256 * 1024 * 1024,
            decode_concurrency: 2,
            decode_timeout_ms: 5_000,
            max_source_dimension: 1024,
            max_alloc_bytes: 32 * 1024 * 1024,
            max_svg_bytes: 256 * 1024,
            max_svg_depth: 48,
            svg: true,
            trace_max_px: 256,
            max_output_svg_bytes: 48 * 1024,
            svg_fidelity: 0.02,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cache {
    pub storage: String,
    pub options: HashMap<String, String>,
    pub public_base_url: Option<String>,
    pub redirect_to_public_url: bool,
    pub ttl_secs: u64,
    pub not_found_ttl_secs: u64,
    pub fetch_failed_ttl_secs: u64,
    pub too_new_ttl_secs: u64,
    pub blocked_ttl_secs: u64,
    pub memory_max_bytes: u64,
    pub memory_meta_entries: u64,
    pub memory_ttl_secs: u64,
    pub absent_ttl_secs: u64,
    pub storage_timeout_ms: u64,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            storage: "none".into(),
            options: HashMap::new(),
            public_base_url: None,
            redirect_to_public_url: false,
            ttl_secs: 7 * 86_400,
            not_found_ttl_secs: 86_400,
            fetch_failed_ttl_secs: 3_600,
            too_new_ttl_secs: 86_400,
            blocked_ttl_secs: 86_400,
            memory_max_bytes: 64 * 1024 * 1024,
            memory_meta_entries: 100_000,
            memory_ttl_secs: 600,
            absent_ttl_secs: 60,
            storage_timeout_ms: 2_000,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheProfile {
    pub browser: String,
    pub cdn: String,
}

impl CacheProfile {
    fn new(browser: &str, cdn: &str) -> Self {
        Self {
            browser: browser.into(),
            cdn: cdn.into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HttpCache {
    pub icon: CacheProfile,
    pub redirect: CacheProfile,
    pub canonical: CacheProfile,
    pub fallback_transient: CacheProfile,
    pub fallback_stable: CacheProfile,
    pub error: CacheProfile,
}

impl Default for HttpCache {
    fn default() -> Self {
        Self {
            icon: CacheProfile::new(
                "public, max-age=604800, stale-while-revalidate=86400",
                "public, max-age=2592000, stale-while-revalidate=604800, stale-if-error=604800",
            ),
            redirect: CacheProfile::new("public, max-age=86400", "public, max-age=86400"),
            canonical: CacheProfile::new(
                "public, max-age=2592000, immutable",
                "public, max-age=31536000, immutable",
            ),
            fallback_transient: CacheProfile::new("public, max-age=300", "public, max-age=60"),
            fallback_stable: CacheProfile::new(
                "public, max-age=86400",
                "public, max-age=86400, stale-if-error=86400",
            ),
            error: CacheProfile::new("no-store", "no-store"),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UnknownAge {
    Allow,
    Block,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub collapse_private_suffixes: bool,
    pub deny_suffixes: Vec<String>,
    pub blocklists: Vec<String>,
    pub allowlist: Vec<String>,
    pub reload_secs: u64,
    pub max_list_bytes: usize,
    pub min_domain_age_days: u32,
    pub unknown_domain_age: UnknownAge,
    pub rdap_bootstrap_url: String,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            collapse_private_suffixes: true,
            deny_suffixes: vec!["onion".into(), "arpa".into(), "i2p".into()],
            blocklists: vec![],
            allowlist: vec![],
            reload_secs: 6 * 3_600,
            max_list_bytes: 64 * 1024 * 1024,
            min_domain_age_days: 30,
            unknown_domain_age: UnknownAge::Allow,
            rdap_bootstrap_url: "https://data.iana.org/rdap/dns.json".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DnsProvider {
    CloudflareSecurity,
    Cloudflare,
    Quad9,
    Google,
    System,
    Custom,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DnsProtocol {
    Https,
    Tls,
    Udp,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Dns {
    pub provider: DnsProvider,
    pub protocol: DnsProtocol,
    pub servers: Vec<std::net::IpAddr>,
    pub server_name: String,
    pub path: String,
    pub cache_size: u64,
    pub max_ttl_secs: u64,
}

impl Default for Dns {
    fn default() -> Self {
        Self {
            provider: DnsProvider::CloudflareSecurity,
            protocol: DnsProtocol::Https,
            servers: vec![],
            server_name: String::new(),
            path: "/dns-query".into(),
            cache_size: 8_192,
            max_ttl_secs: 3_600,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tls {
    pub enabled: bool,
    pub listen: SocketAddr,
    pub http_listen: Option<SocketAddr>,
    pub domains: Vec<String>,
    pub contact: Vec<String>,
    pub directory: String,
    pub directory_ca: Option<PathBuf>,
    pub state_dir: PathBuf,
    pub renew_before_days: u64,
    pub cert: Option<PathBuf>,
    pub key: Option<PathBuf>,
}

impl Default for Tls {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 443)),
            http_listen: Some(SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 80))),
            domains: vec![],
            contact: vec![],
            directory: "https://acme-v02.api.letsencrypt.org/directory".into(),
            directory_ca: None,
            state_dir: PathBuf::from("/var/lib/favicons/acme"),
            renew_before_days: 30,
            cert: None,
            key: None,
        }
    }
}

impl Tls {
    pub fn acme(&self) -> bool {
        self.enabled && self.cert.is_none()
    }

    fn validate(&self) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        anyhow::ensure!(
            self.cert.is_some() == self.key.is_some(),
            "tls.cert and tls.key go together"
        );
        if self.cert.is_some() {
            return Ok(());
        }
        anyhow::ensure!(!self.domains.is_empty(), "tls.domains is required for ACME");
        for d in &self.domains {
            anyhow::ensure!(
                crate::domain::is_valid_hostname(d) && d.contains('.'),
                "tls.domains entry {d:?} is not a hostname"
            );
        }
        for c in &self.contact {
            anyhow::ensure!(
                c.starts_with("mailto:"),
                "tls.contact entry {c:?} must start with mailto:"
            );
        }
        anyhow::ensure!(
            self.directory.starts_with("https://"),
            "tls.directory must be an https URL"
        );
        anyhow::ensure!(
            self.renew_before_days > 0,
            "tls.renew_before_days must be > 0"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Bimi {
    pub enabled: bool,
}

impl Default for Bimi {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct CatalogCfg {
    pub thesvg: TheSvg,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TheSvg {
    pub enabled: bool,
    pub index_url: String,
    pub asset_base: String,
    pub licenses: Vec<String>,
    pub collections: Vec<String>,
    pub reload_secs: u64,
    pub max_index_bytes: usize,
}

impl Default for TheSvg {
    fn default() -> Self {
        Self {
            enabled: false,
            index_url: "https://raw.githubusercontent.com/GLINCKER/thesvg/main/src/data/icons.json"
                .into(),
            asset_base: "https://raw.githubusercontent.com/GLINCKER/thesvg/main/public/".into(),
            licenses: vec!["CC0-1.0".into()],
            collections: vec!["brands".into()],
            reload_secs: 86_400,
            max_index_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Upstream {
    pub providers: Vec<String>,
}

impl Upstream {
    pub fn url(template: &str, domain: &str) -> Option<url::Url> {
        url::Url::parse(&template.replacen("{domain}", domain, 1)).ok()
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Identicon {
    pub salt: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_providers(providers: &[&str]) -> Config {
        Config {
            upstream: Upstream {
                providers: providers.iter().map(|s| s.to_string()).collect(),
            },
            ..Config::default()
        }
    }

    #[test]
    fn rejects_remote_sources_blocked_by_url_policy() {
        Config::default().validate().unwrap();
        assert!(
            with_providers(&["https://icons.example:8443/{domain}"])
                .validate()
                .is_err()
        );
        assert!(
            with_providers(&["https://203.0.113.10/{domain}"])
                .validate()
                .is_err()
        );
        let mut cfg = Config::default();
        cfg.policy.blocklists = vec!["/etc/favicons/block.txt".into()];
        cfg.validate().unwrap();
        cfg.policy.blocklists = vec!["http://lists.example/block.txt".into()];
        assert!(cfg.validate().is_err());
        cfg.fetch.allow_http = true;
        assert!(cfg.validate().is_err());
        cfg.fetch.allowed_ports = vec![80, 443];
        cfg.validate().unwrap();
        cfg.fetch.allowed_ports = vec![];
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validates_upstream_templates() {
        with_providers(&[
            "https://favicon.vemetric.com/{domain}?size=256&format=webp",
            "https://icons.duckduckgo.com/ip3/{domain}.ico",
            "https://www.google.com/s2/favicons?domain={domain}&sz=64",
        ])
        .validate()
        .unwrap();
        assert!(
            with_providers(&["https://icons.example/static.ico"])
                .validate()
                .is_err()
        );
        assert!(
            with_providers(&["http://icons.example/{domain}"])
                .validate()
                .is_err()
        );
        assert!(
            with_providers(&["https://x/{domain}/{domain}"])
                .validate()
                .is_err()
        );
        assert!(with_providers(&["not a url {domain}"]).validate().is_err());
        assert_eq!(
            Upstream::url(
                "https://icons.duckduckgo.com/ip3/{domain}.ico",
                "github.com"
            )
            .unwrap()
            .as_str(),
            "https://icons.duckduckgo.com/ip3/github.com.ico"
        );
    }
}
