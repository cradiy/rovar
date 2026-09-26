use crate::{
    artboard::{FillMode, Rect},
    document::{AssetSource, Document},
    shape::{Shape, ShapeKind},
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, fmt::Write as _, io::Write as _, path::PathBuf, sync::Arc};
mod paint;
mod path;
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
#[derive(Clone)]
pub(crate) struct TextFragment {
    pub text: String,
    pub x: f32,
    pub baseline: f32,
    pub style: crate::text::TextStyle,
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

pub(super) fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn rect_path(r: Rect) -> String {
    format!(
        "M {} {} h {} v {} h {} Z",
        r.x, r.y, r.width, r.height, -r.width
    )
}
fn clip(defs: &mut String, id: &str, path: &str) {
    write!(
        defs,
        "<clipPath id=\"{id}\"><path d=\"{path}\"/></clipPath>"
    )
    .unwrap();
}
fn world(doc: &Document, parent: Option<usize>, mut rect: Rect) -> Rect {
    if let Some(board) = parent.and_then(|id| doc.boards.iter().find(|board| board.id == id)) {
        rect.x += board.rect.x;
        rect.y += board.rect.y;
    }
    rect
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
        let doc = Document::decode(&self.json)?;
        let mut defs = String::new();
        let mut body = String::new();
        let mut images = BTreeMap::new();
        for id in &self.order {
            let board = doc.boards.iter().find(|item| item.id == *id);
            let shape = doc.shapes.iter().find(|item| item.id == *id);
            let text = doc.texts.iter().find(|item| item.id == *id);
            let (rect, rotation) = if let Some(b) = board {
                (b.rect, b.layer.rotation)
            } else if let Some(s) = shape {
                ensure!(
                    s.kind != ShapeKind::Video,
                    ExportError("export-video-unsupported")
                );
                (world(&doc, s.board, s.rect), s.layer.rotation)
            } else if let Some(t) = text {
                (world(&doc, t.board, t.rect), t.layer.rotation)
            } else {
                continue;
            };
            let has_clip = if let Some(bounds) = self.clips.get(id) {
                let key = format!("board-clip-{id}");
                clip(&mut defs, &key, &rect_path(*bounds));
                write!(body, "<g clip-path=\"url(#{key})\">")?;
                true
            } else {
                false
            };
            write!(
                body,
                "<g transform=\"rotate({rotation} {} {})\">",
                rect.x + rect.width / 2.,
                rect.y + rect.height / 2.
            )?;
            if text.is_some() {
                let key = format!("text-clip-{id}");
                clip(&mut defs, &key, &rect_path(rect));
                write!(body, "<g clip-path=\"url(#{key})\" xml:space=\"preserve\">")?;
                for (index, fragment) in self
                    .text
                    .get(id)
                    .context("Missing text layout")?
                    .iter()
                    .enumerate()
                {
                    let style = &fragment.style;
                    let fill = paint::paint(
                        style.fill_mode,
                        style.color,
                        &style.gradient,
                        rect,
                        &mut defs,
                        &format!("text-{id}-{index}"),
                    )?;
                    write!(
                        body,
                        "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" letter-spacing=\"{}\" {fill}>{}</text>",
                        rect.x + fragment.x,
                        rect.y + fragment.baseline,
                        escape(&style.family),
                        style.size,
                        style.weight,
                        style.spacing,
                        escape(&fragment.text)
                    )?;
                }
                body.push_str("</g>");
            } else {
                let mut local = shape
                    .cloned()
                    .unwrap_or_else(|| Shape::new(*id, None, ShapeKind::Rectangle, rect));
                local.rect = rect;
                if local.kind == ShapeKind::Image {
                    local.kind = ShapeKind::Rectangle;
                }
                let contour = path::contour(&local, 0.);
                let (mode, color, gradient, enabled, image_fill) = if let Some(b) = board {
                    (b.fill_mode, b.color, &b.gradient, true, &b.image_fill)
                } else {
                    let s = shape.unwrap();
                    (
                        s.fill_mode,
                        s.color,
                        &s.gradient,
                        s.fill_enabled && s.can_fill(),
                        &s.image_fill,
                    )
                };
                let media = shape.is_some_and(|s| s.kind == ShapeKind::Image);
                if enabled && mode != FillMode::Image && !media {
                    let fill = paint::paint(
                        mode,
                        color,
                        gradient,
                        rect,
                        &mut defs,
                        &format!("fill-{id}"),
                    )?;
                    write!(body, "<path d=\"{contour}\" fill-rule=\"evenodd\" {fill}/>")?;
                }
                if (media || (enabled && mode == FillMode::Image))
                    && let Some(asset) = doc
                        .assets
                        .iter()
                        .find(|a| a.object == *id && a.fill != media)
                {
                    if !images.contains_key(&asset.hash) {
                        let source = self
                            .assets
                            .iter()
                            .find(|s| s.hash == asset.hash)
                            .context("Missing image source")?;
                        images.insert(asset.hash.clone(), paint::image_uri(source)?);
                    }
                    let key = format!("image-clip-{id}");
                    clip(&mut defs, &key, &contour);
                    let fit = if media || image_fill.fit == crate::image_fill::ImageFit::Contain {
                        "meet"
                    } else {
                        "slice"
                    };
                    let opacity = if media { 1. } else { image_fill.opacity };
                    write!(
                        body,
                        "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"xMidYMid {fit}\" opacity=\"{opacity}\" clip-path=\"url(#{key})\" href=\"{}\"/>",
                        rect.x, rect.y, rect.width, rect.height, images[&asset.hash]
                    )?;
                }
                if let Some(s) = shape.filter(|s| s.stroke.enabled && s.stroke.width > 0.) {
                    let vector = s.kind.is_path() || s.kind.is_polygon();
                    let outset = if vector {
                        s.stroke.width / 2.
                    } else {
                        s.stroke.outset()
                    };
                    let paint_bounds = path::expand(rect, outset);
                    let fill = paint::paint(
                        s.stroke.fill_mode,
                        s.stroke.color,
                        &s.stroke.gradient,
                        paint_bounds,
                        &mut defs,
                        &format!("stroke-{id}"),
                    )?;
                    if vector {
                        let stroke = fill
                            .replace("fill=", "stroke=")
                            .replace("fill-opacity=", "stroke-opacity=");
                        write!(
                            body,
                            "<path d=\"{contour}\" fill=\"none\" {stroke} stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                            s.stroke.width
                        )?;
                    } else {
                        let outer = path::contour(&local, outset);
                        let inner = path::contour(&local, outset - s.stroke.width);
                        write!(
                            body,
                            "<path d=\"{outer} {inner}\" fill-rule=\"evenodd\" {fill}/>"
                        )?;
                    }
                }
            }
            body.push_str("</g>");
            if has_clip {
                body.push_str("</g>");
            }
        }
        let r = self.bounds;
        ensure!(
            [r.x, r.y, r.width, r.height].iter().all(|n| n.is_finite())
                && r.width > 0.
                && r.height > 0.,
            "Invalid export bounds"
        );
        Ok(format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\"><defs>{defs}</defs>{body}</svg>",
            r.width, r.height, r.x, r.y, r.width, r.height
        ))
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

pub(crate) fn render_options(with_text: bool) -> Result<resvg::usvg::Options<'static>> {
    let mut options = resvg::usvg::Options::default();
    if with_text {
        let db = options.fontdb_mut();
        db.load_system_fonts();
        // Resolve the platform's fallback instead of usvg's hard-coded Times New Roman.
        let fallback = font_kit::source::SystemSource::new()
            .select_best_match(
                &[font_kit::family_name::FamilyName::SansSerif],
                &font_kit::properties::Properties::new(),
            )?
            .load()?
            .family_name();
        db.set_serif_family(fallback.clone());
        db.set_sans_serif_family(fallback);
        ensure!(
            db.query(&resvg::usvg::fontdb::Query {
                families: &[resvg::usvg::fontdb::Family::SansSerif],
                ..Default::default()
            })
            .is_some(),
            "No export font available"
        );
    }
    Ok(options)
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
