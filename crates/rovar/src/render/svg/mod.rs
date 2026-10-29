use crate::scene::text::TextFragment;
use crate::{
    document::{AssetSource, Page},
    scene::artboard::{FillMode, Rect},
    scene::shape::{Shape, ShapeKind},
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, fmt::Write as _};
mod backdrop;
mod blend;
mod effects;
pub(crate) mod glow;
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
        let mut blending = blend::Stack::new(doc);
        let mut images = BTreeMap::new();
        let mut backdrops = backdrop::Renderer::new(self, raster_scale);
        let included = self.order.iter().copied().collect();
        let mut booleans = crate::scene::boolean::Cache::default();
        let masks =
            crate::scene::mask::outlines(&doc.hierarchy, &doc.shapes, &doc.boards, &mut booleans);
        for (id, outline) in &masks {
            let mut path = String::new();
            for contour in &outline.0 {
                for (i, p) in contour.iter().enumerate() {
                    write!(path, "{} {} {} ", if i == 0 { "M" } else { "L" }, p.x, p.y)?;
                }
                path.push_str("Z ");
            }
            clip(&mut defs, &format!("vector-mask-{id}"), &path);
        }
        for id in self.order {
            if doc
                .shapes
                .iter()
                .any(|s| s.id == *id && s.kind == ShapeKind::Audio)
            {
                continue;
            }
            if crate::scene::boolean::consumed(&doc.hierarchy, *id, &included)
                || crate::scene::mask::is_source(&doc.hierarchy, *id, &included)
            {
                continue;
            }
            blending.advance(*id, &mut body);
            // Backdrop filtering changes preceding artwork in this scope; it
            // must run before isolating the layer's foreground for blending.
            backdrops.paint(self, *id, &mut defs, &mut body)?;
            blending.enter(*id, &mut body);
            let composition = crate::scene::boolean::is_boolean(&doc.hierarchy, *id)
                .then(|| booleans.get(&doc.hierarchy, &doc.shapes, *id))
                .flatten();
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
            let mask_groups = crate::scene::mask::ancestors(&doc.hierarchy, *id, &included);
            for group in &mask_groups {
                write!(body, "<g clip-path=\"url(#vector-mask-{group})\">")?;
            }
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
            if let Some(shape) = shape {
                let pad = std::iter::once(&shape.stroke)
                    .chain(&shape.stroke_layers)
                    .filter(|s| s.enabled)
                    .map(|stroke| {
                        if shape.kind == ShapeKind::Arrow {
                            (stroke.width * 4.).max(12.)
                        } else if shape.kind.is_path() || shape.kind.is_polygon() {
                            stroke.width / 2.
                        } else {
                            stroke.outset()
                        }
                    })
                    .fold(0., f32::max);
                effect_rect.x -= pad;
                effect_rect.y -= pad;
                effect_rect.width += 2. * pad;
                effect_rect.height += 2. * pad;
            }
            let source_start = body.len();
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
                for fragment in self.text.get(id).context("Missing text layout")? {
                    if let Some(color) = fragment.style.background {
                        let mut background = fragment.bounds;
                        background.x += rect.x;
                        background.y += rect.y;
                        let fill = paint::paint(
                            FillMode::Solid,
                            color,
                            &fragment.style.gradient,
                            background,
                            &mut defs,
                            &format!("text-background-{id}"),
                        )?;
                        write!(body, "<path d=\"{}\" {fill}/>", rect_path(background))?;
                    }
                }
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
                        "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" font-style=\"{}\" text-decoration=\"{}\" letter-spacing=\"{}\" {fill}>{}</text>",
                        rect.x + fragment.x,
                        rect.y + fragment.baseline,
                        escape(&crate::platform::render_font_family(&style.family)),
                        style.size,
                        style.weight,
                        if style.italic { "italic" } else { "normal" },
                        match (style.underline, style.strikethrough) {
                            (true, true) => "underline line-through",
                            (true, false) => "underline",
                            (false, true) => "line-through",
                            (false, false) => "none",
                        },
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
                if matches!(local.kind, ShapeKind::Image | ShapeKind::Svg) {
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
                    (
                        b.fill_mode,
                        b.color,
                        &b.gradient,
                        b.fill_enabled && !b.fill_hidden,
                        &b.image_fill,
                    )
                } else {
                    let s = shape.unwrap();
                    (
                        s.fill_mode,
                        s.color,
                        &s.gradient,
                        s.fill_enabled && !s.fill_hidden && s.can_fill(),
                        &s.image_fill,
                    )
                };
                let media =
                    shape.is_some_and(|s| matches!(s.kind, ShapeKind::Image | ShapeKind::Svg));
                let extra = board
                    .map(|b| b.fill_layers.as_slice())
                    .unwrap_or_else(|| &shape.unwrap().fill_layers);
                for (layer_index, (mode, color, gradient, enabled, image_fill, image_hash)) in
                    std::iter::once((mode, color, gradient, enabled, image_fill, None))
                        .chain(extra.iter().map(|f| {
                            (
                                f.fill_mode,
                                f.color,
                                &f.gradient,
                                f.enabled && !f.hidden,
                                &f.image_fill,
                                f.image_hash.as_deref(),
                            )
                        }))
                        .enumerate()
                {
                    let media = media && layer_index == 0;
                    let opacity = if layer_index > 0 {
                        extra[layer_index - 1].opacity
                    } else if media {
                        1.
                    } else {
                        board.map_or_else(|| shape.unwrap().fill_opacity, |b| b.fill_opacity)
                    };
                    write!(body, "<g opacity=\"{opacity}\">")?;
                    if enabled && mode != FillMode::Image && !media {
                        let fill = paint::paint(
                            mode,
                            color,
                            gradient,
                            rect,
                            &mut defs,
                            &format!("fill-{id}-{layer_index}"),
                        )?;
                        write!(body, "<path d=\"{contour}\" fill-rule=\"evenodd\" {fill}/>")?;
                    }
                    if (media || (enabled && mode == FillMode::Image))
                        && let Some(hash) = image_hash.or_else(|| {
                            (layer_index == 0)
                                .then(|| {
                                    doc.assets
                                        .iter()
                                        .find(|a| {
                                            a.object
                                                == composition.as_ref().map_or(*id, |g| g.source)
                                                && a.fill != media
                                        })
                                        .map(|a| a.hash.as_str())
                                })
                                .flatten()
                        })
                    {
                        if !images.contains_key(hash) {
                            let source = self
                                .assets
                                .iter()
                                .find(|s| s.hash == hash)
                                .context("Missing image source")?;
                            images.insert(hash.to_owned(), paint::image_uri(source, output)?);
                        }
                        let key = format!("image-clip-{id}-{layer_index}");
                        clip(&mut defs, &key, &contour);
                        let source = self
                            .assets
                            .iter()
                            .find(|s| s.hash == hash)
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
                            images[hash]
                        )?;
                    }
                    body.push_str("</g>");
                }
                if let Some(s) = shape {
                    for (stroke_index, stroke) in std::iter::once(&s.stroke)
                        .chain(&s.stroke_layers)
                        .enumerate()
                        .filter(|(_, s)| s.enabled && !s.hidden && s.width > 0.)
                    {
                        write!(body, "<g opacity=\"{}\">", stroke.opacity)?;
                        let vector = s.kind.is_path() || s.kind.is_polygon();
                        let outset = if vector {
                            stroke.width / 2.
                        } else {
                            stroke.outset()
                        };
                        let paint_bounds = path::expand(rect, outset);
                        let fill = paint::paint(
                            stroke.fill_mode,
                            stroke.color,
                            &stroke.gradient,
                            paint_bounds,
                            &mut defs,
                            &format!("stroke-{id}-{stroke_index}"),
                        )?;
                        if vector {
                            let stroke_paint = fill
                                .replace("fill=", "stroke=")
                                .replace("fill-opacity=", "stroke-opacity=");
                            write!(
                                body,
                                "<path d=\"{contour}\" fill=\"none\" {stroke_paint} stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                                stroke.width
                            )?;
                        } else {
                            let outer = path::contour(&local, outset);
                            let inner = path::contour(&local, outset - stroke.width);
                            write!(
                                body,
                                "<path d=\"{outer} {inner}\" fill-rule=\"evenodd\" {fill}/>"
                            )?;
                        }
                        body.push_str("</g>");
                    }
                }
            }
            if let Some(effects) = doc.hierarchy.effects.get(id) {
                let source = body.split_off(source_start);
                let lights = glow::images(
                    &defs,
                    &source,
                    effect_rect,
                    effects,
                    raster_scale,
                    text.is_some(),
                )?;
                if effects::filter(&mut defs, *id, effect_rect, effects, &lights) {
                    write!(body, "<g filter=\"url(#shadow-{id})\">{source}</g>")?;
                } else {
                    body.push_str(&source);
                }
            }
            body.push_str("</g>");
            if has_clip {
                body.push_str("</g>");
            }
            for _ in &mask_groups {
                body.push_str("</g>");
            }
        }
        blending.finish(&mut body);
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
