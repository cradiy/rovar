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
    File(
        Arc<rovar_storage::tempfile::TempPath>,
        #[cfg(target_family = "wasm")] Mutex<Option<BrowserUrl>>,
    ),
    Block {
        block: rovar_format::BlockHandle,
        extension: String,
        cache: Mutex<Option<Arc<rovar_storage::tempfile::TempPath>>>,
        #[cfg(target_family = "wasm")]
        url: Mutex<Option<BrowserUrl>>,
    },
}
impl Source {
    pub fn file(path: rovar_storage::tempfile::TempPath) -> Arc<Self> {
        Arc::new(Self::File(
            Arc::new(path),
            #[cfg(target_family = "wasm")]
            Mutex::new(None),
        ))
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
            #[cfg(target_family = "wasm")]
            url: Mutex::new(None),
        })
    }
    pub fn open(&self) -> Result<Box<dyn ReadSeek>> {
        Ok(match self {
            Self::File(path, ..) => Box::new(rovar_storage::fs::File::open(&**path)?),
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
    pub fn cached_path(&self) -> Result<Arc<rovar_storage::tempfile::TempPath>> {
        match self {
            Self::File(path, ..) => Ok(path.clone()),
            Self::Block {
                block,
                extension,
                cache,
                ..
            } => {
                let mut cache = cache
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Poisoned media cache"))?;
                if let Some(path) = &*cache {
                    return Ok(path.clone());
                }
                let mut file = rovar_storage::tempfile::Builder::new()
                    .suffix(&format!(".{extension}"))
                    .tempfile()?;
                block.copy_verified(&mut file)?;
                let path = Arc::new(file.into_temp_path());
                *cache = Some(path.clone());
                Ok(path)
            }
        }
    }
    pub fn media_source(&self) -> Result<gpui_media::MediaSource> {
        #[cfg(not(target_family = "wasm"))]
        {
            Ok(gpui_media::MediaSource::from_path(&*self.cached_path()?)?)
        }
        #[cfg(target_family = "wasm")]
        {
            let url = match self {
                Self::File(_, url) | Self::Block { url, .. } => url,
            };
            let mut url = url
                .lock()
                .map_err(|_| anyhow::anyhow!("Poisoned media URL"))?;
            if url.is_none() {
                self.verify()?;
                let mut bytes = Vec::new();
                self.open()?.read_to_end(&mut bytes)?;
                let array = js_sys::Array::new();
                array.push(&js_sys::Uint8Array::from(bytes.as_slice()));
                let blob = web_sys::Blob::new_with_u8_array_sequence(&array)
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                *url = Some(BrowserUrl(
                    web_sys::Url::create_object_url_with_blob(&blob)
                        .map_err(|e| anyhow::anyhow!("{e:?}"))?,
                ));
            }
            Ok(gpui_media::MediaSource::from_uri(&url.as_ref().unwrap().0)?)
        }
    }
    #[cfg(test)]
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.open().unwrap().read_to_end(&mut bytes).unwrap();
        bytes
    }
}

#[cfg(target_family = "wasm")]
pub(crate) struct BrowserUrl(String);
#[cfg(target_family = "wasm")]
impl Drop for BrowserUrl {
    fn drop(&mut self) {
        let _ = web_sys::Url::revoke_object_url(&self.0);
    }
}
