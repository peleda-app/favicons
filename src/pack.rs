use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, bail, ensure};
use bytes::Bytes;

const MAGIC: &[u8; 8] = b"FVPACK1\n";
const RECORD: usize = 18;

pub struct Pack {
    file: Arc<File>,
    count: usize,
    index: Vec<u8>,
    keys: Vec<u8>,
    len: u64,
}

impl Pack {
    pub fn open(path: &Path, layout: &str) -> anyhow::Result<Self> {
        let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let len = file.metadata()?.len();
        let mut head = [0u8; 9];
        file.read_exact(&mut head)?;
        ensure!(
            &head[..8] == MAGIC,
            "{} is not a favicons pack",
            path.display()
        );
        let mut name = vec![0u8; usize::from(head[8])];
        file.read_exact(&mut name)?;
        ensure!(
            name == layout.as_bytes(),
            "{} holds layout {}, expected {layout}",
            path.display(),
            String::from_utf8_lossy(&name)
        );
        let mut sizes = [0u8; 8];
        file.read_exact(&mut sizes)?;
        let count = u32::from_le_bytes(sizes[..4].try_into()?) as usize;
        let keys_len = u32::from_le_bytes(sizes[4..].try_into()?) as usize;
        let mut index = vec![0u8; count.checked_mul(RECORD).context("pack too large")?];
        file.read_exact(&mut index)?;
        let mut keys = vec![0u8; keys_len];
        file.read_exact(&mut keys)?;
        let pack = Self {
            file: Arc::new(file),
            count,
            index,
            keys,
            len,
        };
        for i in 0..count {
            let (key, off, size) = pack.record(i).context("corrupt pack index")?;
            ensure!(
                off.checked_add(u64::from(size))
                    .is_some_and(|end| end <= len),
                "corrupt pack data"
            );
            if i > 0 && pack.record(i - 1).is_some_and(|(prev, _, _)| prev >= key) {
                bail!("pack index is not sorted");
            }
        }
        Ok(pack)
    }

    pub fn len(&self) -> usize {
        self.count
    }

    fn record(&self, i: usize) -> Option<(&[u8], u64, u32)> {
        let r = self.index.get(i * RECORD..(i + 1) * RECORD)?;
        let key_off = u32::from_le_bytes(r[0..4].try_into().ok()?) as usize;
        let key_len = usize::from(u16::from_le_bytes(r[4..6].try_into().ok()?));
        let off = u64::from_le_bytes(r[6..14].try_into().ok()?);
        let size = u32::from_le_bytes(r[14..18].try_into().ok()?);
        let key = self.keys.get(key_off..key_off.checked_add(key_len)?)?;
        Some((key, off, size))
    }

    fn find(&self, key: &str) -> Option<(u64, u32)> {
        let (mut lo, mut hi) = (0usize, self.count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let (k, off, size) = self.record(mid)?;
            match k.cmp(key.as_bytes()) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some((off, size)),
            }
        }
        None
    }

    pub async fn get(&self, key: &str) -> anyhow::Result<Option<Bytes>> {
        let Some((off, size)) = self.find(key) else {
            return Ok(None);
        };
        debug_assert!(off + u64::from(size) <= self.len);
        let file = self.file.clone();
        let bytes = tokio::task::spawn_blocking(move || {
            let mut buf = vec![0u8; size as usize];
            file.read_exact_at(&mut buf, off).map(|()| buf)
        })
        .await??;
        Ok(Some(Bytes::from(bytes)))
    }
}

