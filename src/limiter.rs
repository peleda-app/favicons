use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use http::HeaderMap;
use moka::sync::Cache;

use crate::config::{ClientIp, IpSource, RateLimit};
use crate::queue::ClientKey;

pub struct ClientKeyer {
    secret: [u8; 32],
    cfg: ClientIp,
}

impl ClientKeyer {
    pub fn new(cfg: ClientIp) -> Self {
        Self {
            secret: rand::random(),
            cfg,
        }
    }

    pub fn client_ip(&self, peer: IpAddr, headers: &HeaderMap) -> IpAddr {
        let header_ip = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<IpAddr>().ok())
        };
        let picked = match self.cfg.source {
            IpSource::Peer => None,
            IpSource::XRealIp => header_ip("x-real-ip"),
            IpSource::CfConnectingIp => header_ip("cf-connecting-ip"),
            IpSource::XForwardedFor => headers
                .get_all("x-forwarded-for")
                .iter()
                .rev()
                .filter_map(|v| v.to_str().ok())
                .flat_map(|v| v.rsplit(','))
                .filter_map(|s| s.trim().parse::<IpAddr>().ok())
                .nth(self.cfg.trusted_hops.max(1) - 1),
        };
        picked.unwrap_or(peer)
    }

    pub fn key(&self, ip: IpAddr) -> ClientKey {
        let masked = mask(
            ip.to_canonical(),
            self.cfg.ipv4_prefix,
            self.cfg.ipv6_prefix,
        );
        let h = match masked {
            IpAddr::V4(v4) => blake3::keyed_hash(&self.secret, &v4.octets()),
            IpAddr::V6(v6) => blake3::keyed_hash(&self.secret, &v6.octets()),
        };
        let mut out = [0u8; 8];
        out.copy_from_slice(&h.as_bytes()[..8]);
        u64::from_le_bytes(out)
    }
}

fn mask(ip: IpAddr, v4: u8, v6: u8) -> IpAddr {
    match ip {
        IpAddr::V4(a) => {
            let bits = u32::from(a);
            let m = if v4 == 0 {
                0
            } else {
                u32::MAX << (32 - u32::from(v4.min(32)))
            };
            IpAddr::V4((bits & m).into())
        }
        IpAddr::V6(a) => {
            let bits = u128::from(a);
            let m = if v6 == 0 {
                0
            } else {
                u128::MAX << (128 - u32::from(v6.min(128)))
            };
            IpAddr::V6((bits & m).into())
        }
    }
}

struct Bucket {
    tokens: f64,
    at: Instant,
}

pub struct RateLimiter {
    buckets: Cache<ClientKey, Arc<Mutex<Bucket>>>,
    rate: f64,
    burst: f64,
}

impl RateLimiter {
    pub fn new(cfg: &RateLimit) -> Self {
        let idle = Duration::from_secs_f64(
            (cfg.burst / cfg.requests_per_second.max(0.001)).clamp(1.0, 3_600.0),
        );
        Self {
            buckets: Cache::builder()
                .max_capacity(cfg.max_tracked_clients)
                .time_to_idle(idle)
                .build(),
            rate: cfg.requests_per_second,
            burst: cfg.burst.max(1.0),
        }
    }

    pub fn check(&self, key: ClientKey) -> bool {
        if self.rate <= 0.0 {
            return true;
        }
        let now = Instant::now();
        let bucket = self.buckets.get_with(key, || {
            Arc::new(Mutex::new(Bucket {
                tokens: self.burst,
                at: now,
            }))
        });
        let Ok(mut b) = bucket.lock() else {
            return false;
        };
        let elapsed = now.saturating_duration_since(b.at).as_secs_f64();
        b.tokens = (b.tokens + elapsed * self.rate).min(self.burst);
        b.at = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyer(source: IpSource) -> ClientKeyer {
        ClientKeyer::new(ClientIp {
            source,
            ..ClientIp::default()
        })
    }

    #[test]
    fn same_subnet_shares_key() {
        let k = keyer(IpSource::Peer);
        let a = k.key("203.0.113.5".parse().unwrap());
        assert_eq!(a, k.key("203.0.113.200".parse().unwrap()));
        assert_ne!(a, k.key("203.0.114.5".parse().unwrap()));
        assert_eq!(
            k.key("2001:db8:aa:bb00::1".parse().unwrap()),
            k.key("2001:db8:aa:bbff::2".parse().unwrap())
        );
        assert_eq!(a, k.key("::ffff:203.0.113.9".parse().unwrap()));
    }

    #[test]
    fn keys_are_not_stable_across_instances() {
        let ip = "198.51.100.1".parse().unwrap();
        assert_ne!(keyer(IpSource::Peer).key(ip), keyer(IpSource::Peer).key(ip));
    }

    #[test]
    fn picks_client_ip_from_trusted_hop() {
        let mut h = HeaderMap::new();
        h.insert(
            "x-forwarded-for",
            "6.6.6.6, 1.2.3.4, 10.0.0.1".parse().unwrap(),
        );
        let peer: IpAddr = "10.0.0.2".parse().unwrap();
        let k = ClientKeyer::new(ClientIp {
            source: IpSource::XForwardedFor,
            trusted_hops: 2,
            ..ClientIp::default()
        });
        assert_eq!(k.client_ip(peer, &h), "1.2.3.4".parse::<IpAddr>().unwrap());
        h.append("x-forwarded-for", "9.9.9.9".parse().unwrap());
        assert_eq!(k.client_ip(peer, &h), "10.0.0.1".parse::<IpAddr>().unwrap());
        let mut junk = HeaderMap::new();
        junk.insert("x-forwarded-for", "nope, 5.5.5.5".parse().unwrap());
        assert_eq!(k.client_ip(peer, &junk), peer);
        assert_eq!(keyer(IpSource::Peer).client_ip(peer, &h), peer);
        assert_eq!(keyer(IpSource::CfConnectingIp).client_ip(peer, &h), peer);
    }

    #[test]
    fn token_bucket_limits_bursts() {
        let l = RateLimiter::new(&RateLimit {
            requests_per_second: 1.0,
            burst: 3.0,
            max_tracked_clients: 10,
        });
        assert!((0..3).all(|_| l.check(1)));
        assert!(!l.check(1));
        assert!(l.check(2));
    }
}
