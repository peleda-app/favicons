use serde::{Deserialize, Serialize};

pub const H_SOURCE: &str = "x-icon-source";
pub const H_FALLBACK: &str = "x-icon-fallback";
pub const H_REASON: &str = "x-icon-fallback-reason";
pub const H_DOMAIN: &str = "x-icon-domain";
pub const H_SIZE: &str = "x-icon-size";
pub const H_THEME: &str = "x-icon-theme";
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

    pub fn parse(s: Option<&str>) -> Style {
        match s.map(str::to_ascii_lowercase).as_deref() {
            Some("mono" | "monochrome" | "mask") => Style::Mono,
            _ => Style::Color,
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
    Light,
    Dark,
    Mono,
}

impl Slot {
    pub fn as_str(self) -> &'static str {
        match self {
            Slot::Light => "light",
            Slot::Dark => "dark",
            Slot::Mono => "mono",
        }
    }
}

impl From<Theme> for Slot {
    fn from(t: Theme) -> Slot {
        match t {
            Theme::Light => Slot::Light,
            Theme::Dark => Slot::Dark,
        }
    }
}
pub const H_CDN_CACHE_CONTROL: &str = "cdn-cache-control";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Webp,
    Png,
}

impl OutputFormat {
    pub fn ext(self) -> &'static str {
        match self {
            OutputFormat::Webp => "webp",
            OutputFormat::Png => "png",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            OutputFormat::Webp => "image/webp",
            OutputFormat::Png => "image/png",
        }
    }
}

pub const SIZES: [u32; 4] = [16, 32, 64, 128];
pub const DEFAULT_SIZE: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn other(self) -> Theme {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        }
    }

    pub fn parse(s: Option<&str>) -> Theme {
        match s.map(str::to_ascii_lowercase).as_deref() {
            Some("dark") => Theme::Dark,
            _ => Theme::Light,
        }
    }
}

pub fn snap_size(requested: Option<&str>, allowed: &[u32]) -> u32 {
    let n = requested
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(DEFAULT_SIZE);
    let allowed = if allowed.is_empty() {
        &SIZES[..]
    } else {
        allowed
    };
    allowed
        .iter()
        .copied()
        .filter(|s| *s <= n)
        .max()
        .or_else(|| allowed.iter().copied().min())
        .unwrap_or(DEFAULT_SIZE)
}

pub fn pick_available(available: &[u32], requested: u32) -> Option<u32> {
    available
        .iter()
        .copied()
        .filter(|s| *s <= requested)
        .max()
        .or_else(|| available.iter().copied().min())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_to_largest_supported_not_above_request() {
        assert_eq!(snap_size(None, &SIZES), 32);
        assert_eq!(snap_size(Some("garbage"), &SIZES), 32);
        assert_eq!(snap_size(Some("1"), &SIZES), 16);
        assert_eq!(snap_size(Some("16"), &SIZES), 16);
        assert_eq!(snap_size(Some("48"), &SIZES), 32);
        assert_eq!(snap_size(Some("128"), &SIZES), 128);
        assert_eq!(snap_size(Some("4096"), &SIZES), 128);
    }

    #[test]
    fn snaps_into_configured_sizes() {
        assert_eq!(snap_size(None, &[128]), 128);
        assert_eq!(snap_size(Some("16"), &[128]), 128);
        assert_eq!(snap_size(Some("64"), &[32, 128]), 32);
        assert_eq!(snap_size(Some("16"), &[32, 128]), 32);
        assert_eq!(snap_size(Some("512"), &[32, 128]), 128);
        assert_eq!(snap_size(None, &[64]), 64);
    }

    #[test]
    fn falls_back_to_smaller_size() {
        assert_eq!(pick_available(&[16, 32], 128), Some(32));
        assert_eq!(pick_available(&[16, 32, 64, 128], 64), Some(64));
        assert_eq!(pick_available(&[64, 128], 16), Some(64));
        assert_eq!(pick_available(&[], 16), None);
    }
}
