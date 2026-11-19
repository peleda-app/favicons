use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use tokio::sync::{Notify, watch};

pub type ClientKey = u64;

pub enum Enqueue {
    Pending(watch::Receiver<bool>),
    Rejected,
}

#[derive(Default)]
struct Inner {
    buckets: HashMap<ClientKey, VecDeque<String>>,
    round_robin: VecDeque<ClientKey>,
    queued: HashMap<String, watch::Sender<bool>>,
    running: HashMap<String, watch::Sender<bool>>,
    len: usize,
}

pub struct FairQueue {
    inner: Mutex<Inner>,
    notify: Notify,
    capacity: usize,
    per_client: usize,
}

impl FairQueue {
    pub fn new(capacity: usize, per_client: usize) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            notify: Notify::new(),
            capacity,
            per_client: per_client.max(1),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map_or(0, |g| g.len)
    }

    pub fn running(&self) -> usize {
        self.inner.lock().map_or(0, |g| g.running.len())
    }

    pub fn push(&self, domain: &str, client: ClientKey) -> Enqueue {
        let Ok(mut g) = self.inner.lock() else {
            return Enqueue::Rejected;
        };
        if let Some(tx) = g.queued.get(domain).or_else(|| g.running.get(domain)) {
            return Enqueue::Pending(tx.subscribe());
        }
        let mine = g.buckets.get(&client).map_or(0, VecDeque::len);
        if mine >= self.per_client {
            return Enqueue::Rejected;
        }
        if g.len >= self.capacity && !evict_from_heaviest(&mut g, mine) {
            return Enqueue::Rejected;
        }
        let (tx, rx) = watch::channel(false);
        g.queued.insert(domain.to_string(), tx);
        let bucket = g.buckets.entry(client).or_default();
        bucket.push_back(domain.to_string());
        if bucket.len() == 1 {
            g.round_robin.push_back(client);
        }
        g.len += 1;
        drop(g);
        self.notify.notify_one();
        Enqueue::Pending(rx)
    }

    pub async fn pop(&self) -> String {
        loop {
            if let Some(d) = self.try_pop() {
                return d;
            }
            self.notify.notified().await;
        }
    }

    fn try_pop(&self) -> Option<String> {
        let mut g = self.inner.lock().ok()?;
        while let Some(client) = g.round_robin.pop_front() {
            let Some(bucket) = g.buckets.get_mut(&client) else {
                continue;
            };
            let Some(domain) = bucket.pop_front() else {
                g.buckets.remove(&client);
                continue;
            };
            if bucket.is_empty() {
                g.buckets.remove(&client);
            } else {
                g.round_robin.push_back(client);
            }
            g.len -= 1;
            if let Some(tx) = g.queued.remove(&domain) {
                g.running.insert(domain.clone(), tx);
            }
            return Some(domain);
        }
        None
    }

    pub fn complete(&self, domain: &str) {
        let tx = self
            .inner
            .lock()
            .ok()
            .and_then(|mut g| g.running.remove(domain));
        if let Some(tx) = tx {
            let _ = tx.send(true);
        }
    }
}

fn evict_from_heaviest(g: &mut Inner, mine: usize) -> bool {
    let Some((&victim, len)) = g
        .buckets
        .iter()
        .map(|(k, b)| (k, b.len()))
        .max_by_key(|(_, l)| *l)
    else {
        return false;
    };
    if len <= mine + 1 {
        return false;
    }
    let Some(bucket) = g.buckets.get_mut(&victim) else {
        return false;
    };
    let Some(dropped) = bucket.pop_back() else {
        return false;
    };
    if bucket.is_empty() {
        g.buckets.remove(&victim);
        g.round_robin.retain(|k| *k != victim);
    }
    g.len -= 1;
    if let Some(tx) = g.queued.remove(&dropped) {
        let _ = tx.send(true);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(e: Enqueue) -> bool {
        matches!(e, Enqueue::Pending(_))
    }

    #[test]
    fn dedupes_domains() {
        let q = FairQueue::new(10, 10);
        assert!(pending(q.push("a.com", 1)));
        assert!(pending(q.push("a.com", 2)));
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn round_robins_between_clients() {
        let q = FairQueue::new(100, 100);
        for i in 0..10 {
            q.push(&format!("spam{i}.com"), 1);
        }
        q.push("honest.com", 2);
        assert_eq!(q.try_pop().as_deref(), Some("spam0.com"));
        assert_eq!(q.try_pop().as_deref(), Some("honest.com"));
        assert_eq!(q.try_pop().as_deref(), Some("spam1.com"));
    }

    #[test]
    fn caps_each_client() {
        let q = FairQueue::new(100, 3);
        for i in 0..3 {
            assert!(pending(q.push(&format!("{i}.com"), 1)));
        }
        assert!(!pending(q.push("4.com", 1)));
        assert!(pending(q.push("4.com", 2)));
    }

    #[test]
    fn evicts_heaviest_client_when_full() {
        let q = FairQueue::new(5, 100);
        let mut spam_rx = Vec::new();
        for i in 0..5 {
            if let Enqueue::Pending(rx) = q.push(&format!("spam{i}.com"), 1) {
                spam_rx.push(rx);
            }
        }
        assert!(pending(q.push("honest.com", 2)));
        assert_eq!(q.len(), 5);
        assert!(*spam_rx[4].borrow(), "evicted waiter must be released");
        let q2 = FairQueue::new(2, 100);
        q2.push("a.com", 1);
        q2.push("b.com", 2);
        assert!(
            !pending(q2.push("c.com", 3)),
            "no client is heavier, so reject"
        );
    }

    #[tokio::test]
    async fn completion_wakes_waiters() {
        let q = FairQueue::new(10, 10);
        let Enqueue::Pending(mut rx) = q.push("a.com", 1) else {
            panic!()
        };
        let d = q.pop().await;
        assert!(pending(q.push("a.com", 9)), "running domain still dedupes");
        q.complete(&d);
        assert!(rx.wait_for(|v| *v).await.is_ok());
        assert_eq!(q.running(), 0);
    }
}
