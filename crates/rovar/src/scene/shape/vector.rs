use super::*;
use crate::scene::bezier::Node;

impl Shape {
    pub fn editable_closed(&self) -> bool {
        matches!(self.kind, ShapeKind::Rectangle | ShapeKind::Ellipse)
            || self.kind.is_polygon()
            || self.closed
    }

    /// Generate editing handles without changing the primitive or its history.
    pub fn editable_nodes(&self) -> Vec<Node> {
        if self.kind.is_media() {
            return Vec::new();
        }
        if self.kind.is_polygon() {
            return self
                .polygon_points()
                .into_iter()
                .map(|p| {
                    Node::corner(point(
                        self.rect.x + p.x * self.rect.width,
                        self.rect.y + p.y * self.rect.height,
                    ))
                })
                .collect();
        }
        if self.kind == ShapeKind::Arrow && self.points.0.len() >= 2 {
            let a = self.path_point(0);
            let b = self.path_point(1);
            let [left, right] = arrow_wings(a, b, self.stroke.width);
            return [a, b, left, b, right].map(Node::corner).to_vec();
        }
        if self.kind == ShapeKind::Bezier {
            return self.world_nodes();
        }
        if self.kind.is_path() {
            return (0..self.points.0.len())
                .map(|i| Node::corner(self.path_point(i)))
                .collect();
        }
        let Rect {
            x,
            y,
            width: w,
            height: h,
        } = self.rect;
        const K: f32 = 0.552_284_8;
        if self.kind == ShapeKind::Ellipse {
            let rx = w / 2.;
            let ry = h / 2.;
            return [
                (point(x + rx, y), point(K * rx, 0.)),
                (point(x + w, y + ry), point(0., K * ry)),
                (point(x + rx, y + h), point(-K * rx, 0.)),
                (point(x, y + ry), point(0., -K * ry)),
            ]
            .into_iter()
            .map(|(p, tangent)| Node {
                anchor: p,
                incoming: p - tangent,
                outgoing: p + tangent,
                smooth: true,
            })
            .collect();
        }
        let [tl, tr, br, bl] = self.displayed_radii();
        let mut nodes = Vec::new();
        for (a, b, ca, cb, radius) in [
            (
                point(x, y + tl),
                point(x + tl, y),
                point(x, y + tl * (1. - K)),
                point(x + tl * (1. - K), y),
                tl,
            ),
            (
                point(x + w - tr, y),
                point(x + w, y + tr),
                point(x + w - tr * (1. - K), y),
                point(x + w, y + tr * (1. - K)),
                tr,
            ),
            (
                point(x + w, y + h - br),
                point(x + w - br, y + h),
                point(x + w, y + h - br * (1. - K)),
                point(x + w - br * (1. - K), y + h),
                br,
            ),
            (
                point(x + bl, y + h),
                point(x, y + h - bl),
                point(x + bl * (1. - K), y + h),
                point(x, y + h - bl * (1. - K)),
                bl,
            ),
        ] {
            let mut start = Node::corner(a);
            if radius > 0. {
                let mut end = Node::corner(b);
                start.outgoing = ca;
                end.incoming = cb;
                nodes.extend([start, end]);
            } else {
                nodes.push(start);
            }
        }
        nodes
    }

    pub fn apply_vector_nodes(&mut self, nodes: &[Node], closed: bool) {
        if self.kind != ShapeKind::Bezier {
            self.stroke.align = StrokeAlign::Center;
        }
        self.kind = ShapeKind::Bezier;
        self.set_bezier(nodes, closed);
    }
}

pub(crate) fn arrow_wings(a: Point<f32>, b: Point<f32>, width: f32) -> [Point<f32>; 2] {
    let d = b - a;
    let length = d.x.hypot(d.y);
    // A compact, right-angle chevron: each wing sits 45 degrees from the shaft.
    // Limit its depth on short arrows so the head does not dominate the line.
    let head = (width * 3.5).max(10.).min(length * 0.35);
    let d = d / length.max(0.001);
    let base = b - d * head;
    let normal = point(-d.y, d.x) * head;
    [base + normal, base - normal]
}

impl Shape {
    pub fn polygon_points(&self) -> Vec<Point<f32>> {
        let count = self.vertices.clamp(3, 60);
        let star = self.kind == ShapeKind::Star;
        let total = count * if star { 2 } else { 1 };
        let mut points: Vec<_> = (0..total)
            .map(|i| {
                let angle =
                    std::f32::consts::TAU * i as f32 / total as f32 - std::f32::consts::FRAC_PI_2;
                let r = if star && i % 2 == 1 {
                    self.inner_radius
                } else {
                    1.
                };
                point(angle.cos() * r, angle.sin() * r)
            })
            .collect();
        let min = points
            .iter()
            .fold(point(f32::INFINITY, f32::INFINITY), |a, p| {
                point(a.x.min(p.x), a.y.min(p.y))
            });
        let max = points
            .iter()
            .fold(point(f32::NEG_INFINITY, f32::NEG_INFINITY), |a, p| {
                point(a.x.max(p.x), a.y.max(p.y))
            });
        for p in &mut points {
            p.x = (p.x - min.x) / (max.x - min.x);
            p.y = (p.y - min.y) / (max.y - min.y);
            if self.mirrored[0] {
                p.x = 1. - p.x;
            }
            if self.mirrored[1] {
                p.y = 1. - p.y;
            }
        }
        points
    }
}
