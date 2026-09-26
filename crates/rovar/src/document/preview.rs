use super::*;
use crate::{
    artboard::{FillMode, LinearGradient},
    shape::ShapeKind,
};
use base64::Engine;
use std::fmt::Write as _;

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn color(color: gpui::Rgba) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (color.r * 255.).round() as u8,
        (color.g * 255.).round() as u8,
        (color.b * 255.).round() as u8
    )
}
fn png_uri(image: image::RgbaImage) -> Result<String> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut bytes, image::ImageFormat::Png)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    ))
}

fn paint(
    mode: FillMode,
    rgba: gpui::Rgba,
    gradient: &LinearGradient,
    defs: &mut String,
    id: &str,
) -> Result<String> {
    if mode != FillMode::Linear {
        return Ok(format!(
            "fill=\"{}\" fill-opacity=\"{}\"",
            color(rgba),
            rgba.a
        ));
    }
    let mut bitmap = image::RgbaImage::new(128, 128);
    let angle = gradient.angle.to_radians();
    let (sin, cos) = angle.sin_cos();
    for (x, y, pixel) in bitmap.enumerate_pixels_mut() {
        let x = (x as f32 + 0.5) / 128. - 0.5;
        let y = (y as f32 + 0.5) / 128. - 0.5;
        let u = cos * x + sin * y;
        let v = -sin * x + cos * y;
        let t = match gradient.kind {
            gpui::GradientKind::Linear => {
                0.5 + (sin * x - cos * y) / (sin.abs() + cos.abs()).max(0.001)
            }
            gpui::GradientKind::Radial => (x * x + y * y).sqrt() * 2.,
            gpui::GradientKind::Angular => {
                ((y.atan2(x) - angle) / std::f32::consts::TAU).rem_euclid(1.)
            }
            gpui::GradientKind::Diamond => (u.abs() + v.abs()) * 2.,
        }
        .clamp(0., 1.);
        let stops = gradient.stops();
        let right = stops
            .partition_point(|s| s.position < t)
            .min(stops.len() - 1);
        let left = right.saturating_sub(1);
        let (a, b) = (stops[left], stops[right]);
        let f = if a.position == b.position {
            0.
        } else {
            (t - a.position) / (b.position - a.position)
        }
        .clamp(0., 1.);
        let blend = |a: f32, b: f32| ((a + (b - a) * f) * 255.).round() as u8;
        *pixel = image::Rgba([
            blend(a.color.r, b.color.r),
            blend(a.color.g, b.color.g),
            blend(a.color.b, b.color.b),
            blend(a.color.a, b.color.a),
        ]);
    }
    let uri = png_uri(bitmap)?;
    write!(
        defs,
        "<pattern id=\"{id}\" width=\"1\" height=\"1\" patternContentUnits=\"objectBoundingBox\"><image width=\"1\" height=\"1\" preserveAspectRatio=\"none\" href=\"{uri}\"/></pattern>"
    )?;
    Ok(format!("fill=\"url(#{id})\""))
}

