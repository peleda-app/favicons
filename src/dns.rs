use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use hickory_resolver::TokioResolver;
use hickory_resolver::config::{
    ConnectionConfig, NameServerConfig, ResolverConfig, ResolverOpts, ServerGroup,
};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use crate::config::{Dns as DnsCfg, DnsProtocol, DnsProvider};
use crate::net::is_public_ip;

const CLOUDFLARE_SECURITY: ServerGroup<'static> = ServerGroup {
    ips: &[
        IpAddr::V4(Ipv4Addr::new(1, 1, 1, 2)),
        IpAddr::V4(Ipv4Addr::new(1, 0, 0, 2)),
        IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1112)),
        IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1002)),
    ],
    server_name: "security.cloudflare-dns.com",
    path: "/dns-query",
};

const QUAD9: ServerGroup<'static> = ServerGroup {
    ips: &[
        IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9)),
        IpAddr::V4(Ipv4Addr::new(149, 112, 112, 112)),
        IpAddr::V6(Ipv6Addr::new(0x2620, 0xfe, 0, 0, 0, 0, 0, 0xfe)),
        IpAddr::V6(Ipv6Addr::new(0x2620, 0xfe, 0, 0, 0, 0, 0, 0x9)),
    ],
    server_name: "dns.quad9.net",
    path: "/dns-query",
};

#[derive(Debug, thiserror::Error)]
pub enum DnsError {
    #[error("blocked by the filtering resolver")]
    Blocked,
    #[error("resolves only to non-public addresses")]
    NonPublic,
    #[error("no such host")]
    NotFound,
    #[error("dns timeout")]
    Timeout,
    #[error("dns failure: {0}")]
    Failed(String),
}

pub struct SafeDns {
    resolver: TokioResolver,
    timeout: Duration,
}

impl SafeDns {
    pub fn new(cfg: &DnsCfg, timeout: Duration) -> anyhow::Result<Arc<Self>> {
        let mut builder = match cfg.provider {
            DnsProvider::System => {
                TokioResolver::builder_tokio().context("reading system DNS config")?
            }
            _ => TokioResolver::builder_with_config(
                resolver_config(cfg)?,
                TokioRuntimeProvider::default(),
            ),
        };
        let opts: &mut ResolverOpts = builder.options_mut();
        opts.timeout = timeout;
        opts.attempts = 2;
        opts.cache_size = cfg.cache_size;
        opts.positive_max_ttl = Some(Duration::from_secs(cfg.max_ttl_secs));
        let resolver = builder.build().context("building DNS resolver")?;
        Ok(Arc::new(Self { resolver, timeout }))
    }

    pub async fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, DnsError> {
        let fqdn = format!("{}.", host.trim_end_matches('.'));
        let lookup = tokio::time::timeout(self.timeout * 3, self.resolver.lookup_ip(fqdn))
            .await
            .map_err(|_| DnsError::Timeout)?;
        let ips: Vec<IpAddr> = match lookup {
            Ok(l) => l.iter().collect(),
            Err(e) if e.is_nx_domain() || e.is_no_records_found() => {
                return Err(DnsError::NotFound);
            }
            Err(e) => return Err(DnsError::Failed(e.to_string())),
        };
        classify(ips)
    }
}

pub fn classify(ips: Vec<IpAddr>) -> Result<Vec<IpAddr>, DnsError> {
    if ips.is_empty() {
        return Err(DnsError::NotFound);
    }
    if ips.iter().all(|ip| ip.is_unspecified()) {
        return Err(DnsError::Blocked);
    }
    let public: Vec<IpAddr> = ips.into_iter().filter(|ip| is_public_ip(*ip)).collect();
    if public.is_empty() {
        return Err(DnsError::NonPublic);
    }
    Ok(public)
}

