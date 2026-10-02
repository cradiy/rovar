use super::*;

fn lerp(a: Point<f32>, b: Point<f32>, t: f32) -> Point<f32> {
    a + (b - a) * t
}

fn split(p: [Point<f32>; 4], t: f32) -> ([Point<f32>; 4], [Point<f32>; 4]) {
    let a = lerp(p[0], p[1], t);
    let b = lerp(p[1], p[2], t);
    let c = lerp(p[2], p[3], t);
    let d = lerp(a, b, t);
    let e = lerp(b, c, t);
    let f = lerp(d, e, t);
    ([p[0], a, d, f], [f, e, c, p[3]])
}

/// De Casteljau subdivision preserves the complete curve, including the closing edge.
pub(crate) fn split_segment(nodes: &mut Vec<Node>, index: usize, t: f32) -> usize {
    let next = (index + 1) % nodes.len();
    let (a, b) = split(
        [
            nodes[index].anchor,
            nodes[index].outgoing,
            nodes[next].incoming,
            nodes[next].anchor,
        ],
        t,
    );
    nodes[index].outgoing = a[1];
    nodes[next].incoming = b[2];
    nodes.insert(
        index + 1,
        Node {
            anchor: a[3],
            incoming: a[2],
            outgoing: b[1],
            smooth: true,
        },
    );
    index + 1
}

pub(crate) fn smooth_node(nodes: &mut [Node], index: usize, closed: bool) {
    let anchor = nodes[index].anchor;
    let prev = if index > 0 {
        nodes[index - 1].anchor
    } else if closed {
        nodes[nodes.len() - 1].anchor
    } else {
        anchor
    };
    let next = if index + 1 < nodes.len() {
        nodes[index + 1].anchor
    } else if closed {
        nodes[0].anchor
    } else {
        anchor
    };
    let mut direction = next - prev;
    if direction.x.hypot(direction.y) < 0.0001 {
        direction = next - anchor;
    }
    let length = direction.x.hypot(direction.y);
    if length < 0.0001 {
        return;
    }
    direction = direction / length;
    nodes[index].incoming = anchor - direction * (anchor.x - prev.x).hypot(anchor.y - prev.y) / 3.;
    nodes[index].outgoing = anchor + direction * (next.x - anchor.x).hypot(next.y - anchor.y) / 3.;
    nodes[index].smooth = true;
}

/// Adaptive subdivision in document units; pruning keeps distant paths cheap.
/// Flatness is tied to hit tolerance so zoom does not change picking accuracy.
pub(crate) fn nearest_segment(
    nodes: &[Node],
    closed: bool,
    position: Point<f32>,
    tolerance: f32,
) -> Option<(usize, f32)> {
    let count = nodes.len().saturating_sub(1) + usize::from(closed && nodes.len() > 1);
    let mut best = tolerance;
    let mut result = None;
    for index in 0..count {
        let a = nodes[index];
        let b = nodes[(index + 1) % nodes.len()];
        let mut stack = vec![([a.anchor, a.outgoing, b.incoming, b.anchor], 0., 1., 0)];
        while let Some((p, lo, hi, depth)) = stack.pop() {
            let min = p.iter().fold(point(f32::INFINITY, f32::INFINITY), |a, b| {
                point(a.x.min(b.x), a.y.min(b.y))
            });
            let max = p
                .iter()
                .fold(point(f32::NEG_INFINITY, f32::NEG_INFINITY), |a, b| {
                    point(a.x.max(b.x), a.y.max(b.y))
                });
            let dx = (min.x - position.x).max(0.).max(position.x - max.x);
            let dy = (min.y - position.y).max(0.).max(position.y - max.y);
            if dx.hypot(dy) > best {
                continue;
            }
            let flatness =
                distance_to_segment(p[1], p[0], p[3]).max(distance_to_segment(p[2], p[0], p[3]));
            if flatness <= tolerance / 32. || depth >= 16 {
                // Refine within this leaf; subdivision may have a non-uniform parameter speed.
                let mut left = 0.;
                let mut right = 1.;
                let distance = |t| {
                    let q = split(p, t).0[3];
                    (q.x - position.x).hypot(q.y - position.y)
                };
                for _ in 0..20 {
                    let u = left + (right - left) / 3.;
                    let v = right - (right - left) / 3.;
                    if distance(u) < distance(v) {
                        right = v;
                    } else {
                        left = u;
                    }
                }
                let t = (left + right) / 2.;
                let distance = distance(t);
                if distance <= best {
                    best = distance;
                    result = Some((index, lo + (hi - lo) * t));
                }
            } else {
                let (a, b) = split(p, 0.5);
                let mid = (lo + hi) / 2.;
                stack.push((b, mid, hi, depth + 1));
                stack.push((a, lo, mid, depth + 1));
            }
        }
    }
    result.filter(|(_, t)| *t > 0.0001 && *t < 0.9999)
}
