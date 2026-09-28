use crate::scene_render::{TextFragment, render_options};
use crate::{
    artboard::Rect,
    document::{AssetSource, Document},
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, io::Write as _, path::PathBuf, sync::Arc};
#[cfg(test)]
mod tests;

#[derive(Debug)]
struct ExportError(&'static str);
impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::i18n::t(self.0))
    }
}
impl std::error::Error for ExportError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Png,
    Svg,
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Svg => "svg",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Svg => "SVG",
        }
    }
}
pub(crate) struct Job {
    pub original: Option<AssetSource>,
    pub name: String,
    pub json: Arc<Vec<u8>>,
    pub assets: Arc<Vec<AssetSource>>,
    pub order: Vec<usize>,
    pub bounds: Rect,
    pub clips: BTreeMap<usize, Rect>,
    pub text: BTreeMap<usize, Vec<TextFragment>>,
}

impl Job {
    pub fn extension(&self, format: Format) -> Result<String> {
        match &self.original {
            Some(source) => std::path::Path::new(&source.name)
                .extension()
                .and_then(|ext| ext.to_str())
                .filter(|ext| !ext.is_empty())
                .map(|ext| ext.to_ascii_lowercase())
                .context("Missing video format"),
            None => Ok(format.extension().into()),
        }
    }
    pub fn output_name(&self, format: Format) -> Result<String> {
        let extension = self.extension(format)?;
        let name = filename(&self.name);
        let stem = if self.original.is_some()
            && std::path::Path::new(&name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case(&extension))
        {
            std::path::Path::new(&name)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        } else {
            name
        };
        Ok(format!("{}.{}", filename(&stem), extension))
    }

    pub fn svg(&self) -> Result<String> {
        let document = Document::decode(&self.json)?;
        ensure!(
            !document
                .shapes
                .iter()
                .any(|shape| self.order.contains(&shape.id)
                    && shape.kind == crate::shape::ShapeKind::Video),
            ExportError("export-video-unsupported")
        );
        crate::scene_render::Scene {
            document: &document,
            assets: &self.assets,
            order: &self.order,
            bounds: self.bounds,
            clips: &self.clips,
            text: &self.text,
        }
        .svg(crate::scene_render::Output::Export)
    }
    pub fn render(
        &self,
        format: Format,
        scale: u32,
        options: &resvg::usvg::Options,
    ) -> Result<Vec<u8>> {
        let svg = self.svg()?;
        let tree = resvg::usvg::Tree::from_str(&svg, options)?;
        if format == Format::Svg {
            return Ok(tree
                .to_string(&resvg::usvg::WriteOptions::default())
                .into_bytes());
        }
        let (width, height) = (
            (self.bounds.width * scale as f32).ceil(),
            (self.bounds.height * scale as f32).ceil(),
        );
        ensure!(
            scale > 0 && width <= 16384. && height <= 16384. && width * height <= 64_000_000.,
            ExportError("export-size-limit")
        );
        let mut pixels = resvg::tiny_skia::Pixmap::new(width as u32, height as u32)
            .context("Invalid PNG size")?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale as f32, scale as f32),
            &mut pixels.as_mut(),
        );
        Ok(pixels.encode_png()?)
    }
}

pub(crate) fn filename(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c))
        .take(100)
        .collect();
    let clean = clean.trim().trim_matches('.');
    if clean.is_empty() {
        "Export".into()
    } else {
        clean.into()
    }
}

pub(crate) fn write(
    jobs: Vec<Job>,
    destination: PathBuf,
    format: Format,
    scale: u32,
    batch: bool,
) -> Result<Vec<PathBuf>> {
    ensure!(!jobs.is_empty(), "Empty export");
    if !batch {
        let extension = jobs[0].extension(format)?;
        ensure!(
            destination
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case(&extension)),
            ExportError("export-extension")
        );
    }
    let parent = if batch {
        destination.as_path()
    } else {
        destination.parent().context("Missing export directory")?
    };
    let options = render_options(jobs.iter().any(|job| !job.text.is_empty()))?;
    let mut prepared = Vec::new();
    for job in jobs {
        let name = job.output_name(format)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        if let Some(source) = &job.original {
            source.path.verify()?;
            let mut input = source.path.open()?;
            std::io::copy(&mut input, &mut file)?;
        } else {
            file.write_all(&job.render(format, scale, &options)?)?;
        }
        file.as_file().sync_all()?;
        prepared.push((name, file));
    }
    if !batch {
        let (_, file) = prepared.pop().context("Empty export")?;
        file.persist(&destination)?;
        return Ok(vec![destination]);
    }
    let mut written = Vec::new();
    for (name, mut file) in prepared {
        let mut suffix = 0;
        loop {
            let name = if suffix == 0 {
                name.clone()
            } else {
                let path = std::path::Path::new(&name);
                format!(
                    "{} ({suffix}).{}",
                    path.file_stem().unwrap().to_string_lossy(),
                    path.extension().unwrap().to_string_lossy()
                )
            };
            let path = parent.join(name);
            match file.persist_noclobber(&path) {
                Ok(_) => {
                    written.push(path);
                    break;
                }
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    file = error.file;
                    suffix += 1;
                }
                Err(error) => {
                    for path in &written {
                        let _ = std::fs::remove_file(path);
                    }
                    return Err(error.error.into());
                }
            }
        }
    }
    Ok(written)
}
