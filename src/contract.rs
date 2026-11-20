use serde::{Deserialize, Serialize};

pub const H_SOURCE: &str = "x-icon-source";
pub const H_FALLBACK: &str = "x-icon-fallback";
pub const H_REASON: &str = "x-icon-fallback-reason";
pub const H_DOMAIN: &str = "x-icon-domain";
pub const H_STALE: &str = "x-icon-stale";
pub const H_BLOCKED: &str = "x-icon-blocked";
pub const H_STYLE: &str = "x-icon-style";
pub const H_MONO_SOURCE: &str = "x-icon-mono-source";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    Color,
    Mono,
}

impl Style {
    pub fn as_str(self) -> &'static str {
        match self {
            Style::Color => "color",
            Style::Mono => "mono",
        }
    }

    pub fn from_path(s: &str) -> Option<Style> {
        match s {
            "color" => Some(Style::Color),
            "mono" => Some(Style::Mono),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MonoSource {
    MaskIcon,
    Derived,
}

impl MonoSource {
    pub fn as_str(self) -> &'static str {
        match self {
            MonoSource::MaskIcon => "mask-icon",
            MonoSource::Derived => "derived",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Color,
    Mono,
}

impl Slot {
    pub fn as_str(self) -> &'static str {
        match self {
            Slot::Color => "color",
            Slot::Mono => "mono",
        }
    }
}

pub const H_CDN_CACHE_CONTROL: &str = "cdn-cache-control";

pub const SVG_MIME: &str = "image/svg+xml";
pub const H_VECTOR: &str = "x-icon-vector";
pub const H_UPSTREAM: &str = "x-icon-upstream";
pub const H_PROVENANCE: &str = "x-icon-provenance";
pub const H_LICENSE: &str = "x-icon-license";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provenance {
    Site,
    Bimi,
    Thesvg,
    Upstream,
}

impl Provenance {
    pub fn as_str(self) -> &'static str {
        match self {
            Provenance::Site => "site",
            Provenance::Bimi => "bimi",
            Provenance::Thesvg => "thesvg",
            Provenance::Upstream => "upstream",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Cache,
    Origin,
    Generated,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Cache => "cache",
            Source::Origin => "origin",
            Source::Generated => "generated",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackReason {
    Queued,
    Overloaded,
    NotFound,
    FetchFailed,
    DomainTooNew,
    Blocklisted,
    DnsBlocked,
    InvalidDomain,
    NoMonochrome,
}

impl FallbackReason {
    pub fn as_str(self) -> &'static str {
        match self {
            FallbackReason::Queued => "queued",
            FallbackReason::Overloaded => "overloaded",
            FallbackReason::NotFound => "not-found",
            FallbackReason::FetchFailed => "fetch-failed",
            FallbackReason::DomainTooNew => "domain-too-new",
            FallbackReason::Blocklisted => "blocklisted",
            FallbackReason::DnsBlocked => "dns-blocked",
            FallbackReason::InvalidDomain => "invalid-domain",
            FallbackReason::NoMonochrome => "no-monochrome",
        }
    }

    pub fn is_transient(self) -> bool {
        matches!(
            self,
            FallbackReason::Queued | FallbackReason::Overloaded | FallbackReason::FetchFailed
        )
    }

    pub fn blocked_by(self) -> Option<&'static str> {
        match self {
            FallbackReason::Blocklisted => Some("blocklist"),
            FallbackReason::DnsBlocked => Some("dns-filter"),
            _ => None,
        }
    }

    pub const ALL: [FallbackReason; 9] = [
        FallbackReason::Queued,
        FallbackReason::Overloaded,
        FallbackReason::NotFound,
        FallbackReason::FetchFailed,
        FallbackReason::DomainTooNew,
        FallbackReason::Blocklisted,
        FallbackReason::DnsBlocked,
        FallbackReason::InvalidDomain,
        FallbackReason::NoMonochrome,
    ];
}