fn resolver_config(cfg: &DnsCfg) -> anyhow::Result<ResolverConfig> {
    let group = match cfg.provider {
        DnsProvider::CloudflareSecurity => CLOUDFLARE_SECURITY,
        DnsProvider::Cloudflare => hickory_resolver::config::CLOUDFLARE,
        DnsProvider::Quad9 => QUAD9,
        DnsProvider::Google => hickory_resolver::config::GOOGLE,
        DnsProvider::Custom => {
            anyhow::ensure!(
                !cfg.servers.is_empty(),
                "dns.servers is required for provider = custom"
            );
            let name: Arc<str> = Arc::from(cfg.server_name.as_str());
            let path: Arc<str> = Arc::from(cfg.path.as_str());
            let servers = cfg
                .servers
                .iter()
                .map(|ip| {
                    let conn = match cfg.protocol {
                        DnsProtocol::Https => {
                            ConnectionConfig::https(name.clone(), Some(path.clone()))
                        }
                        DnsProtocol::Tls => ConnectionConfig::tls(name.clone()),
                        DnsProtocol::Udp => ConnectionConfig::udp(),
                    };
                    NameServerConfig::new(*ip, true, vec![conn])
                })
                .collect();
            if cfg.protocol != DnsProtocol::Udp {
                anyhow::ensure!(
                    !cfg.server_name.is_empty(),
                    "dns.server_name is required for encrypted custom DNS"
                );
            }
            return Ok(ResolverConfig::from_name_servers(servers));
        }
        DnsProvider::System => unreachable!("handled by caller"),
    };
    Ok(match cfg.protocol {
        DnsProtocol::Https => ResolverConfig::https(&group),
        DnsProtocol::Tls => ResolverConfig::tls(&group),
        DnsProtocol::Udp => ResolverConfig::udp_and_tcp(&group),
    })
}

pub struct ReqwestResolver(pub Arc<SafeDns>);

impl Resolve for ReqwestResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let dns = self.0.clone();
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let ips = dns.resolve(&host).await?;
            let addrs: Vec<SocketAddr> = ips.into_iter().map(|ip| SocketAddr::new(ip, 0)).collect();
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ips(list: &[&str]) -> Vec<IpAddr> {
        list.iter().map(|s| s.parse().unwrap()).collect()
    }

    #[test]
    fn recognises_sinkhole_answers() {
        assert!(matches!(
            classify(ips(&["0.0.0.0"])),
            Err(DnsError::Blocked)
        ));
        assert!(matches!(
            classify(ips(&["0.0.0.0", "::"])),
            Err(DnsError::Blocked)
        ));
        assert!(matches!(
            classify(ips(&["10.0.0.1"])),
            Err(DnsError::NonPublic)
        ));
        assert!(matches!(classify(vec![]), Err(DnsError::NotFound)));
        assert_eq!(
            classify(ips(&["10.0.0.1", "1.1.1.1"])).unwrap(),
            ips(&["1.1.1.1"])
        );
    }

    #[tokio::test]
    #[ignore = "needs network"]
    async fn cloudflare_security_sinkholes_malware() {
        let dns = SafeDns::new(&DnsCfg::default(), Duration::from_secs(3)).unwrap();
        assert!(matches!(
            dns.resolve("malware.testcategory.com").await,
            Err(DnsError::Blocked)
        ));
        assert!(dns.resolve("example.com").await.is_ok());
        assert!(matches!(
            dns.resolve("nope-zzzq-xx.com").await,
            Err(DnsError::NotFound)
        ));
    }

    #[test]
    fn builds_every_preset() {
        for provider in [
            DnsProvider::CloudflareSecurity,
            DnsProvider::Cloudflare,
            DnsProvider::Quad9,
            DnsProvider::Google,
        ] {
            for protocol in [DnsProtocol::Https, DnsProtocol::Tls, DnsProtocol::Udp] {
                let cfg = DnsCfg {
                    provider,
                    protocol,
                    ..DnsCfg::default()
                };
                assert!(!resolver_config(&cfg).unwrap().name_servers().is_empty());
            }
        }
        let bad = DnsCfg {
            provider: DnsProvider::Custom,
            ..DnsCfg::default()
        };
        assert!(resolver_config(&bad).is_err());
    }
}
