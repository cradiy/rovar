use gpui::{Point, point};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Node {
    pub anchor: Point<f32>,
    pub incoming: Point<f32>,
    pub outgoing: Point<f32>,
    pub smooth: bool,
}
impl Node {
    pub fn corner(anchor: Point<f32>) -> Self {
        Self {
            anchor,
            incoming: anchor,
            outgoing: anchor,
            smooth: false,
        }
    }
    pub fn map(self, f: impl Fn(Point<f32>) -> Point<f32>) -> Self {
        Self {
            anchor: f(self.anchor),
            incoming: f(self.incoming),
            outgoing: f(self.outgoing),
            smooth: self.smooth,
        }
    }
}

mod editing;
pub(crate) use editing::{nearest_segment, smooth_node, split_segment};

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Nodes(pub Arc<Vec<Node>>);
impl PartialEq for Nodes {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

pub(crate) fn distance_to_segment(p: Point<f32>, a: Point<f32>, b: Point<f32>) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let length2 = ab.x * ab.x + ab.y * ab.y;
    let t = if length2 > 0. {
        ((ap.x * ab.x + ap.y * ab.y) / length2).clamp(0., 1.)
    } else {
        0.
    };
    (p.x - a.x - ab.x * t).hypot(p.y - a.y - ab.y * t)
}

/// Points at curve endpoints and derivative roots give the actual curve bounds.
pub(crate) fn extrema(nodes: &[Node], closed: bool) -> Vec<Point<f32>> {
    let mut points: Vec<_> = nodes.iter().map(|n| n.anchor).collect();
    let segments = nodes.len().saturating_sub(1) + usize::from(closed && nodes.len() > 1);
    for i in 0..segments {
        let a = nodes[i];
        let b = nodes[(i + 1) % nodes.len()];
        let p = [a.anchor, a.outgoing, b.incoming, b.anchor];
        for v in [
            [p[0].x, p[1].x, p[2].x, p[3].x],
            [p[0].y, p[1].y, p[2].y, p[3].y],
        ] {
            let a = -v[0] + 3. * v[1] - 3. * v[2] + v[3];
            let b = 2. * (v[0] - 2. * v[1] + v[2]);
            let c = v[1] - v[0];
            let mut add = |t: f32| {
                if t > 0. && t < 1. {
                    let u = 1. - t;
                    points.push(point(
                        u * u * u * p[0].x
                            + 3. * u * u * t * p[1].x
                            + 3. * u * t * t * p[2].x
                            + t * t * t * p[3].x,
                        u * u * u * p[0].y
                            + 3. * u * u * t * p[1].y
                            + 3. * u * t * t * p[2].y
                            + t * t * t * p[3].y,
                    ));
                }
            };
            if a.abs() < 0.000001 {
                if b.abs() > 0.000001 {
                    add(-c / b);
                }
            } else {
                let discriminant = b * b - 4. * a * c;
                if discriminant >= 0. {
                    let root = discriminant.sqrt();
                    add((-b + root) / (2. * a));
                    add((-b - root) / (2. * a));
                }
            }
        }
    }
    points
}
