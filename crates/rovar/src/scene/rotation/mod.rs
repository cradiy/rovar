use crate::scene::artboard::Rect;
#[cfg(test)]
use gpui::{Bounds, Pixels, px};
use gpui::{Point, point};

pub fn normalize(angle: f32) -> f32 {
    (angle + 180.).rem_euclid(360.) - 180.
}

pub fn vector(p: Point<f32>, angle: f32) -> Point<f32> {
    let (s, c) = angle.to_radians().sin_cos();
    point(c * p.x - s * p.y, s * p.x + c * p.y)
}

pub fn center(r: Rect) -> Point<f32> {
    point(r.x + r.width / 2., r.y + r.height / 2.)
}

pub fn around(p: Point<f32>, center: Point<f32>, angle: f32) -> Point<f32> {
    center + vector(p - center, angle)
}

#[cfg(test)]
pub fn pixels(p: Point<Pixels>, center: Point<Pixels>, angle: f32) -> Point<Pixels> {
    center + vector((p - center).map(f32::from), angle).map(px)
}

pub fn bounds(rect: Rect, angle: f32) -> Rect {
    let (s, c) = angle.to_radians().sin_cos();
    let width = c.abs() * rect.width + s.abs() * rect.height;
    let height = s.abs() * rect.width + c.abs() * rect.height;
    Rect {
        x: rect.x + (rect.width - width) / 2.,
        y: rect.y + (rect.height - height) / 2.,
        width,
        height,
    }
}

/// Rectangle intersection on both rectangles' edge normals, excluding the empty
/// corners of a rotated object's axis-aligned bounding box from marquee hits.
pub fn intersects(rect: Rect, angle: f32, marquee: Rect) -> bool {
    let a = center(rect);
    let b = center(marquee);
    let x = vector(point(1., 0.), angle);
    let y = vector(point(0., 1.), angle);
    let dot = |a: Point<f32>, b: Point<f32>| a.x * b.x + a.y * b.y;
    [point(1., 0.), point(0., 1.), x, y]
        .into_iter()
        .all(|axis| {
            dot(b - a, axis).abs()
                <= (dot(x, axis).abs() * rect.width
                    + dot(y, axis).abs() * rect.height
                    + axis.x.abs() * marquee.width
                    + axis.y.abs() * marquee.height)
                    / 2.
        })
}

#[cfg(test)]
pub fn pixel_bounds(rect: Bounds<Pixels>, pivot: Point<Pixels>, angle: f32) -> Bounds<Pixels> {
    let c = pixels(rect.center(), pivot, angle);
    let r = bounds(
        Rect {
            x: 0.,
            y: 0.,
            width: rect.size.width.into(),
            height: rect.size.height.into(),
        },
        angle,
    );
    Bounds::new(
        c - point(px(r.width / 2.), px(r.height / 2.)),
        gpui::size(px(r.width), px(r.height)),
    )
}

mod surface;
pub use surface::surface;
