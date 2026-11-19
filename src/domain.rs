use url::{Host, Url};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    pub host: String,
    pub registrable: String,
}

pub const MAX_INPUT_LEN: usize = 1024;

pub fn normalize(
    input: &str,
    collapse_private: bool,
    deny_suffixes: &[String],
) -> Option<Normalized> {
    let input = input.trim();
    if input.is_empty()
        || input.len() > MAX_INPUT_LEN
        || input.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }
    let url = if input.contains("://") {
        Url::parse(input).ok()?
    } else {
        Url::parse(&format!("http://{input}")).ok()?
    };
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = match url.host()? {
        Host::Domain(d) => d.trim_end_matches('.').to_ascii_lowercase(),
        Host::Ipv4(_) | Host::Ipv6(_) => return None,
    };
    if !is_valid_hostname(&host) {
        return None;
    }
    let registrable = registrable_domain(&host, collapse_private)?;
    let tld = registrable.rsplit('.').next()?;
    if deny_suffixes
        .iter()
        .any(|s| s.eq_ignore_ascii_case(tld) || registrable.ends_with(&format!(".{s}")))
    {
        return None;
    }
    Some(Normalized { host, registrable })
}

pub fn is_valid_hostname(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    let mut labels = 0;
    for label in host.split('.') {
        labels += 1;
        let b = label.as_bytes();
        if b.is_empty() || b.len() > 63 || b[0] == b'-' || b[b.len() - 1] == b'-' {
            return false;
        }
        if !b
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        {
            return false;
        }
    }
    labels >= 2
        && !host
            .rsplit('.')
            .next()
            .is_some_and(|tld| tld.bytes().all(|c| c.is_ascii_digit()))
}

fn registrable_domain(host: &str, collapse_private: bool) -> Option<String> {
    let suffix = psl::suffix(host.as_bytes())?;
    if !suffix.is_known() {
        return None;
    }
    let private = suffix.typ() == Some(psl::Type::Private);
    if private && collapse_private {
        return icann_registrable(host);
    }
    match psl::domain(host.as_bytes()) {
        Some(d) => std::str::from_utf8(d.as_bytes()).ok().map(str::to_string),
        None if private => Some(host.to_string()),
        None => None,
    }
}

fn icann_registrable(host: &str) -> Option<String> {
    let labels: Vec<&str> = host.split('.').collect();
    let n = labels.len();
    let icann_len = (1..n)
        .filter(|k| {
            let cand = labels[n - k..].join(".");
            psl::suffix(cand.as_bytes()).is_some_and(|s| {
                s.typ() == Some(psl::Type::Icann) && s.as_bytes() == cand.as_bytes()
            })
        })
        .max()?;
    Some(labels[n - icann_len - 1..].join("."))
}

pub fn parent_chain(host: &str) -> impl Iterator<Item = &str> {
    std::iter::successors(Some(host), |h| h.split_once('.').map(|(_, rest)| rest))
        .filter(|h| h.contains('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(s: &str) -> Option<String> {
        normalize(s, true, &["onion".into(), "arpa".into()]).map(|n| n.registrable)
    }

    #[test]
    fn collapses_to_registrable_domain() {
        assert_eq!(reg("example.com").as_deref(), Some("example.com"));
        assert_eq!(
            reg("unique-spammer-id.example.com").as_deref(),
            Some("example.com")
        );
        assert_eq!(reg("a.b.c.example.co.uk").as_deref(), Some("example.co.uk"));
        assert_eq!(
            reg("https://WWW.Example.COM:8443/path?q=1#x").as_deref(),
            Some("example.com")
        );
        assert_eq!(reg("example.com.").as_deref(), Some("example.com"));
        assert_eq!(reg("github.io").as_deref(), Some("github.io"));
        assert_eq!(reg("co.uk"), None);
    }

    #[test]
    fn collapses_private_suffixes_when_enabled() {
        assert_eq!(reg("tracker-123.github.io").as_deref(), Some("github.io"));
        let keep = normalize("someone.github.io", false, &[]).map(|n| n.registrable);
        assert_eq!(keep.as_deref(), Some("someone.github.io"));
    }

    #[test]
    fn converts_idn_to_punycode() {
        assert_eq!(reg("пример.рф").as_deref(), Some("xn--e1afmkfd.xn--p1ai"));
    }

    #[test]
    fn rejects_unsafe_or_meaningless_input() {
        for bad in [
            "",
            "localhost",
            "127.0.0.1",
            "[::1]",
            "10.0.0.1",
            "0x7f.0.0.1",
            "2130706433",
            "foo.internal",
            "printer.local",
            "abc.onion",
            "1.0.0.127.in-addr.arpa",
            "com",
            "ftp://example.com",
            "file:///etc/passwd",
            "exa mple.com",
            "-bad-.com",
            "example.notarealtld",
        ] {
            assert_eq!(reg(bad), None, "{bad} must be rejected");
        }
        assert_eq!(reg(&"a".repeat(2000)), None);
    }

    #[test]
    fn walks_parent_chain() {
        let v: Vec<_> = parent_chain("a.b.example.com").collect();
        assert_eq!(v, ["a.b.example.com", "b.example.com", "example.com"]);
    }
}
