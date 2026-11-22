use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, NewAccount,
    NewOrder, OrderStatus, RetryPolicy,
};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use tokio::sync::watch;

use crate::config::Tls as TlsCfg;

const MAX_TOKEN_LEN: usize = 128;

#[derive(Default)]
pub struct Challenges(RwLock<HashMap<String, String>>);

impl Challenges {
    pub fn get(&self, token: &str) -> Option<String> {
        if token.is_empty()
            || token.len() > MAX_TOKEN_LEN
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return None;
        }
        self.0.read().ok()?.get(token).cloned()
    }

    fn set(&self, token: String, key_authorization: String) {
        if let Ok(mut m) = self.0.write() {
            m.insert(token, key_authorization);
        }
    }

    fn clear(&self) {
        if let Ok(mut m) = self.0.write() {
            m.clear();
        }
    }
}

pub struct CertStore {
    current: RwLock<Arc<CertifiedKey>>,
    not_after: RwLock<Option<SystemTime>>,
}

impl fmt::Debug for CertStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CertStore").finish_non_exhaustive()
    }
}

impl ResolvesServerCert for CertStore {
    fn resolve(&self, _hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        self.current.read().ok().map(|c| c.clone())
    }
}

impl CertStore {
    fn new(key: CertifiedKey, not_after: Option<SystemTime>) -> Arc<Self> {
        Arc::new(Self {
            current: RwLock::new(Arc::new(key)),
            not_after: RwLock::new(not_after),
        })
    }

    fn replace(&self, key: CertifiedKey, not_after: Option<SystemTime>) {
        if let Ok(mut c) = self.current.write() {
            *c = Arc::new(key);
        }
        if let Ok(mut n) = self.not_after.write() {
            *n = not_after;
        }
    }

    fn not_after(&self) -> Option<SystemTime> {
        self.not_after.read().ok().and_then(|n| *n)
    }
}

pub fn server_config(certs: Arc<CertStore>) -> anyhow::Result<rustls::ServerConfig> {
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .context("tls protocol versions")?
    .with_no_client_auth()
    .with_cert_resolver(certs);
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

fn certified_key(chain_pem: &str, key_pem: &str) -> anyhow::Result<CertifiedKey> {
    let chain = CertificateDer::pem_slice_iter(chain_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .context("parsing certificate chain")?;
    anyhow::ensure!(!chain.is_empty(), "certificate chain is empty");
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).context("parsing private key")?;
    let signer = rustls::crypto::ring::sign::any_supported_type(&key)
        .context("unsupported private key type")?;
    Ok(CertifiedKey::new(chain, signer))
}

fn not_after(chain_pem: &str) -> Option<SystemTime> {
    let der = CertificateDer::pem_slice_iter(chain_pem.as_bytes())
        .next()?
        .ok()?;
    let (_, cert) = x509_parser::parse_x509_certificate(&der).ok()?;
    let secs = u64::try_from(cert.validity().not_after.timestamp()).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(secs))
}

