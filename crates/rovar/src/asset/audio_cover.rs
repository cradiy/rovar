use super::{Source, source::ReadSeek};
use anyhow::Result;
use gpui::RenderImage;
use std::{
    io::{Read, Seek, SeekFrom},
    sync::{Arc, Mutex},
};
use symphonia::core::{
    common::Limit,
    formats::{FormatOptions, probe::Hint},
    io::{MediaSource, MediaSourceStream},
    meta::{MetadataOptions, StandardVisualKey},
};

struct Reader {
    inner: Mutex<Box<dyn ReadSeek>>,
    len: u64,
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner
            .get_mut()
            .map_err(|_| std::io::Error::other("Audio reader poisoned"))?
            .read(buf)
    }
}

impl Seek for Reader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner
            .get_mut()
            .map_err(|_| std::io::Error::other("Audio reader poisoned"))?
            .seek(pos)
    }
}

impl MediaSource for Reader {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

pub(super) fn load(source: &Source) -> Result<Option<Arc<super::ManagedImage>>> {
    let mut inner = source.open()?;
    let len = inner.seek(SeekFrom::End(0))?;
    inner.rewind()?;
    let stream = MediaSourceStream::new(
        Box::new(Reader {
            inner: Mutex::new(inner),
            len,
        }),
        Default::default(),
    );
    let mut format = symphonia::default::get_probe().probe(
        &Hint::new(),
        stream,
        FormatOptions::default(),
        MetadataOptions::default().limit_visual_bytes(Limit::Maximum(16 * 1024 * 1024)),
    )?;
    let mut metadata = format.metadata();
    let mut fallback = None;
    while let Some(revision) = metadata.current() {
        let visuals = revision.media.visuals.iter().chain(
            revision
                .per_track
                .iter()
                .flat_map(|track| &track.metadata.visuals),
        );
        for visual in visuals {
            if (visual.usage == Some(StandardVisualKey::FrontCover) || fallback.is_none())
                && let Ok(image) = decode(&visual.data)
            {
                if visual.usage == Some(StandardVisualKey::FrontCover) {
                    return Ok(Some(image));
                }
                fallback = Some(image);
            }
        }
        if metadata.pop().is_none() {
            break;
        }
    }
    Ok(fallback)
}

fn decode(bytes: &[u8]) -> Result<Arc<super::ManagedImage>> {
    use image::ImageDecoder;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut decoded = image::DynamicImage::from_decoder(decoder)?;
    decoded.apply_orientation(orientation);
    let mut pixels = decoded.thumbnail(512, 512).into_rgba8();
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Ok(super::ManagedImage::new(Arc::new(RenderImage::new(vec![
        image::Frame::new(pixels),
    ]))))
}