pub fn write(root: &Path, layout: &str, out: &Path) -> anyhow::Result<usize> {
    let base = root.join(layout);
    let mut entries = Vec::new();
    for dir in std::fs::read_dir(&base).with_context(|| format!("reading {}", base.display()))? {
        let dir = dir?;
        if !dir.file_type()?.is_dir() {
            continue;
        }
        let domain = dir
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("non-utf8 entry"))?;
        for file in std::fs::read_dir(dir.path())? {
            let file = file?;
            if !file.file_type()?.is_file() {
                continue;
            }
            let name = file
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("non-utf8 file"))?;
            entries.push((format!("{domain}/{name}"), file.path()));
        }
    }
    entries.sort();
    ensure!(entries.len() <= u32::MAX as usize, "too many entries");
    ensure!(layout.len() <= 255, "layout name too long");

    let keys_len: usize = entries.iter().map(|(k, _)| k.len()).sum();
    ensure!(keys_len <= u32::MAX as usize, "keys too large");
    let header = MAGIC.len() + 1 + layout.len() + 8;
    let mut data_off = (header + entries.len() * RECORD + keys_len) as u64;

    let tmp = out.with_extension("pack.tmp");
    let mut w = BufWriter::new(File::create(&tmp)?);
    w.write_all(MAGIC)?;
    w.write_all(&[layout.len() as u8])?;
    w.write_all(layout.as_bytes())?;
    w.write_all(&(entries.len() as u32).to_le_bytes())?;
    w.write_all(&(keys_len as u32).to_le_bytes())?;
    let sizes: Vec<u64> = entries
        .iter()
        .map(|(_, p)| std::fs::metadata(p).map(|m| m.len()))
        .collect::<Result<_, _>>()?;
    let mut key_off = 0u32;
    for ((key, _), size) in entries.iter().zip(&sizes) {
        ensure!(key.len() <= usize::from(u16::MAX), "key too long: {key}");
        ensure!(*size <= u64::from(u32::MAX), "file too large: {key}");
        w.write_all(&key_off.to_le_bytes())?;
        w.write_all(&(key.len() as u16).to_le_bytes())?;
        w.write_all(&data_off.to_le_bytes())?;
        w.write_all(&(*size as u32).to_le_bytes())?;
        key_off += key.len() as u32;
        data_off += size;
    }
    for (key, _) in &entries {
        w.write_all(key.as_bytes())?;
    }
    for ((key, path), size) in entries.iter().zip(&sizes) {
        let data = std::fs::read(path)?;
        ensure!(data.len() as u64 == *size, "{key} changed while packing");
        w.write_all(&data)?;
    }
    w.into_inner().map_err(|e| e.into_error())?.sync_all()?;
    std::fs::rename(&tmp, out)?;
    Ok(entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn packs_a_tree_and_reads_it_back() {
        let root = std::env::temp_dir().join(format!("favicons-pack-{}", rand::random::<u64>()));
        for (domain, name, body) in [
            ("github.com", "meta.json", &b"{\"a\":1}"[..]),
            ("github.com", "color.svg.gz", &b"\x1f\x8bcolor"[..]),
            ("@selfhst.jellyfin", "meta.json", &b"{}"[..]),
            ("example.org", "mono.svg", &b""[..]),
        ] {
            std::fs::create_dir_all(root.join("v4").join(domain)).unwrap();
            std::fs::write(root.join("v4").join(domain).join(name), body).unwrap();
        }
        let out = root.join("seed.pack");
        assert_eq!(write(&root, "v4", &out).unwrap(), 4);

        let pack = Pack::open(&out, "v4").unwrap();
        assert_eq!(pack.len(), 4);
        assert_eq!(
            pack.get("github.com/meta.json")
                .await
                .unwrap()
                .unwrap()
                .as_ref(),
            b"{\"a\":1}"
        );
        assert_eq!(
            pack.get("github.com/color.svg.gz")
                .await
                .unwrap()
                .unwrap()
                .as_ref(),
            b"\x1f\x8bcolor"
        );
        assert_eq!(
            pack.get("@selfhst.jellyfin/meta.json")
                .await
                .unwrap()
                .unwrap()
                .as_ref(),
            b"{}"
        );
        assert_eq!(
            pack.get("example.org/mono.svg")
                .await
                .unwrap()
                .unwrap()
                .as_ref(),
            b""
        );
        assert!(pack.get("github.com/mono.svg.gz").await.unwrap().is_none());
        assert!(pack.get("gitlab.com/meta.json").await.unwrap().is_none());

        assert!(Pack::open(&out, "v5").is_err());
        std::fs::write(root.join("junk.pack"), b"not a pack at all").unwrap();
        assert!(Pack::open(&root.join("junk.pack"), "v4").is_err());
        let mut cut = std::fs::read(&out).unwrap();
        cut.truncate(cut.len() - 3);
        std::fs::write(root.join("cut.pack"), cut).unwrap();
        assert!(Pack::open(&root.join("cut.pack"), "v4").is_err());

        std::fs::remove_dir_all(&root).unwrap();
    }
}
