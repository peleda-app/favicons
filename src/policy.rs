use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use serde::Deserialize;

use crate::config::Policy as PolicyCfg;
use crate::domain::{is_valid_hostname, parent_chain};
use crate::net::{HttpClient, read_source};

#[derive(Default)]
struct Lists {
    blocked: HashSet<String>,
    aliases: HashMap<String, String>,
}

pub struct Policy {
    cfg: PolicyCfg,
    allow: HashSet<String>,
    lists: RwLock<Arc<Lists>>,
}

impl Policy {
    pub fn new(cfg: PolicyCfg) -> Self {
        let allow = cfg
            .allowlist
            .iter()
            .map(|d| d.trim().to_ascii_lowercase())
            .collect();
        Self {
            cfg,
            allow,
            lists: RwLock::new(Arc::new(Lists::default())),
        }
    }

    fn lists(&self) -> Arc<Lists> {
        self.lists.read().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn canonical(&self, registrable: &str) -> String {
        if let Some(target) = self.cfg.alias_overrides.get(registrable) {
            return target.clone();
        }
        self.lists()
            .aliases
            .get(registrable)
            .cloned()
            .unwrap_or_else(|| registrable.to_string())
    }

    pub fn is_blocked(&self, host: &str, canonical: &str) -> bool {
        let lists = self.lists();
        let allowed = |d: &str| parent_chain(d).any(|p| self.allow.contains(p));
        if allowed(host) || allowed(canonical) {
            return false;
        }
        parent_chain(host)
            .chain(parent_chain(canonical))
            .any(|d| lists.blocked.contains(d))
    }

    pub fn counts(&self) -> (usize, usize) {
        let l = self.lists();
        (l.blocked.len(), l.aliases.len())
    }

    pub async fn reload(&self, client: &HttpClient) {
        let current = self.lists();
        let mut blocked = HashSet::new();
        let mut blocked_ok = true;
        for src in &self.cfg.blocklists {
            match read_source(client, src, self.cfg.max_list_bytes).await {
                Ok(bytes) => blocked.extend(parse_blocklist(&String::from_utf8_lossy(&bytes))),
                Err(e) => {
                    blocked_ok = false;
                    tracing::warn!(source = %src, error = %e, "blocklist load failed");
                }
            }
        }
        let mut aliases = HashMap::new();
        let mut aliases_ok = true;
        for src in &self.cfg.aliases {
            match read_source(client, src, self.cfg.max_list_bytes).await {
                Ok(bytes) => match parse_equivalent_domains(&bytes, &self.cfg.alias_preferred) {
                    Ok(map) => aliases.extend(map),
                    Err(e) => {
                        aliases_ok = false;
                        tracing::warn!(source = %src, error = %e, "alias list parse failed");
                    }
                },
                Err(e) => {
                    aliases_ok = false;
                    tracing::warn!(source = %src, error = %e, "alias list load failed");
                }
            }
        }
        let next = Lists {
            blocked: if blocked_ok || current.blocked.is_empty() {
                blocked
            } else {
                current.blocked.clone()
            },
            aliases: if aliases_ok || current.aliases.is_empty() {
                aliases
            } else {
                current.aliases.clone()
            },
        };
        tracing::info!(
            blocked = next.blocked.len(),
            aliases = next.aliases.len(),
            "policy lists loaded"
        );
        if let Ok(mut g) = self.lists.write() {
            *g = Arc::new(next);
        }
    }
}

pub fn parse_blocklist(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines().filter_map(|line| {
        let line = line.split('#').next()?.split('!').next()?.trim();
        if line.is_empty() {
            return None;
        }
        let token = line.split_ascii_whitespace().last()?;
        let token = token
            .trim_start_matches("||")
            .trim_start_matches("*.")
            .trim_end_matches('^')
            .trim_end_matches('.');
        let d = token.to_ascii_lowercase();
        is_valid_hostname(&d).then_some(d)
    })
}

#[derive(Deserialize)]
struct EquivalentGroup {
    domains: Vec<String>,
    #[serde(default)]
    excluded: bool,
}

pub fn parse_equivalent_domains(
    json: &[u8],
    preferred: &[String],
) -> anyhow::Result<HashMap<String, String>> {
    let groups: Vec<EquivalentGroup> = serde_json::from_slice(json)?;
    let mut map = HashMap::new();
    for g in groups.into_iter().filter(|g| !g.excluded) {
        let domains: Vec<String> = g
            .domains
            .into_iter()
            .map(|d| d.trim().to_ascii_lowercase())
            .filter(|d| is_valid_hostname(d))
            .collect();
        let primary = preferred
            .iter()
            .find(|p| domains.contains(p))
            .or_else(|| domains.first());
        let Some(primary) = primary.cloned() else {
            continue;
        };
        for d in domains.into_iter().filter(|d| *d != primary) {
            map.entry(d).or_insert_with(|| primary.clone());
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_blocklist_formats() {
        let text = "# comment\n0.0.0.0 evil.example\n127.0.0.1\tads.example.org # trailing\n||track.example.net^\nplain.example.com\n*.wild.example\n! adblock comment\n0.0.0.0 localhost\n";
        let got: HashSet<String> = parse_blocklist(text).collect();
        let want: HashSet<String> = [
            "evil.example",
            "ads.example.org",
            "track.example.net",
            "plain.example.com",
            "wild.example",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn parses_vaultwarden_equivalent_domains() {
        let json = br#"[{"type":1,"domains":["microsoft.com","live.com","office.com"],"excluded":false},{"type":2,"domains":["a.com","b.com"],"excluded":true}]"#;
        let map = parse_equivalent_domains(json, &[]).unwrap();
        assert_eq!(
            map.get("live.com").map(String::as_str),
            Some("microsoft.com")
        );
        assert_eq!(
            map.get("office.com").map(String::as_str),
            Some("microsoft.com")
        );
        assert!(!map.contains_key("microsoft.com"));
        assert!(!map.contains_key("b.com"));
        let bing_first = br#"[{"domains":["bing.com","live.com","microsoft.com"]}]"#;
        let map = parse_equivalent_domains(bing_first, &["microsoft.com".into()]).unwrap();
        assert_eq!(
            map.get("bing.com").map(String::as_str),
            Some("microsoft.com")
        );
        assert_eq!(
            map.get("live.com").map(String::as_str),
            Some("microsoft.com")
        );
        assert!(!map.contains_key("microsoft.com"));
    }

    #[test]
    fn blocks_parents_and_honours_allowlist() {
        let p = Policy::new(PolicyCfg {
            allowlist: vec!["good.example.com".into()],
            ..PolicyCfg::default()
        });
        *p.lists.write().unwrap() = Arc::new(Lists {
            blocked: ["example.com".to_string(), "bad.org".to_string()]
                .into_iter()
                .collect(),
            aliases: HashMap::new(),
        });
        assert!(p.is_blocked("x.example.com", "example.com"));
        assert!(p.is_blocked("bad.org", "bad.org"));
        assert!(!p.is_blocked("good.example.com", "example.com"));
        assert!(!p.is_blocked("fine.net", "fine.net"));
    }
}
