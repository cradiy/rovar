use super::{Scene, clip, path, rect_path, world};
use crate::{
    render::raster,
    scene::{
        artboard::Rect,
        effects::backdrop,
        shape::{Shape, ShapeKind},
    },
};
use anyhow::{Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::fmt::Write;

pub(super) struct Renderer {
    scope: Rect,
    scale: f32,
    options: Option<raster::Options>,
}

fn expand(r: Rect, padding: f32) -> Rect {
    Rect {
        x: r.x - padding,
        y: r.y - padding,
        width: r.width + padding * 2.,
        height: r.height + padding * 2.,
    }
}

fn region(scene: &Scene<'_>, id: usize) -> Option<(backdrop::Region, String)> {
    let doc = scene.document;
    if let Some(b) = doc.boards.iter().find(|b| b.id == id) {
        return Some((
            backdrop::Region {
                rect: b.rect,
                rotation: b.layer.rotation,
                corners: [0.; 4],
                ellipse: false,
            },
            rect_path(b.rect),
        ));
    }
    let s = doc
        .shapes
        .iter()
        .find(|s| s.id == id && backdrop::supports_shape(s))?;
    let mut local: Shape = s.clone();
    local.rect = world(doc, s.board, s.rect);
    if local.kind == ShapeKind::Image {
        local.kind = ShapeKind::Rectangle;
    }
    Some((
        backdrop::Region {
            rect: local.rect,
            rotation: s.layer.rotation,
            corners: s.displayed_radii(),
            ellipse: s.kind == ShapeKind::Ellipse,
        },
        path::contour(&local, 0.),
    ))
}

impl Renderer {
    pub fn new(scene: &Scene<'_>, scale: f32) -> Self {
        let mut scope = scene.bounds;
        for id in scene.order {
            let radius = scene
                .document
                .hierarchy
                .effects
                .get(id)
                .map_or(0., |e| backdrop::radius(e));
            if radius > 0.
                && let Some((region, _)) = region(scene, *id)
            {
                let r = expand(region.bounds(), radius * 1.5 + 2.);
                let x = scope.x.min(r.x);
                let y = scope.y.min(r.y);
                scope = Rect {
                    x,
                    y,
                    width: (scope.x + scope.width).max(r.x + r.width) - x,
                    height: (scope.y + scope.height).max(r.y + r.height) - y,
                };
            }
        }
        Self {
            scope,
            scale,
            options: None,
        }
    }

    pub fn paint(
        &mut self,
        scene: &Scene<'_>,
        id: usize,
        defs: &mut String,
        body: &mut String,
    ) -> Result<()> {
        let radius = scene
            .document
            .hierarchy
            .effects
            .get(&id)
            .map_or(0., |e| backdrop::radius(e));
        if radius <= 0. || body.is_empty() {
            return Ok(());
        }
        let Some((region, contour)) = region(scene, id) else {
            return Ok(());
        };
        let mut r = expand(region.bounds(), radius * 1.5 + 2.);
        let width = (r.width * self.scale).ceil();
        let height = (r.height * self.scale).ceil();
        ensure!(
            self.scale.is_finite()
                && self.scale > 0.
                && width > 0.
                && height > 0.
                && width <= 16384.
                && height <= 16384.
                && width * height <= 64_000_000.,
            "Background blur capture exceeds the export size limit"
        );
        r.width = width / self.scale;
        r.height = height / self.scale;
        // Each snapshot contains the preceding document artwork only. Earlier
        // backdrops are already images, avoiding recursive SVG use expansion.
        let capture = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\"><defs>{defs}<filter id=\"background-sample-{id}\" filterUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" color-interpolation-filters=\"sRGB\"><feGaussianBlur stdDeviation=\"{}\"/></filter></defs><g filter=\"url(#background-sample-{id})\">{body}</g></svg>",
            r.width,
            r.height,
            r.x,
            r.y,
            r.width,
            r.height,
            r.x,
            r.y,
            r.width,
            r.height,
            radius * 0.5,
        );
        if self.options.is_none() {
            self.options = Some(super::render_options(!scene.text.is_empty())?);
        }
        let pixels = raster::render(
            &capture,
            [width as u32, height as u32],
            self.scale,
            false,
            false,
            self.options.as_ref().unwrap(),
        )?;
        let center = crate::scene::rotation::center(region.rect);
        let opacity = scene
            .document
            .boards
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.layer.opacity)
            .or_else(|| {
                scene
                    .document
                    .shapes
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.layer.opacity)
            })
            .unwrap_or(1.);
        let transform = format!("rotate({} {} {})", region.rotation, center.x, center.y);
        let mut coverage = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\">",
            r.width, r.height, r.x, r.y, r.width, r.height
        );
        if let Some(bounds) = scene.clips.get(&id) {
            coverage.push_str("<defs>");
            clip(&mut coverage, "board", &rect_path(*bounds));
            coverage.push_str("</defs><g clip-path=\"url(#board)\">");
        }
        write!(
            coverage,
            "<path d=\"{contour}\" transform=\"{transform}\" fill=\"white\" fill-opacity=\"{opacity}\" fill-rule=\"evenodd\"/>"
        )?;
        if scene.clips.contains_key(&id) {
            coverage.push_str("</g>");
        }
        coverage.push_str("</svg>");
        let s = self.scope;
        // Add complementary premultiplied regions. Source-over would apply the
        // outline coverage twice and leave translucent seams at antialiased edges.
        write!(
            defs,
            "<filter id=\"background-replace-{id}\" filterUnits=\"userSpaceOnUse\" primitiveUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" color-interpolation-filters=\"sRGB\"><feImage x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" href=\"data:image/png;base64,{}\" result=\"blurred\"/><feImage x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" href=\"data:image/svg+xml;base64,{}\" result=\"coverage\"/><feComposite in=\"SourceGraphic\" in2=\"coverage\" operator=\"out\" result=\"outside\"/><feComposite in=\"blurred\" in2=\"coverage\" operator=\"in\" result=\"inside\"/><feComposite in=\"outside\" in2=\"inside\" operator=\"arithmetic\" k2=\"1\" k3=\"1\"/></filter>",
            s.x,
            s.y,
            s.width,
            s.height,
            r.x,
            r.y,
            r.width,
            r.height,
            STANDARD.encode(pixels),
            r.x,
            r.y,
            r.width,
            r.height,
            STANDARD.encode(coverage),
        )?;
        let previous = std::mem::take(body);
        write!(
            body,
            "<g filter=\"url(#background-replace-{id})\">{previous}</g>"
        )?;
        Ok(())
    }
}
