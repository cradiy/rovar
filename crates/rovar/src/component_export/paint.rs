use super::*;
use crate::artboard::LinearGradient;
use gpui::{GradientKind, Rgba};

fn color(c: Rgba) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (c.r.clamp(0., 1.) * 255.).round() as u8,
        (c.g.clamp(0., 1.) * 255.).round() as u8,
        (c.b.clamp(0., 1.) * 255.).round() as u8
    )
}
fn stops(gradient: &LinearGradient) -> String {
    gradient
        .stops()
        .iter()
        .map(|s| {
            format!(
                "<stop offset=\"{}\" stop-color=\"{}\" stop-opacity=\"{}\"/>",
                s.position,
                color(s.color),
                s.color.a
            )
        })
        .collect()
}
fn sample(gradient: &LinearGradient, t: f32) -> Rgba {
    let stops = gradient.stops();
    if t <= stops[0].position {
        return stops[0].color;
    }
    for pair in stops.windows(2) {
        let [a, b] = [pair[0], pair[1]];
        if t <= b.position {
            let f = ((t - a.position) / (b.position - a.position).max(f32::EPSILON)).clamp(0., 1.);
            let mix = |x, y| x + (y - x) * f;
            return Rgba {
                r: mix(a.color.r, b.color.r),
                g: mix(a.color.g, b.color.g),
                b: mix(a.color.b, b.color.b),
                a: mix(a.color.a, b.color.a),
            };
        }
    }
    stops.last().unwrap().color
}
pub(super) fn paint(
    mode: FillMode,
    c: Rgba,
    gradient: &LinearGradient,
    r: Rect,
    defs: &mut String,
    id: &str,
) -> Result<String> {
    if mode != FillMode::Linear {
        return Ok(format!("fill=\"{}\" fill-opacity=\"{}\"", color(c), c.a));
    }
    let s = stops(gradient);
    let angle = (gradient.angle - 90.).to_radians();
    let (cx, cy) = (r.x + r.width / 2., r.y + r.height / 2.);
    let (w, h) = (r.width.max(0.001), r.height.max(0.001));
    match gradient.kind {
        GradientKind::Linear => {
            let (mut dx, mut dy) = (angle.cos(), angle.sin());
            if w > h {
                dy *= h / w;
            } else {
                dx *= w / h;
            }
            let length = if dx.abs() > dy.abs() { w } else { h };
            let scale = length / (dx.hypot(dy) * 2.);
            dx *= scale;
            dy *= scale;
            write!(
                defs,
                "<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">{s}</linearGradient>",
                cx - dx,
                cy - dy,
                cx + dx,
                cy + dy
            )?;
        }
        GradientKind::Radial => {
            write!(
                defs,
                "<radialGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"0\" cy=\"0\" r=\"1\" gradientTransform=\"translate({cx} {cy}) scale({} {})\">{s}</radialGradient>",
                w / 2.,
                h / 2.
            )?;
        }
        GradientKind::Diamond | GradientKind::Angular => {
            let mut content = String::new();
            if gradient.kind == GradientKind::Diamond {
                for (i, (x, y)) in [(1., 1.), (-1., 1.), (-1., -1.), (1., -1.)]
                    .into_iter()
                    .enumerate()
                {
                    write!(
                        defs,
                        "<linearGradient id=\"{id}-{i}\" gradientUnits=\"userSpaceOnUse\" x1=\"0\" y1=\"0\" x2=\"{}\" y2=\"{}\">{s}</linearGradient>",
                        x / 2.,
                        y / 2.
                    )?;
                    write!(
                        content,
                        "<path d=\"M 0 0 L {} 0 L 0 {} Z\" fill=\"url(#{id}-{i})\"/>",
                        x * 4.,
                        y * 4.
                    )?;
                }
            } else {
                // SVG has no conic gradient; small vector sectors preserve a standalone SVG.
                const SECTORS: usize = 1024;
                for i in 0..SECTORS {
                    let a = i as f32 / SECTORS as f32 * std::f32::consts::TAU;
                    let b = (i + 1) as f32 / SECTORS as f32 * std::f32::consts::TAU;
                    let c = sample(gradient, (i as f32 + 0.5) / SECTORS as f32);
                    write!(
                        content,
                        "<path d=\"M 0 0 L {} {} L {} {} Z\" fill=\"{}\" fill-opacity=\"{}\"/>",
                        a.cos() * 2.,
                        a.sin() * 2.,
                        b.cos() * 2.,
                        b.sin() * 2.,
                        color(c),
                        c.a
                    )?;
                }
            }
            write!(
                defs,
                "<pattern id=\"{id}\" patternUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{w}\" height=\"{h}\" viewBox=\"-1 -1 2 2\" preserveAspectRatio=\"none\"><g transform=\"rotate({})\" shape-rendering=\"crispEdges\">{content}</g></pattern>",
                r.x,
                r.y,
                gradient.angle - 90.
            )?;
        }
    }
    Ok(format!("fill=\"url(#{id})\""))
}

pub(super) fn image_uri(source: &AssetSource) -> Result<String> {
    use base64::Engine;
    let asset = crate::media::MediaAsset::from_cached(
        source.name.clone().into(),
        source.path.clone(),
        source.hash.clone(),
    )?;
    let Some(crate::media::MediaContent::Image(image)) = asset.content() else {
        anyhow::bail!("Expected an image");
    };
    let mut rgba = image.as_bytes(0).context("Missing image pixels")?.to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let image = image::RgbaImage::from_raw(asset.width, asset.height, rgba)
        .context("Invalid image data")?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    ))
}
