use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use crate::config::Policy as PolicyCfg;
use crate::domain::{is_valid_hostname, parent_chain};
use crate::net::{HttpClient, read_source};

pub struct Policy {
    cfg: PolicyCfg,
    allow: HashSet<String>,
    blocked: RwLock<Arc<HashSet<String>>>,
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
            blocked: RwLock::new(Arc::new(HashSet::new())),
        }
    }

    fn blocked(&self) -> Arc<HashSet<String>> {
        self.blocked.read().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn is_blocked(&self, host: &str, registrable: &str) -> bool {
        let blocked = self.blocked();
        let allowed = |d: &str| parent_chain(d).any(|p| self.allow.contains(p));
        if allowed(host) || allowed(registrable) {
            return false;
        }
        parent_chain(host)
            .chain(parent_chain(registrable))
            .any(|d| blocked.contains(d))
    }

    pub fn blocked_count(&self) -> usize {
        self.blocked().len()
    }

    pub async fn reload(&self, client: &HttpClient) {
        let mut next = HashSet::new();
        for src in &self.cfg.blocklists {
            match read_source(client, src, self.cfg.max_list_bytes).await {
                Ok(bytes) => next.extend(parse_blocklist(&String::from_utf8_lossy(&bytes))),
                Err(e) => {
                    tracing::warn!(source = %src, error = %e, "blocklist load failed; keeping previous list");
                    return;
                }
            }
        }
        tracing::info!(blocked = next.len(), "blocklists loaded");
        if let Ok(mut g) = self.blocked.write() {
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
    fn blocks_parents_and_honours_allowlist() {
        let p = Policy::new(PolicyCfg {
            allowlist: vec!["good.example.com".into()],
            ..PolicyCfg::default()
        });
        *p.blocked.write().unwrap() = Arc::new(
            ["example.com".to_string(), "bad.org".to_string()]
                .into_iter()
                .collect(),
        );
        assert!(p.is_blocked("x.example.com", "example.com"));
        assert!(p.is_blocked("bad.org", "bad.org"));
        assert!(!p.is_blocked("good.example.com", "example.com"));
        assert!(!p.is_blocked("fine.net", "fine.net"));
    }
}
