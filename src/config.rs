use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
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
}

impl Config {
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let mut fig = Figment::new();
        if let Some(p) = path {
            fig = fig.merge(Toml::file_exact(p));
        }
        let mut cfg: Config = fig
            .merge(
                Env::prefixed("FAVICONS_")
                    .split("__")
                    .ignore(&["log", "log_format", "config"]),
            )
            .extract()?;
        cfg.render.sizes.sort_unstable();
        cfg.render.sizes.dedup();
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.render.sizes.is_empty()
                && self
                    .render
                    .sizes
                    .iter()
                    .all(|s| crate::contract::SIZES.contains(s)),
            "render.sizes must be a non-empty subset of {:?}",
            crate::contract::SIZES
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
            allow_http: true,
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
    pub sizes: Vec<u32>,
    pub decode_concurrency: usize,
    pub decode_timeout_ms: u64,
    pub max_source_dimension: u32,
    pub max_alloc_bytes: u64,
    pub max_svg_bytes: usize,
    pub max_svg_depth: usize,
    pub svg: bool,
    pub format: crate::contract::OutputFormat,
}

impl Default for Render {
    fn default() -> Self {
        Self {
            sandbox: SandboxMode::Process,
            socket_path: "/run/favicons/render.sock".into(),
            worker_memory_bytes: 256 * 1024 * 1024,
            sizes: crate::contract::SIZES.to_vec(),
            decode_concurrency: 2,
            decode_timeout_ms: 3_000,
            max_source_dimension: 1024,
            max_alloc_bytes: 32 * 1024 * 1024,
            max_svg_bytes: 256 * 1024,
            max_svg_depth: 48,
            svg: true,
            format: crate::contract::OutputFormat::Webp,
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
    pub aliases: Vec<String>,
    pub alias_preferred: Vec<String>,
    pub alias_overrides: HashMap<String, String>,
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
            aliases: vec![
                "https://raw.githubusercontent.com/dani-garcia/vaultwarden/main/src/static/global_domains.json"
                    .into(),
            ],
            alias_preferred: [
                "microsoft.com",
                "google.com",
                "apple.com",
                "amazon.com",
                "yahoo.com",
            ]
            .map(String::from)
            .to_vec(),
            alias_overrides: HashMap::new(),
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

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Identicon {
    pub salt: String,
}
