use crate::i18n::t;
use anyhow::{Context, Result, bail};
use gpui::{RenderImage, SurfaceFrame};
use gpui_media::{MediaSource, VideoFrameExtractor, VideoSurface};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

mod source;
pub(crate) use source::Source;

pub(crate) enum MediaContent {
    Image(Arc<RenderImage>),
    Video(Arc<SurfaceFrame>),
}

pub(crate) struct MediaAsset {
    pub path: PathBuf,
    pub source: Arc<Source>,
    pub hash: String,
    pub width: u32,
    pub height: u32,
    pub content: OnceLock<Result<MediaContent, String>>,
    pub video: bool,
}

// Document snapshots share decoded media; playback state is kept separately.
impl PartialEq for MediaAsset {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}
impl std::fmt::Debug for MediaAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaAsset")
            .field("path", &self.path)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}
impl MediaAsset {
    pub async fn load_async(
        path: PathBuf,
        executor: &gpui::BackgroundExecutor,
    ) -> Result<Arc<Self>> {
        #[cfg(not(target_family = "wasm"))]
        {
            executor.spawn(async move { Self::load(path) }).await
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = executor;
            if !is_video(&path) {
                return Self::load(path);
            }
            let (source, hash) = Self::cache_source(&path)?;
            let (width, height, content) = Self::decode_video(&source).await?;
            Ok(Arc::new(Self {
                path,
                source,
                hash,
                width,
                height,
                content: OnceLock::from(Ok(content)),
                video: true,
            }))
        }
    }
    pub async fn ensure_decoded_async(
        self: Arc<Self>,
        executor: &gpui::BackgroundExecutor,
    ) -> Result<()> {
        #[cfg(not(target_family = "wasm"))]
        {
            executor.spawn(async move { self.ensure_decoded() }).await
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = executor;
            if !self.video {
                return self.ensure_decoded();
            }
            if self.content.get().is_none() {
                let result = Self::decode_video(&self.source)
                    .await
                    .and_then(|(w, h, content)| {
                        anyhow::ensure!(
                            (w, h) == (self.width, self.height),
                            "Media dimensions differ from stored metadata"
                        );
                        Ok(content)
                    })
                    .map_err(|e| e.to_string());
                let _ = self.content.set(result);
            }
            self.content
                .get()
                .unwrap()
                .as_ref()
                .map(|_| ())
                .map_err(|e| anyhow::anyhow!(e.clone()))
        }
    }
    #[cfg(target_family = "wasm")]
    async fn decode_video(source: &Source) -> Result<(u32, u32, MediaContent)> {
        gpui_media_backend::SystemBackend::initialize()?;
        let frame = VideoFrameExtractor::new(
            source.media_source()?,
            Arc::new(gpui_media_backend::SystemBackend),
        )?
        .initial_frame()
        .await?;
        let size = frame.display_size();
        Ok((
            size.width.try_into()?,
            size.height.try_into()?,
            MediaContent::Video(VideoSurface::new().set_frame(&frame)?),
        ))
    }
    pub fn kind(&self) -> crate::shape::ShapeKind {
        if self.video {
            crate::shape::ShapeKind::Video
        } else {
            crate::shape::ShapeKind::Image
        }
    }
    pub fn content(&self) -> Option<&MediaContent> {
        self.content.get().and_then(|result| result.as_ref().ok())
    }
    pub fn is_pending(&self) -> bool {
        self.content.get().is_none()
    }
    pub fn ensure_decoded(&self) -> Result<()> {
        let result = self.content.get_or_init(|| {
            Self::decode(&self.path, &self.source)
                .and_then(|(width, height, content)| {
                    anyhow::ensure!(
                        (width, height) == (self.width, self.height),
                        "Media dimensions differ from stored metadata"
                    );
                    Ok(content)
                })
                .map_err(|error| format!("{error:#}"))
        });
        match result {
            Ok(_) => Ok(()),
            Err(error) => anyhow::bail!(error.clone()),
        }
    }
    pub fn deferred(path: PathBuf, source: Arc<Source>, hash: String, size: [u32; 2]) -> Arc<Self> {
        let video = is_video(&path);
        Arc::new(Self {
            path,
            source,
            hash,
            width: size[0],
            height: size[1],
            video,
            content: OnceLock::new(),
        })
    }

    pub fn name(&self) -> String {
        self.path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }
    pub fn load(path: PathBuf) -> Result<Arc<Self>> {
        let (source, hash) = Self::cache_source(&path)?;
        Self::from_cached(path, source, hash)
    }
    fn cache_source(path: &Path) -> Result<(Arc<Source>, String)> {
        use sha2::{Digest, Sha256};
        use std::io::{Read, Write};
        let mut input = rovar_storage::fs::File::open(path)?;
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("bin");
        let mut file = rovar_storage::tempfile::Builder::new()
            .suffix(&format!(".{extension}"))
            .tempfile()?;
        let mut digest = Sha256::new();
        let mut bytes = vec![0; 1024 * 1024];
        loop {
            let count = input.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            file.write_all(&bytes[..count])?;
            digest.update(&bytes[..count]);
        }
        Ok((
            Source::file(file.into_temp_path()),
            hex::encode(digest.finalize()),
        ))
    }
    pub(crate) fn from_cached(
        path: PathBuf,
        source: Arc<Source>,
        hash: String,
    ) -> Result<Arc<Self>> {
        let (width, height, content) = Self::decode(&path, &source)?;
        let video = is_video(&path);
        Ok(Arc::new(Self {
            path,
            source,
            hash,
            width,
            height,
            video,
            content: OnceLock::from(Ok(content)),
        }))
    }
    pub(crate) fn load_image(path: &Path) -> Result<Arc<Self>> {
        let (source, hash) = Self::cache_source(path)?;
        Self::from_cached(path.to_owned(), source, hash)
    }
    fn decode(path: &Path, source: &Source) -> Result<(u32, u32, MediaContent)> {
        if is_video(path) {
            gpui_media_backend::SystemBackend::initialize()?;
            let cache = source.cached_path()?;
            let frame = VideoFrameExtractor::new(
                MediaSource::from_path(&*cache)?,
                Arc::new(gpui_media_backend::SystemBackend),
            )?
            .initial_frame_blocking()?;
            let size = frame.display_size();
            let poster = VideoSurface::new().set_frame(&frame)?;
            return Ok((
                size.width.try_into()?,
                size.height.try_into()?,
                MediaContent::Video(poster),
            ));
        }
        source.verify()?;
        let mut reader = image::ImageReader::new(std::io::BufReader::new(source.open()?))
            .with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16_384);
        limits.max_image_height = Some(16_384);
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        let mut decoder = reader.into_decoder().context(t("media-supported"))?;
        use image::ImageDecoder;
        let orientation = decoder.orientation()?;
        let mut decoded = image::DynamicImage::from_decoder(decoder)?;
        decoded.apply_orientation(orientation);
        let mut pixels = decoded.into_rgba8();
        let (width, height) = pixels.dimensions();
        if width == 0 || height == 0 {
            bail!("{}", t("image-invalid-size"));
        }
        for p in pixels.pixels_mut() {
            p.0.swap(0, 2);
        }
        Ok((
            width,
            height,
            MediaContent::Image(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]))),
        ))
    }
}

fn is_video(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v"
    )
}
