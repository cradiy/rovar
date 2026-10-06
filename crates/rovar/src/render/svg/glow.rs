use crate::{
    render::raster,
    scene::{
        artboard::Rect,
        effects::{Effect, Glow},
    },
};
use anyhow::{Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};

pub(super) fn images(
    defs: &str,
    source: &str,
    rect: Rect,
    effects: &[Effect],
    scale: f32,
    text: bool,
) -> Result<Vec<(Rect, String)>> {
    let glows: Vec<_> = effects
        .iter()
        .filter_map(|e| match e {
            Effect::Glow(g) if g.visible() => Some(g),
            _ => None,
        })
        .collect();
    if glows.is_empty() {
        return Ok(Vec::new());
    }
    let padding = glows.iter().map(|g| g.radius).fold(0., f32::max) + 2.;
    let width = ((rect.width + padding * 2.) * scale).ceil();
    let height = ((rect.height + padding * 2.) * scale).ceil();
    ensure!(
        scale.is_finite()
            && scale > 0.
            && width > 0.
            && height > 0.
            && width <= 16384.
            && height <= 16384.
            && width * height <= 16_000_000.,
        "Contour glow capture exceeds the export size limit"
    );
    let r = Rect {
        x: rect.x - padding,
        y: rect.y - padding,
        width: width / scale,
        height: height / scale,
    };
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\"><defs>{defs}</defs>{source}</svg>",
        r.width, r.height, r.x, r.y, r.width, r.height
    );
    let options = super::render_options(text)?;
    let pixels = raster::render(
        &svg,
        [width as u32, height as u32],
        scale,
        false,
        false,
        &options,
    )?;
    let pixels = image::load_from_memory(&pixels)?.to_rgba8();
    let mut images = Vec::new();
    for glow in glows {
        let light = rasterize(&pixels, glow, scale);
        let mut bytes = std::io::Cursor::new(Vec::new());
        light.write_to(&mut bytes, image::ImageFormat::Png)?;
        images.push((
            r,
            format!(
                "data:image/png;base64,{}",
                STANDARD.encode(bytes.into_inner())
            ),
        ));
    }
    Ok(images)
}

/// Subpixel alpha crossings followed by jump flooding, matching GPUI's distance field.
fn distances(image: &image::RgbaImage, threshold: f32) -> Vec<f32> {
    let (width, height) = image.dimensions();
    let (w, h) = (width as i32, height as i32);
    let alpha = |x: i32, y: i32| {
        if x < 0 || y < 0 || x >= w || y >= h {
            0.
        } else {
            image.get_pixel(x as u32, y as u32).0[3] as f32 / 255.
        }
    };
    let mut seeds = vec![[f32::INFINITY; 2]; (width * height) as usize];
    for y in 0..h {
        for x in 0..w {
            let a = alpha(x, y);
            let mut best = f32::INFINITY;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let b = alpha(x + dx, y + dy);
                if (a >= threshold) != (b >= threshold) {
                    let t = ((threshold - a) / (b - a)).clamp(0., 1.);
                    if t < best {
                        best = t;
                        seeds[(y * w + x) as usize] =
                            [x as f32 + dx as f32 * t, y as f32 + dy as f32 * t];
                    }
                }
            }
        }
    }
    let mut output = seeds.clone();
    let mut step = width.max(height).next_power_of_two() / 2;
    while step > 0 {
        for y in 0..h {
            for x in 0..w {
                let index = (y * w + x) as usize;
                let mut best = f32::INFINITY;
                let mut nearest = [f32::INFINITY; 2];
                for dy in [-1, 0, 1] {
                    for dx in [-1, 0, 1] {
                        let (nx, ny) = (x + dx * step as i32, y + dy * step as i32);
                        if nx < 0 || ny < 0 || nx >= w || ny >= h {
                            continue;
                        }
                        let p = seeds[(ny * w + nx) as usize];
                        let d = (p[0] - x as f32).powi(2) + (p[1] - y as f32).powi(2);
                        if d < best {
                            best = d;
                            nearest = p;
                        }
                    }
                }
                output[index] = nearest;
            }
        }
        std::mem::swap(&mut seeds, &mut output);
        step /= 2;
    }
    seeds
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let x = (i % width as usize) as f32;
            let y = (i / width as usize) as f32;
            (p[0] - x).hypot(p[1] - y)
        })
        .collect()
}

pub(crate) fn rasterize(source: &image::RgbaImage, glow: &Glow, scale: f32) -> image::RgbaImage {
    let distances = distances(source, glow.threshold);
    image::RgbaImage::from_fn(source.width(), source.height(), |x, y| {
        let alpha = glow.alpha(
            distances[(y * source.width() + x) as usize] / scale,
            1. / scale,
        );
        image::Rgba(
            [glow.color.r, glow.color.g, glow.color.b, alpha].map(|v| (v * 255.).round() as u8),
        )
    })
}
