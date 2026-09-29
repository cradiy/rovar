use crate::{
    Reader, VERSION,
    layout::{self, Block, Commit, FORMAT_TAG, HEADER, MAGIC, MAX_INDEX, SLOT},
};
use anyhow::{Result, ensure};
use rovar_storage::fs::{File, OpenOptions};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
};

pub struct Writer {
    file: File,
    committed: Reader,
    index: BTreeMap<String, Block>,
}
impl Writer {
    /// Initialize an empty file. Never truncates an existing container.
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.try_lock()?;
        ensure!(file.metadata()?.len() == 0, "Container already exists");
        let mut header = [0; HEADER as usize];
        header[..8].copy_from_slice(MAGIC);
        header[8..12].copy_from_slice(&VERSION.to_le_bytes());
        header[12..20].copy_from_slice(FORMAT_TAG);
        file.write_all(&header)?;
        let committed = Reader {
            file: Arc::new(Mutex::new(file.try_clone()?)),
            commit: Commit {
                end: HEADER,
                ..Default::default()
            },
            index: BTreeMap::new(),
        };
        Ok(Self {
            file,
            committed,
            index: BTreeMap::new(),
        })
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.as_ref())?;
        file.try_lock()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let held = file.metadata()?;
            let live = rovar_storage::fs::metadata(path.as_ref())?;
            ensure!(
                (held.dev(), held.ino()) == (live.dev(), live.ino()),
                "Container was replaced while opening"
            );
        }
        let committed = Reader::from_file(file.try_clone()?)?;
        // Discard only an unpublished tail, never committed blocks.
        file.set_len(committed.commit.end)?;
        let index = committed.index.clone();
        Ok(Self {
            file,
            committed,
            index,
        })
    }
    pub fn snapshot(&self) -> Reader {
        self.committed.clone()
    }
    pub fn contains(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }
    pub fn retain(&mut self, keys: &BTreeSet<String>) {
        self.index.retain(|key, _| keys.contains(key));
    }
    pub fn remove(&mut self, key: &str) {
        self.index.remove(key);
    }
    pub fn put_bytes(&mut self, key: &str, kind: &str, bytes: &[u8]) -> Result<()> {
        let hash: [u8; 32] = Sha256::digest(bytes).into();
        if self.index.get(key).is_some_and(|old| {
            old.hash == hash && old.kind == kind && old.length == bytes.len() as u64
        }) {
            return Ok(());
        }
        self.put(key, kind, bytes, Some(hash))
    }
    /// Streams bytes with bounded memory and verifies their expected digest.
    /// The key is changed only when the entire stream has been written.
    pub fn put(
        &mut self,
        key: &str,
        kind: &str,
        mut input: impl Read,
        expected: Option<[u8; 32]>,
    ) -> Result<()> {
        layout::validate_key(key, kind)?;
        let offset = self.file.metadata()?.len();
        let mut length = 0u64;
        let mut hash = Sha256::new();
        let mut buffer = vec![0; 1024 * 1024];
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            write_at(&self.file, &buffer[..count], offset + length)?;
            length = length
                .checked_add(count as u64)
                .ok_or_else(|| anyhow::anyhow!("Block length overflow"))?;
            hash.update(&buffer[..count]);
        }
        let hash = hash.finalize().into();
        ensure!(
            expected.is_none_or(|expected| expected == hash),
            "Source checksum changed"
        );
        self.index.insert(
            key.into(),
            Block {
                kind: kind.into(),
                offset,
                length,
                hash,
            },
        );
        Ok(())
    }
    pub fn commit(&mut self) -> Result<()> {
        let bytes = serde_json::to_vec(&self.index.iter().collect::<Vec<_>>())?;
        ensure!(
            bytes.len() as u64 <= MAX_INDEX,
            "Container index is too large"
        );
        let index_offset = self.file.metadata()?.len();
        let generation = self
            .committed
            .generation()
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Generation overflow"))?;
        let commit = Commit {
            generation,
            index_offset,
            index_length: bytes.len() as u64,
            end: index_offset
                .checked_add(bytes.len() as u64 + SLOT)
                .ok_or_else(|| anyhow::anyhow!("Container length overflow"))?,
            hash: Sha256::digest(&bytes).into(),
        };
        write_at(&self.file, &bytes, index_offset)?;
        // Blocks and index must be durable before publishing either commit copy.
        self.file.sync_all()?;
        let record = commit.encode();
        write_at(&self.file, &record, index_offset + bytes.len() as u64)?;
        self.file.sync_all()?;
        write_at(&self.file, &record, 32 + ((generation - 1) % 2) * SLOT)?;
        self.file.sync_all()?;
        self.committed.commit = commit;
        self.committed.index = self.index.clone();
        Ok(())
    }
    /// Compact after a successful commit when obsolete bytes exceed both the
    /// threshold and the live payload size. This writer must not be reused.
    pub fn compact_if_needed(mut self, path: &Path, threshold: u64) -> Result<()> {
        let live = self.index.values().map(|b| b.length).sum::<u64>()
            + HEADER
            + self.committed.commit.index_length
            + SLOT;
        if self.committed.commit.end.saturating_sub(live) < threshold.max(live) {
            return Ok(());
        }
        let directory = path.parent().unwrap_or(Path::new("."));
        let temporary = rovar_storage::tempfile::NamedTempFile::new_in(directory)?.into_temp_path();
        let mut output = Self::create(&temporary)?;
        for (key, block) in self.committed.entries() {
            output.put(
                key,
                &block.kind,
                self.committed.block(key)?.reader(),
                Some(block.hash),
            )?;
        }
        output.commit()?;
        drop(output);
        rovar_storage::fs::rename(&temporary, path)?;
        crate::sync_parent(path)?;
        self.finish();
        Ok(())
    }
    pub(crate) fn finish(&mut self) {
        let _ = self.file.unlock();
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.finish();
    }
}

fn write_at(file: &File, bytes: &[u8], offset: u64) -> std::io::Result<()> {
    #[cfg(any(unix, target_family = "wasm"))]
    {
        #[cfg(unix)]
        use std::os::unix::fs::FileExt;
        file.write_all_at(bytes, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut remaining = bytes;
        let mut position = offset;
        while !remaining.is_empty() {
            match file.seek_write(remaining, position) {
                Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
                Ok(count) => {
                    remaining = &remaining[count..];
                    position += count as u64;
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}
