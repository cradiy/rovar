pub use crate::layout::Block;
use crate::layout::{self, Commit};
use anyhow::{Context, Result, ensure};
use rovar_storage::fs::File;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct Reader {
    pub(crate) file: Arc<Mutex<File>>,
    pub(crate) commit: Commit,
    pub(crate) index: BTreeMap<String, Block>,
}
impl Reader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_file(File::open(path)?)
    }
    pub(crate) fn from_file(mut file: File) -> Result<Self> {
        let (commit, index) = layout::read_commit(&mut file)?;
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            commit,
            index,
        })
    }
    pub fn version(&self) -> crate::Version {
        crate::VERSION
    }
    pub fn generation(&self) -> u64 {
        self.commit.generation
    }
    pub fn committed_length(&self) -> u64 {
        self.commit.end
    }
    pub fn entries(&self) -> impl Iterator<Item = (&str, &Block)> {
        self.index.iter().map(|(key, value)| (key.as_str(), value))
    }
    pub fn entry(&self, key: &str) -> Option<&Block> {
        self.index.get(key)
    }
    pub fn block(&self, key: &str) -> Result<BlockHandle> {
        let info = self
            .index
            .get(key)
            .context("Missing container block")?
            .clone();
        Ok(BlockHandle {
            file: self.file.clone(),
            info,
        })
    }
    /// Read and validate a bounded metadata block. Media should use `block`.
    pub fn read(&self, key: &str, maximum: u64) -> Result<Vec<u8>> {
        let block = self.block(key)?;
        ensure!(block.info.length <= maximum, "Block exceeds read limit");
        let mut bytes = Vec::new();
        block.copy_verified(&mut bytes)?;
        Ok(bytes)
    }
    pub fn verify(&self) -> Result<()> {
        for key in self.index.keys() {
            self.block(key)?
                .copy_verified(&mut io::sink())
                .with_context(|| format!("Damaged block: {key}"))?;
        }
        Ok(())
    }
}

/// Immutable reference to a block in an open file snapshot. Keeps the file alive
/// across rename/unlink; no path reopen or shared seek cursor escapes this type.
#[derive(Clone)]
pub struct BlockHandle {
    file: Arc<Mutex<File>>,
    pub info: Block,
}
impl BlockHandle {
    pub fn reader(&self) -> BlockReader {
        BlockReader {
            block: self.clone(),
            position: 0,
        }
    }
    pub fn copy_verified(&self, output: &mut impl io::Write) -> Result<u64> {
        let mut reader = self.reader();
        let mut buffer = vec![0; 1024 * 1024];
        let mut hash = Sha256::new();
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
            output.write_all(&buffer[..count])?;
        }
        ensure!(
            hash.finalize()[..] == self.info.hash,
            "Damaged block checksum"
        );
        Ok(self.info.length)
    }
}

pub struct BlockReader {
    block: BlockHandle,
    position: u64,
}
impl Read for BlockReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self.block.info.length.saturating_sub(self.position);
        let length = remaining.min(buffer.len() as u64) as usize;
        if length == 0 {
            return Ok(0);
        }
        let file = self
            .block
            .file
            .lock()
            .map_err(|_| io::Error::other("Poisoned file lock"))?;
        #[cfg(any(unix, target_family = "wasm"))]
        let count = {
            #[cfg(unix)]
            use std::os::unix::fs::FileExt;
            file.read_at(
                &mut buffer[..length],
                self.block.info.offset + self.position,
            )?
        };
        #[cfg(windows)]
        let count = {
            use std::os::windows::fs::FileExt;
            file.seek_read(
                &mut buffer[..length],
                self.block.info.offset + self.position,
            )?
        };
        if count == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.position += count as u64;
        Ok(count)
    }
}
impl Seek for BlockReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
            SeekFrom::End(value) => i128::from(self.block.info.length) + i128::from(value),
        };
        if !(0..=u64::MAX as i128).contains(&position) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.position = position as u64;
        Ok(self.position)
    }
}
