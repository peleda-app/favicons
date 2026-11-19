use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;
use serde::Deserialize;
use tokio::sync::RwLock;
use url::Url;

use crate::config::{Policy as PolicyCfg, UnknownAge};
use crate::net::HttpClient;
use crate::store::now;

const BOOTSTRAP_MAX: usize = 1024 * 1024;
const RDAP_MAX: usize = 256 * 1024;

#[derive(Deserialize)]
struct Bootstrap {
    services: Vec<(Vec<String>, Vec<String>)>,
}

#[derive(Deserialize)]
struct RdapDomain {
    #[serde(default)]
    events: Vec<RdapEvent>,
}

#[derive(Deserialize)]
struct RdapEvent {
    #[serde(rename = "eventAction")]
    action: String,
    #[serde(rename = "eventDate")]
    date: String,
}

type ServerMap = Arc<HashMap<String, Url>>;

pub struct DomainAge {
    client: HttpClient,
    min_days: u32,
    unknown: UnknownAge,
    bootstrap_url: String,
    servers: RwLock<Option<(u64, ServerMap)>>,
    cache: Cache<String, Option<u64>>,
}

impl DomainAge {
    pub fn new(client: HttpClient, cfg: &PolicyCfg) -> Option<Self> {
        (cfg.min_domain_age_days > 0).then(|| Self {
            client,
            min_days: cfg.min_domain_age_days,
            unknown: cfg.unknown_domain_age,
            bootstrap_url: cfg.rdap_bootstrap_url.clone(),
            servers: RwLock::new(None),
            cache: Cache::builder()
                .max_capacity(200_000)
                .time_to_live(Duration::from_secs(86_400))
                .build(),
        })
    }

    pub async fn is_too_new(&self, domain: &str) -> bool {
        let created = match self.cache.get(domain).await {
            Some(v) => v,
            None => {
                let v = self.lookup(domain).await;
                self.cache.insert(domain.to_string(), v).await;
                v
            }
        };
        match created {
            Some(day) => days_since_epoch(now()).saturating_sub(day) < u64::from(self.min_days),
            None => self.unknown == UnknownAge::Block,
        }
    }

    async fn lookup(&self, domain: &str) -> Option<u64> {
        let tld = domain.rsplit('.').next()?;
        let base = self.servers().await?.get(tld)?.clone();
        let url = base.join(&format!("domain/{domain}")).ok()?;
        let resp = self
            .client
            .get(&url, "application/rdap+json, application/json", RDAP_MAX)
            .await
            .ok()?;
        let rdap: RdapDomain = serde_json::from_slice(&resp.bytes).ok()?;
        rdap.events
            .iter()
            .find(|e| e.action.eq_ignore_ascii_case("registration"))
            .and_then(|e| parse_day(&e.date))
    }

    async fn servers(&self) -> Option<Arc<HashMap<String, Url>>> {
        if let Some((at, map)) = self.servers.read().await.as_ref()
            && now().saturating_sub(*at) < 86_400
        {
            return Some(map.clone());
        }
        let mut guard = self.servers.write().await;
        if let Some((at, map)) = guard.as_ref()
            && now().saturating_sub(*at) < 300
        {
            return Some(map.clone());
        }
        let url = Url::parse(&self.bootstrap_url).ok()?;
        let fetched = self
            .client
            .get(&url, "application/json", BOOTSTRAP_MAX)
            .await;
        let parsed = fetched
            .ok()
            .and_then(|r| serde_json::from_slice::<Bootstrap>(&r.bytes).ok());
        let Some(boot) = parsed else {
            tracing::warn!("rdap bootstrap unavailable");
            let stale = guard.as_ref().map(|(_, m)| m.clone());
            *guard = Some((
                now().saturating_sub(86_400 - 300),
                stale.clone().unwrap_or_default(),
            ));
            return stale;
        };
        let mut map = HashMap::new();
        for (tlds, urls) in boot.services {
            let Some(base) = urls
                .iter()
                .find(|u| u.starts_with("https://"))
                .and_then(|u| {
                    let u = if u.ends_with('/') {
                        u.clone()
                    } else {
                        format!("{u}/")
                    };
                    Url::parse(&u).ok()
                })
            else {
                continue;
            };
            for t in tlds {
                map.insert(t.to_ascii_lowercase(), base.clone());
            }
        }
        let map = Arc::new(map);
        *guard = Some((now(), map.clone()));
        Some(map)
    }
}

fn days_since_epoch(secs: u64) -> u64 {
    secs / 86_400
}

pub fn parse_day(s: &str) -> Option<u64> {
    let date = s.get(..10)?;
    let mut parts = date.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    u64::try_from(era * 146_097 + doe - 719_468).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339_day() {
        assert_eq!(parse_day("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_day("2000-03-01T12:34:56Z"), Some(11_017));
        assert_eq!(parse_day("2026-11-19"), Some(20_776));
        assert_eq!(parse_day("garbage"), None);
        assert_eq!(parse_day("2026-13-01"), None);
    }

    #[test]
    fn parses_rdap_events() {
        let json = br#"{"events":[{"eventAction":"last changed","eventDate":"2024-01-01T00:00:00Z"},{"eventAction":"registration","eventDate":"1995-08-14T04:00:00Z"}]}"#;
        let d: RdapDomain = serde_json::from_slice(json).unwrap();
        let reg = d
            .events
            .iter()
            .find(|e| e.action == "registration")
            .unwrap();
        assert_eq!(parse_day(&reg.date), Some(9_356));
    }
}