fn covers(chain_pem: &str, domains: &[String]) -> bool {
    let Some(Ok(der)) = CertificateDer::pem_slice_iter(chain_pem.as_bytes()).next() else {
        return false;
    };
    let Ok((_, cert)) = x509_parser::parse_x509_certificate(&der) else {
        return false;
    };
    let names: Vec<String> = cert
        .subject_alternative_name()
        .ok()
        .flatten()
        .map(|san| {
            san.value
                .general_names
                .iter()
                .filter_map(|n| match n {
                    x509_parser::extensions::GeneralName::DNSName(d) => {
                        Some(d.to_ascii_lowercase())
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    domains
        .iter()
        .all(|d| names.iter().any(|n| n.eq_ignore_ascii_case(d)))
}

fn self_signed(domains: &[String]) -> anyhow::Result<CertifiedKey> {
    let names = if domains.is_empty() {
        vec!["localhost".to_string()]
    } else {
        domains.to_vec()
    };
    let cert =
        rcgen::generate_simple_self_signed(names).context("generating self-signed certificate")?;
    certified_key(&cert.cert.pem(), &cert.signing_key.serialize_pem())
}

struct Paths {
    dir: PathBuf,
}

impl Paths {
    fn new(cfg: &TlsCfg) -> Self {
        let host = url::Url::parse(&cfg.directory)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_else(|| "acme".into());
        let tag: String = host
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Self {
            dir: cfg.state_dir.join(tag),
        }
    }

    fn account(&self) -> PathBuf {
        self.dir.join("account.json")
    }

    fn chain(&self) -> PathBuf {
        self.dir.join("cert.pem")
    }

    fn key(&self) -> PathBuf {
        self.dir.join("key.pem")
    }
}

fn write_private(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut f = opts
        .open(&tmp)
        .with_context(|| format!("writing {}", tmp.display()))?;
    f.write_all(data)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

pub struct Tls {
    pub certs: Arc<CertStore>,
    pub challenges: Arc<Challenges>,
}

impl Tls {
    pub fn load(cfg: &TlsCfg, challenges: Arc<Challenges>) -> anyhow::Result<Self> {
        if let (Some(cert), Some(key)) = (&cfg.cert, &cfg.key) {
            let chain = std::fs::read_to_string(cert)
                .with_context(|| format!("reading {}", cert.display()))?;
            let key = std::fs::read_to_string(key)
                .with_context(|| format!("reading {}", key.display()))?;
            return Ok(Self {
                certs: CertStore::new(certified_key(&chain, &key)?, not_after(&chain)),
                challenges,
            });
        }
        let paths = Paths::new(cfg);
        let stored = std::fs::read_to_string(paths.chain())
            .ok()
            .zip(std::fs::read_to_string(paths.key()).ok())
            .and_then(|(chain, key)| {
                let ck = certified_key(&chain, &key).ok()?;
                let expiry = if covers(&chain, &cfg.domains) {
                    not_after(&chain)
                } else {
                    tracing::info!(domains = ?cfg.domains, "stored certificate misses a configured domain; reissuing");
                    None
                };
                Some((ck, expiry))
            });
        let certs = match stored {
            Some((ck, expiry)) => {
                tracing::info!(dir = %paths.dir.display(), "loaded stored certificate");
                CertStore::new(ck, expiry)
            }
            None => {
                tracing::info!("no certificate yet; serving a self-signed one until ACME succeeds");
                CertStore::new(self_signed(&cfg.domains)?, None)
            }
        };
        Ok(Self { certs, challenges })
    }

    pub fn spawn_acme(&self, cfg: TlsCfg, mut shutdown: watch::Receiver<bool>) {
        let certs = self.certs.clone();
        let challenges = self.challenges.clone();
        tokio::spawn(async move {
            let paths = Paths::new(&cfg);
            let renew_before = Duration::from_secs(cfg.renew_before_days * 86_400);
            let mut backoff = Duration::from_secs(60);
            loop {
                let wait = match certs.not_after() {
                    Some(expiry) => expiry
                        .checked_sub(renew_before)
                        .and_then(|due| due.duration_since(SystemTime::now()).ok())
                        .unwrap_or_default(),
                    None => Duration::ZERO,
                };
                if !wait.is_zero() {
                    let nap = wait.min(Duration::from_secs(12 * 3600));
                    tokio::select! {
                        _ = tokio::time::sleep(nap) => continue,
                        _ = shutdown.changed() => return,
                    }
                }
                let result = issue(&cfg, &paths, &challenges).await;
                challenges.clear();
                match result {
                    Ok((chain, key)) => match certified_key(&chain, &key) {
                        Ok(ck) => {
                            let expiry = not_after(&chain);
                            certs.replace(ck, expiry);
                            backoff = Duration::from_secs(60);
                            tracing::info!(
                                domains = ?cfg.domains,
                                expires = expiry.and_then(|e| e.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()),
                                "certificate issued"
                            );
                        }
                        Err(e) => tracing::error!(error = %e, "issued certificate is unusable"),
                    },
                    Err(e) => {
                        tracing::warn!(error = %format!("{e:#}"), retry_secs = backoff.as_secs(), "ACME order failed");
                        tokio::select! {
                            _ = tokio::time::sleep(backoff) => {}
                            _ = shutdown.changed() => return,
                        }
                        backoff = (backoff * 2).min(Duration::from_secs(6 * 3600));
                    }
                }
            }
        });
    }
}

async fn account(cfg: &TlsCfg, paths: &Paths) -> anyhow::Result<Account> {
    let builder = || match &cfg.directory_ca {
        Some(ca) => Account::builder_with_root(ca),
        None => Account::builder(),
    };
    if let Ok(raw) = std::fs::read(paths.account()) {
        let creds: AccountCredentials =
            serde_json::from_slice(&raw).context("reading stored ACME account")?;
        return builder()?
            .from_credentials(creds)
            .await
            .context("restoring ACME account");
    }
    let contact: Vec<&str> = cfg.contact.iter().map(String::as_str).collect();
    let (account, creds) = builder()?
        .create(
            &NewAccount {
                contact: &contact,
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            cfg.directory.clone(),
            None,
        )
        .await
        .context("creating ACME account")?;
    std::fs::create_dir_all(&paths.dir)
        .with_context(|| format!("creating {}", paths.dir.display()))?;
    write_private(&paths.account(), &serde_json::to_vec(&creds)?)?;
    tracing::info!(id = account.id(), "ACME account created");
    Ok(account)
}

async fn issue(
    cfg: &TlsCfg,
    paths: &Paths,
    challenges: &Challenges,
) -> anyhow::Result<(String, String)> {
    let account = account(cfg, paths).await?;
    let identifiers: Vec<Identifier> = cfg.domains.iter().cloned().map(Identifier::Dns).collect();
    let mut order = account
        .new_order(&NewOrder::new(&identifiers))
        .await
        .context("creating ACME order")?;

    let mut authorizations = order.authorizations();
    while let Some(result) = authorizations.next().await {
        let mut authz = result.context("fetching authorization")?;
        match authz.status {
            AuthorizationStatus::Pending => {}
            AuthorizationStatus::Valid => continue,
            other => bail!("authorization is {other:?}"),
        }
        let mut challenge = authz
            .challenge(ChallengeType::Http01)
            .context("the ACME server offered no http-01 challenge")?;
        challenges.set(
            challenge.token.clone(),
            challenge.key_authorization().as_str().to_owned(),
        );
        challenge
            .set_ready()
            .await
            .context("marking challenge ready")?;
    }

    let retries = RetryPolicy::new()
        .initial_delay(Duration::from_secs(1))
        .timeout(Duration::from_secs(120));
    let status = order
        .poll_ready(&retries)
        .await
        .context("waiting for validation")?;
    if status != OrderStatus::Ready {
        let detail = order
            .refresh()
            .await
            .ok()
            .and_then(|s| s.error.as_ref().and_then(|e| e.detail.clone()))
            .unwrap_or_default();
        bail!("order is {status:?} {detail}");
    }
    let key_pem = order.finalize().await.context("finalizing order")?;
    let chain_pem = order
        .poll_certificate(&retries)
        .await
        .context("downloading certificate")?;

    std::fs::create_dir_all(&paths.dir)
        .with_context(|| format!("creating {}", paths.dir.display()))?;
    write_private(&paths.key(), key_pem.as_bytes())?;
    write_private(&paths.chain(), chain_pem.as_bytes())?;
    Ok((chain_pem, key_pem))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_tokens_are_checked() {
        let c = Challenges::default();
        c.set("abc-DEF_123".into(), "abc-DEF_123.thumb".into());
        assert_eq!(c.get("abc-DEF_123").as_deref(), Some("abc-DEF_123.thumb"));
        assert_eq!(c.get("missing"), None);
        assert_eq!(c.get("../etc/passwd"), None);
        assert_eq!(c.get(""), None);
        assert_eq!(c.get(&"a".repeat(MAX_TOKEN_LEN + 1)), None);
        c.clear();
        assert_eq!(c.get("abc-DEF_123"), None);
    }

    #[test]
    fn self_signed_certificate_loads_and_has_expiry() {
        let names = vec!["favicon.example.com".to_string()];
        let cert = rcgen::generate_simple_self_signed(names.clone()).unwrap();
        let chain = cert.cert.pem();
        let ck = certified_key(&chain, &cert.signing_key.serialize_pem()).unwrap();
        assert_eq!(ck.cert.len(), 1);
        let expiry = not_after(&chain).unwrap();
        assert!(expiry > SystemTime::now());
        assert!(self_signed(&names).is_ok());
    }

    #[test]
    fn a_stored_certificate_must_cover_every_configured_domain() {
        let cert =
            rcgen::generate_simple_self_signed(vec!["favicons.example.com".to_string()]).unwrap();
        let pem = cert.cert.pem();
        assert!(covers(&pem, &["favicons.example.com".into()]));
        assert!(!covers(
            &pem,
            &[
                "favicons.example.com".into(),
                "private.favicons.example.com".into()
            ]
        ));
    }

    #[test]
    fn state_is_kept_per_directory() {
        let mut cfg = TlsCfg {
            state_dir: PathBuf::from("/var/lib/favicons/acme"),
            ..TlsCfg::default()
        };
        assert_eq!(
            Paths::new(&cfg).chain(),
            PathBuf::from("/var/lib/favicons/acme/acme-v02.api.letsencrypt.org/cert.pem")
        );
        cfg.directory = "https://acme-staging-v02.api.letsencrypt.org/directory".into();
        assert_eq!(
            Paths::new(&cfg).account(),
            PathBuf::from(
                "/var/lib/favicons/acme/acme-staging-v02.api.letsencrypt.org/account.json"
            )
        );
    }
}
