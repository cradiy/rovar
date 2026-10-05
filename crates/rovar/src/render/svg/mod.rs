use crate::scene::text::TextFragment;
use crate::{
    document::{AssetSource, Page},
    scene::artboard::{FillMode, Rect},
    scene::shape::{Shape, ShapeKind},
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, fmt::Write as _};
mod backdrop;
mod effects;
mod paint;
mod path;

#[derive(Clone, Copy)]
pub(crate) enum Output {
    Export,
    Preview,
}

pub(crate) struct Scene<'a> {
    pub document: &'a Page,
    pub assets: &'a [AssetSource],
    pub order: &'a [usize],
    pub bounds: Rect,
    pub clips: &'a BTreeMap<usize, Rect>,
    pub text: &'a BTreeMap<usize, Vec<TextFragment>>,
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
        "<clipPath id=\"{id}\"><path d=\"{path}\" clip-rule=\"evenodd\"/></clipPath>"
    )
    .unwrap();
}
fn world(doc: &Page, parent: Option<usize>, mut rect: Rect) -> Rect {
    if let Some(board) = parent.and_then(|id| doc.boards.iter().find(|board| board.id == id)) {
        rect.x += board.rect.x;
        rect.y += board.rect.y;
    }
    rect
}

impl Scene<'_> {
    pub fn svg(&self, output: Output, raster_scale: f32) -> Result<String> {
        let doc = self.document;
        let mut defs = String::new();
        let mut body = String::new();
        let mut images = BTreeMap::new();
        let mut backdrops = backdrop::Renderer::new(self, raster_scale);
        let included = self.order.iter().copied().collect();
        let mut booleans = crate::scene::boolean::Cache::default();
        for id in self.order {
            if crate::scene::boolean::consumed(&doc.hierarchy, *id, &included) {
                continue;
            }
            let composition = crate::scene::boolean::is_boolean(&doc.hierarchy, *id)
                .then(|| booleans.get(&doc.hierarchy, &doc.shapes, *id))
                .flatten();
            backdrops.paint(self, *id, &mut defs, &mut body)?;
            let board = doc.boards.iter().find(|item| item.id == *id);
            let shape = doc
                .shapes
                .iter()
                .find(|item| item.id == *id)
                .or_else(|| composition.as_ref().map(|g| &g.shape));
            let text = doc.texts.iter().find(|item| item.id == *id);
            let (rect, rotation) = if let Some(b) = board {
                (b.rect, b.layer.rotation)
            } else if let Some(s) = shape {
                (world(doc, s.board, s.rect), s.layer.rotation)
            } else if let Some(t) = text {
                (world(doc, t.board, t.rect), t.layer.rotation)
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
            let mut effect_rect = rect;
            if let Some(shape) = shape.filter(|s| s.stroke.enabled) {
                let pad = if shape.kind == ShapeKind::Arrow {
                    (shape.stroke.width * 4.).max(12.)
                } else if shape.kind.is_path() || shape.kind.is_polygon() {
                    shape.stroke.width / 2.
                } else {
                    shape.stroke.outset()
                };
                effect_rect.x -= pad;
                effect_rect.y -= pad;
                effect_rect.width += 2. * pad;
                effect_rect.height += 2. * pad;
            }
            let has_shadow = doc
                .hierarchy
                .effects
                .get(id)
                .is_some_and(|shadows| effects::filter(&mut defs, *id, effect_rect, shadows));
            if has_shadow {
                write!(body, "<g filter=\"url(#shadow-{id})\">")?;
            }
            if shape.is_some_and(|s| s.kind == ShapeKind::Video) {
                ensure!(
                    matches!(output, Output::Preview),
                    "Video requires original-file export"
                );
                let cx = rect.x + rect.width / 2.;
                let cy = rect.y + rect.height / 2.;
                write!(
                    body,
                    "<path d=\"{}\" fill=\"#282c36\"/><path d=\"M {} {} l {} {} l {} {} Z\" fill=\"#b4a2ee\"/>",
                    rect_path(rect),
                    cx - rect.width * 0.1,
                    cy - rect.height * 0.16,
                    rect.width * 0.24,
                    rect.height * 0.16,
                    -rect.width * 0.24,
                    rect.height * 0.16
                )?;
            } else if text.is_some() {
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
                        escape(&crate::platform::render_font_family(&style.family)),
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
                let contour = if let Some(g) = &composition {
                    let mut path = String::new();
                    for contour in g.contours.iter() {
                        for (i, p) in contour.iter().enumerate() {
                            write!(
                                path,
                                "{} {} {} ",
                                if i == 0 { "M" } else { "L" },
                                rect.x + p.x * rect.width,
                                rect.y + p.y * rect.height
                            )?;
                        }
                        path.push_str("Z ");
                    }
                    path
                } else {
                    path::contour(&local, 0.)
                };
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
                    && let Some(asset) = doc.assets.iter().find(|a| {
                        a.object == composition.as_ref().map_or(*id, |g| g.source)
                            && a.fill != media
                    })
                {
                    if !images.contains_key(&asset.hash) {
                        let source = self
                            .assets
                            .iter()
                            .find(|s| s.hash == asset.hash)
                            .context("Missing image source")?;
                        images.insert(asset.hash.clone(), paint::image_uri(source, output)?);
                    }
                    let key = format!("image-clip-{id}");
                    clip(&mut defs, &key, &contour);
                    let source = self
                        .assets
                        .iter()
                        .find(|s| s.hash == asset.hash)
                        .context("Missing image source")?;
                    let (placement, fit) = if media {
                        (
                            shape.unwrap().media_placement,
                            crate::scene::image_fill::ImageFit::Contain,
                        )
                    } else {
                        (image_fill.placement, image_fill.fit)
                    };
                    let image_rect = placement.rect(rect, source.size, fit);
                    let opacity = if media { 1. } else { image_fill.opacity };
                    write!(
                        body,
                        "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" opacity=\"{opacity}\" clip-path=\"url(#{key})\" href=\"{}\"/>",
                        image_rect.x,
                        image_rect.y,
                        image_rect.width,
                        image_rect.height,
                        images[&asset.hash]
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
            if has_shadow {
                body.push_str("</g>");
            }
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
        let (width, height) = match output {
            Output::Export => (r.width, r.height),
            Output::Preview => (560., 336.),
        };
        Ok(format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"{} {} {} {}\"><defs>{defs}</defs>{body}</svg>",
            r.x, r.y, r.width, r.height
        ))
    }
}

#[cfg(target_family = "wasm")]
pub(crate) fn render_options(_: bool) -> Result<crate::render::raster::Options> {
    Ok(crate::render::raster::Options)
}

#[cfg(not(target_family = "wasm"))]
pub(crate) fn render_options(with_text: bool) -> Result<crate::render::raster::Options> {
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