pub(super) fn render(json: &[u8], sources: &[AssetSource]) -> Result<Vec<u8>> {
    let doc = Document::decode(json)?;
    let mut items = BTreeMap::new();
    for b in &doc.boards {
        items.insert(b.id, (None, b.layer, b.rect));
    }
    for s in &doc.shapes {
        items.insert(s.id, (s.board, s.layer, s.rect));
    }
    for t in &doc.texts {
        items.insert(t.id, (t.board, t.layer, t.rect));
    }
    for (id, g) in &doc.hierarchy.groups {
        items.insert(
            *id,
            (
                g.board,
                g.layer,
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 0.,
                    height: 0.,
                },
            ),
        );
    }
    let mut children: BTreeMap<Option<usize>, Vec<usize>> = BTreeMap::new();
    for (id, (board, _, _)) in &items {
        children
            .entry(doc.hierarchy.parents.get(id).copied().or(*board))
            .or_default()
            .push(*id);
    }
    let ranks: BTreeMap<_, _> = doc
        .hierarchy
        .order
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();
    for ids in children.values_mut() {
        ids.sort_by_key(|id| ranks.get(id).map_or((1, *id), |rank| (0, *rank)));
    }
    let mut stack = children.get(&None).cloned().unwrap_or_default();
    stack.reverse();
    let mut order = Vec::new();
    while let Some(id) = stack.pop() {
        if items[&id].1.hidden {
            continue;
        }
        if !doc.hierarchy.groups.contains_key(&id) {
            order.push(id);
        }
        if let Some(children) = children.get(&Some(id)) {
            stack.extend(children.iter().rev());
        }
    }
    let world = |id: usize| {
        let (board, _, mut rect) = items[&id];
        if let Some(board) = board {
            rect.x += items[&board].2.x;
            rect.y += items[&board].2.y;
        }
        rect
    };
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    for id in &order {
        let rect = world(*id);
        let rect = crate::rotation::bounds(rect, items[id].1.rotation);
        bounds = Some(bounds.map_or(
            (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height),
            |(l, t, r, b)| {
                (
                    l.min(rect.x),
                    t.min(rect.y),
                    r.max(rect.x + rect.width),
                    b.max(rect.y + rect.height),
                )
            },
        ));
    }
    let (l, t, r, b) = bounds.unwrap_or((0., 0., 560., 336.));
    let pad = ((r - l).max(b - t) * 0.06).max(4.);
    let mut defs = String::new();
    let mut body = String::new();
    let mut images = BTreeMap::new();
    for source in sources {
        if images.contains_key(&source.hash) {
            continue;
        }
        if doc.assets.iter().any(|a| {
            a.hash == source.hash
                && !a.fill
                && doc
                    .shapes
                    .iter()
                    .any(|s| s.id == a.object && s.kind == ShapeKind::Video)
        }) {
            continue;
        }
        // Only thumbnail pixels leave the decoded image, never an external URL.
        if let Ok(asset) = crate::media::MediaAsset::from_cached(
            PathBuf::from(&source.name),
            source.path.clone(),
            source.hash.clone(),
        ) && let Some(crate::media::MediaContent::Image(image)) = asset.content()
            && let Some(bytes) = image.as_bytes(0)
            && let Some(mut pixels) =
                image::RgbaImage::from_raw(asset.width, asset.height, bytes.to_vec())
        {
            for pixel in pixels.pixels_mut() {
                pixel.0.swap(0, 2);
            }
            let pixels = image::imageops::thumbnail(&pixels, 560, 336);
            images.insert(source.hash.clone(), png_uri(pixels)?);
        }
    }
    for id in order {
        let rect = world(id);
        let rotation = items[&id].1.rotation;
        let cx = rect.x + rect.width / 2.;
        let cy = rect.y + rect.height / 2.;
        write!(body, "<g transform=\"rotate({rotation} {cx} {cy})\">")?;
        if let Some(text) = doc.texts.iter().find(|t| t.id == id) {
            let style = &text.styles.default;
            write!(
                defs,
                "<clipPath id=\"text-{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>",
                rect.x, rect.y, rect.width, rect.height
            )?;
            write!(body, "<g clip-path=\"url(#text-{id})\">")?;
            for (index, line) in text.content.lines().enumerate() {
                write!(
                    body,
                    "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" fill=\"{}\" fill-opacity=\"{}\">{}</text>",
                    rect.x,
                    rect.y + style.size * (1. + index as f32 * style.line_height),
                    escape(&style.family),
                    style.size,
                    style.weight,
                    color(style.color),
                    style.color.a,
                    escape(line)
                )?;
            }
            body.push_str("</g></g>");
            continue;
        }
        let board = doc.boards.iter().find(|b| b.id == id);
        let shape = doc.shapes.iter().find(|s| s.id == id);
        let (path, mode, rgba, gradient, enabled) = if let Some(board) = board {
            (
                format!(
                    "M {} {} h {} v {} h {} Z",
                    rect.x, rect.y, rect.width, rect.height, -rect.width
                ),
                board.fill_mode,
                board.color,
                &board.gradient,
                true,
            )
        } else {
            let mut shape = shape.unwrap().clone();
            shape.rect = rect;
            if shape.kind.is_media() {
                shape.kind = ShapeKind::Rectangle;
            }
            let nodes = shape.editable_nodes();
            let mut path = String::new();
            if let Some(first) = nodes.first() {
                write!(path, "M {} {} ", first.anchor.x, first.anchor.y)?;
                let count = if shape.editable_closed() {
                    nodes.len()
                } else {
                    nodes.len().saturating_sub(1)
                };
                for i in 0..count {
                    let a = nodes[i];
                    let b = nodes[(i + 1) % nodes.len()];
                    write!(
                        path,
                        "C {} {} {} {} {} {} ",
                        a.outgoing.x,
                        a.outgoing.y,
                        b.incoming.x,
                        b.incoming.y,
                        b.anchor.x,
                        b.anchor.y
                    )?;
                }
                if shape.editable_closed() {
                    path.push('Z');
                }
            }
            (
                path,
                shape.fill_mode,
                shape.color,
                &doc.shapes.iter().find(|s| s.id == id).unwrap().gradient,
                shape.fill_enabled && shape.can_fill(),
            )
        };
        let media = shape.is_some_and(|s| s.kind.is_media());
        if enabled && mode != FillMode::Image && !media {
            let fill = paint(mode, rgba, gradient, &mut defs, &format!("fill-{id}"))?;
            write!(body, "<path d=\"{path}\" {fill}/>")?;
        }
        if media || (enabled && mode == FillMode::Image) {
            let asset = doc
                .assets
                .iter()
                .find(|a| a.object == id && a.fill != media)
                .and_then(|a| images.get(&a.hash));
            if let Some(uri) = asset {
                let fill = board
                    .map(|b| &b.image_fill)
                    .or_else(|| shape.map(|s| &s.image_fill))
                    .unwrap();
                let fit = if media || fill.fit == crate::image_fill::ImageFit::Contain {
                    "xMidYMid meet"
                } else {
                    "xMidYMid slice"
                };
                let opacity = if media { 1. } else { fill.opacity };
                write!(
                    defs,
                    "<clipPath id=\"clip-{id}\"><path d=\"{path}\"/></clipPath>"
                )?;
                write!(
                    body,
                    "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"{fit}\" opacity=\"{opacity}\" clip-path=\"url(#clip-{id})\" href=\"{uri}\"/>",
                    rect.x, rect.y, rect.width, rect.height
                )?;
            } else if shape.is_some_and(|s| s.kind == ShapeKind::Video) {
                write!(
                    body,
                    "<path d=\"{path}\" fill=\"#282c36\"/><path d=\"M {} {} l {} {} l {} {} Z\" fill=\"#b4a2ee\"/>",
                    cx - rect.width * 0.1,
                    cy - rect.height * 0.16,
                    rect.width * 0.24,
                    rect.height * 0.16,
                    -rect.width * 0.24,
                    rect.height * 0.16
                )?;
            }
        }
        if let Some(shape) = shape
            && shape.stroke.enabled
        {
            let fill = paint(
                shape.stroke.fill_mode,
                shape.stroke.color,
                &shape.stroke.gradient,
                &mut defs,
                &format!("stroke-{id}"),
            )?
            .replace("fill", "stroke");
            write!(
                body,
                "<path d=\"{path}\" fill=\"none\" {fill} stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>",
                shape.stroke.width
            )?;
        }
        body.push_str("</g>");
    }
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"560\" height=\"336\" viewBox=\"{} {} {} {}\"><defs>{defs}</defs>{body}</svg>",
        l - pad,
        t - pad,
        (r - l + pad * 2.).max(1.),
        (b - t + pad * 2.).max(1.)
    );
    let mut options = resvg::usvg::Options::default();
    if !doc.texts.is_empty() {
        options.fontdb_mut().load_system_fonts();
    }
    let tree = resvg::usvg::Tree::from_str(&svg, &options)?;
    let mut pixels = resvg::tiny_skia::Pixmap::new(560, 336).context("Invalid thumbnail size")?;
    pixels.fill(resvg::tiny_skia::Color::from_rgba8(18, 20, 25, 255));
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixels.as_mut(),
    );
    Ok(pixels.encode_png()?)
}
