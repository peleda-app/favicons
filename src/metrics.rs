use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::contract::FallbackReason;

#[derive(Default)]
pub struct Stats {
    pub requests: AtomicU64,
    pub served_cache: AtomicU64,
    pub canonical_redirects: AtomicU64,
    pub rate_limited: AtomicU64,
    pub shed: AtomicU64,
    pub fetch_ok: AtomicU64,
    pub fetch_not_found: AtomicU64,
    pub fetch_failed: AtomicU64,
    pub fetch_too_new: AtomicU64,
    pub fetch_blocked: AtomicU64,
    pub decode_errors: AtomicU64,
    fallback: [AtomicU64; FallbackReason::ALL.len()],
}

pub fn inc(c: &AtomicU64) {
    c.fetch_add(1, Ordering::Relaxed);
}

impl Stats {
    pub fn fallback(&self, r: FallbackReason) {
        if let Some(i) = FallbackReason::ALL.iter().position(|x| *x == r) {
            inc(&self.fallback[i]);
        }
    }

    pub fn render(
        &self,
        queue_len: usize,
        running: usize,
        blocked: usize,
        catalog: usize,
    ) -> String {
        let g = |c: &AtomicU64| c.load(Ordering::Relaxed);
        let mut s = String::new();
        let counters = [
            ("favicons_requests_total", g(&self.requests)),
            ("favicons_served_cache_total", g(&self.served_cache)),
            (
                "favicons_canonical_redirects_total",
                g(&self.canonical_redirects),
            ),
            ("favicons_rate_limited_total", g(&self.rate_limited)),
            ("favicons_shed_total", g(&self.shed)),
            ("favicons_fetch_ok_total", g(&self.fetch_ok)),
            ("favicons_fetch_not_found_total", g(&self.fetch_not_found)),
            ("favicons_fetch_failed_total", g(&self.fetch_failed)),
            ("favicons_fetch_too_new_total", g(&self.fetch_too_new)),
            ("favicons_fetch_blocked_total", g(&self.fetch_blocked)),
            ("favicons_decode_errors_total", g(&self.decode_errors)),
        ];
        for (name, v) in counters {
            let _ = writeln!(s, "# TYPE {name} counter\n{name} {v}");
        }
        let _ = writeln!(s, "# TYPE favicons_fallback_total counter");
        for (i, r) in FallbackReason::ALL.iter().enumerate() {
            let _ = writeln!(
                s,
                "favicons_fallback_total{{reason=\"{}\"}} {}",
                r.as_str(),
                g(&self.fallback[i])
            );
        }
        for (name, v) in [
            ("favicons_queue_length", queue_len),
            ("favicons_fetch_running", running),
            ("favicons_blocklist_entries", blocked),
            ("favicons_catalog_entries", catalog),
        ] {
            let _ = writeln!(s, "# TYPE {name} gauge\n{name} {v}");
        }
        s
    }
}
