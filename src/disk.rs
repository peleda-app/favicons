use std::collections::BinaryHeap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use tokio::sync::Notify;

const BLOCK: u64 = 4096;
const TOUCH_AFTER: Duration = Duration::from_secs(86_400);
const CANDIDATES: usize = 8192;
const RECENT: Duration = Duration::from_secs(60);
const LOW_WATER: f64 = 0.9;

pub struct Disk {
    root: PathBuf,
    max_bytes: u64,
    max_entries: u64,
    bytes: AtomicU64,
    entries: AtomicU64,
    known: AtomicBool,
    pub wake: Notify,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Sweep {
    pub bytes: u64,
    pub entries: u64,
    pub evicted: Vec<String>,
}

impl Disk {
    pub fn new(root: PathBuf, max_bytes: u64, max_entries: u64) -> Option<Self> {
        (max_bytes > 0 || max_entries > 0).then(|| Self {
            root,
            max_bytes,
            max_entries,
            bytes: AtomicU64::new(0),
            entries: AtomicU64::new(0),
            known: AtomicBool::new(false),
            wake: Notify::new(),
        })
    }

    pub fn exists(&self, domain: &str) -> bool {
        self.root.join(domain).is_dir()
    }

    pub fn added(&self, sizes: impl IntoIterator<Item = usize>, new_entry: bool) {
        let mut bytes: u64 = sizes.into_iter().map(|n| allocated(n as u64)).sum();
        if new_entry {
            bytes += BLOCK;
            self.entries.fetch_add(1, Ordering::Relaxed);
        }
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        if self.over() {
            self.wake.notify_one();
        }
    }

    fn over(&self) -> bool {
        self.known.load(Ordering::Relaxed)
            && exceeds(
                self.bytes.load(Ordering::Relaxed),
                self.entries.load(Ordering::Relaxed),
                self.max_bytes,
                self.max_entries,
            )
    }

    pub fn touch(&self, domain: &str) {
        let path = self.root.join(domain).join("meta.json");
        let Ok(file) = fs::File::open(&path) else {
            return;
        };
        let stale = file
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > TOUCH_AFTER);
        if stale {
            let _ = file.set_modified(SystemTime::now());
        }
    }

    pub fn sweep(&self) -> Sweep {
        let mut total = Sweep::default();
        let mut oldest: BinaryHeap<(SystemTime, u64, String)> = BinaryHeap::new();
        let Ok(dir) = fs::read_dir(&self.root) else {
            self.store(&total);
            return total;
        };
        let now = SystemTime::now();
        for entry in dir.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') || !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Some((used, seen)) = measure(&entry.path()) else {
                continue;
            };
            total.bytes += used;
            total.entries += 1;
            if now.duration_since(seen).is_ok_and(|age| age < RECENT) {
                continue;
            }
            oldest.push((seen, used, name));
            if oldest.len() > CANDIDATES {
                oldest.pop();
            }
        }
        if exceeds(total.bytes, total.entries, self.max_bytes, self.max_entries) {
            let goal_bytes = low_water(self.max_bytes);
            let goal_entries = low_water(self.max_entries);
            for (_, used, name) in oldest.into_sorted_vec() {
                if total.bytes <= goal_bytes && total.entries <= goal_entries {
                    break;
                }
                if fs::remove_dir_all(self.root.join(&name)).is_ok() {
                    total.bytes = total.bytes.saturating_sub(used);
                    total.entries = total.entries.saturating_sub(1);
                    total.evicted.push(name);
                }
            }
        }
        self.store(&total);
        total
    }

    fn store(&self, s: &Sweep) {
        self.bytes.store(s.bytes, Ordering::Relaxed);
        self.entries.store(s.entries, Ordering::Relaxed);
        self.known.store(true, Ordering::Relaxed);
    }

    pub fn needs_sweep(&self) -> bool {
        self.over()
    }
}

fn exceeds(bytes: u64, entries: u64, max_bytes: u64, max_entries: u64) -> bool {
    (max_bytes > 0 && bytes > max_bytes) || (max_entries > 0 && entries > max_entries)
}

fn low_water(max: u64) -> u64 {
    if max == 0 {
        u64::MAX
    } else {
        (max as f64 * LOW_WATER) as u64
    }
}

fn allocated(len: u64) -> u64 {
    len.div_ceil(BLOCK).max(1) * BLOCK
}

fn measure(dir: &Path) -> Option<(u64, SystemTime)> {
    let md = fs::metadata(dir).ok()?;
    let mut used = on_disk(&md);
    let mut seen = md.modified().ok()?;
    let mut meta_time = None;
    for f in fs::read_dir(dir).ok()?.flatten() {
        let Ok(md) = f.metadata() else { continue };
        used += on_disk(&md);
        if let Ok(t) = md.modified() {
            seen = seen.max(t);
            if f.file_name() == "meta.json" {
                meta_time = Some(t);
            }
        }
    }
    Some((used, meta_time.unwrap_or(seen)))
}

#[cfg(unix)]
fn on_disk(md: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    md.blocks() * 512
}

#[cfg(not(unix))]
fn on_disk(md: &fs::Metadata) -> u64 {
    allocated(md.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(root: &Path, name: &str, age_days: u64, size: usize) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        for file in ["meta.json", "color.svg"] {
            let path = dir.join(file);
            fs::write(&path, vec![b'x'; size]).unwrap();
            let t = SystemTime::now() - Duration::from_secs(age_days * 86_400 + 3_600);
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
    }

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("favicons-disk-{}", rand::random::<u64>()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn evicts_least_recently_used_down_to_low_water() {
        let root = tmp();
        for i in 0..10u64 {
            entry(&root, &format!("d{i}.com"), 10 - i, 100);
        }
        let disk = Disk::new(root.clone(), 0, 5).unwrap();
        let s = disk.sweep();
        assert_eq!(s.entries, 4);
        assert_eq!(
            s.evicted,
            ["d0.com", "d1.com", "d2.com", "d3.com", "d4.com", "d5.com"]
        );
        assert!(!root.join("d0.com").exists());
        assert!(root.join("d9.com").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn counts_allocated_bytes_and_leaves_room_alone() {
        let root = tmp();
        for i in 0..4 {
            entry(&root, &format!("d{i}.com"), 3, 10);
        }
        let disk = Disk::new(root.clone(), 1 << 30, 0).unwrap();
        let s = disk.sweep();
        assert_eq!(s.entries, 4);
        assert!(s.evicted.is_empty());
        assert!(s.bytes >= 4 * 2 * 512, "{}", s.bytes);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn touch_marks_an_entry_as_recently_used() {
        let root = tmp();
        entry(&root, "old.com", 30, 10);
        entry(&root, "newer.com", 20, 10);
        let disk = Disk::new(root.clone(), 0, 1).unwrap();
        disk.touch("old.com");
        let s = disk.sweep();
        assert_eq!(s.evicted, ["newer.com"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn wakes_the_sweeper_only_when_over_the_limit() {
        let root = tmp();
        let disk = Disk::new(root.clone(), 3 * BLOCK, 0).unwrap();
        disk.sweep();
        disk.added([10, 10], true);
        assert!(!disk.needs_sweep());
        disk.added([10], false);
        assert!(disk.needs_sweep());
        assert!(Disk::new(root.clone(), 0, 0).is_none());
        let _ = fs::remove_dir_all(root);
    }
}
