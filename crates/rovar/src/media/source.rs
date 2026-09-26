use anyhow::Result;
use std::{
    io::{Read, Seek},
    path::Path,
    sync::{Arc, Mutex},
};

pub(crate) trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// Keeps either imported bytes or an immutable container snapshot alive.
pub(crate) enum Source {
    File(Arc<tempfile::TempPath>),
    Block {
        block: rovar_format::BlockHandle,
        extension: String,
        cache: Mutex<Option<Arc<tempfile::TempPath>>>,
    },
}
impl Source {
    pub fn file(path: tempfile::TempPath) -> Arc<Self> {
        Arc::new(Self::File(Arc::new(path)))
    }
    pub fn block(block: rovar_format::BlockHandle, name: &str) -> Arc<Self> {
        let extension = Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin")
            .to_owned();
        Arc::new(Self::Block {
            block,
            extension,
            cache: Mutex::new(None),
        })
    }
    pub fn open(&self) -> Result<Box<dyn ReadSeek>> {
        Ok(match self {
            Self::File(path) => Box::new(std::fs::File::open(&**path)?),
            Self::Block { block, .. } => Box::new(block.reader()),
        })
    }
    pub fn verify(&self) -> Result<()> {
        if let Self::Block { block, .. } = self {
            block.copy_verified(&mut std::io::sink())?;
        }
        Ok(())
    }
    /// URI-only playback backends need a local file. Materialize just this
    /// requested video, once; images read directly from their container block.
    pub fn cached_path(&self) -> Result<Arc<tempfile::TempPath>> {
        match self {
            Self::File(path) => Ok(path.clone()),
            Self::Block {
                block,
                extension,
                cache,
            } => {
                let mut cache = cache
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Poisoned media cache"))?;
                if let Some(path) = &*cache {
                    return Ok(path.clone());
                }
                let mut file = tempfile::Builder::new()
                    .suffix(&format!(".{extension}"))
                    .tempfile()?;
                block.copy_verified(&mut file)?;
                let path = Arc::new(file.into_temp_path());
                *cache = Some(path.clone());
                Ok(path)
            }
        }
    }
    #[cfg(test)]
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.open().unwrap().read_to_end(&mut bytes).unwrap();
        bytes
    }
}
