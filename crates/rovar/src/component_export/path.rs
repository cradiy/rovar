use super::*;

pub(crate) fn expand(rect: Rect, amount: f32) -> Rect {
    Rect {
        x: rect.x - amount,
        y: rect.y - amount,
        width: rect.width + 2. * amount,
        height: rect.height + 2. * amount,
    }
}

pub(super) fn contour(shape: &Shape, offset: f32) -> String {
    if shape.kind.is_path() || shape.kind.is_polygon() {
        if shape.kind == ShapeKind::Arrow && shape.points.0.len() >= 2 {
            let a = shape.path_point(0);
            let b = shape.path_point(1);
            let [l, r] = crate::shape::arrow_wings(a, b, shape.stroke.width);
            return format!(
                "M {} {} L {} {} M {} {} L {} {} L {} {}",
                a.x, a.y, b.x, b.y, l.x, l.y, b.x, b.y, r.x, r.y
            );
        }
        let nodes = shape.editable_nodes();
        let mut d = String::new();
        if let Some(first) = nodes.first() {
            write!(d, "M {} {} ", first.anchor.x, first.anchor.y).unwrap();
            let count = if shape.editable_closed() {
                nodes.len()
            } else {
                nodes.len().saturating_sub(1)
            };
            for i in 0..count {
                let a = nodes[i];
                let b = nodes[(i + 1) % nodes.len()];
                write!(
                    d,
                    "C {} {} {} {} {} {} ",
                    a.outgoing.x, a.outgoing.y, b.incoming.x, b.incoming.y, b.anchor.x, b.anchor.y
                )
                .unwrap();
            }
            if shape.editable_closed() {
                d.push('Z');
            }
        }
        return d;
    }
    let Rect {
        x,
        y,
        width: w,
        height: h,
    } = expand(shape.rect, offset);
    if w <= 0. || h <= 0. {
        return String::new();
    }
    if shape.kind == ShapeKind::Ellipse {
        return format!(
            "M {} {} A {} {} 0 1 1 {} {} A {} {} 0 1 1 {} {} Z",
            x + w,
            y + h / 2.,
            w / 2.,
            h / 2.,
            x,
            y + h / 2.,
            w / 2.,
            h / 2.,
            x + w,
            y + h / 2.
        );
    }
    let [tl, tr, br, bl] = shape.displayed_radii().map(|r| {
        if r == 0. {
            0.
        } else {
            (r + offset).max(0.).min(w / 2.).min(h / 2.)
        }
    });
    format!(
        "M {} {y} H {} A {tr} {tr} 0 0 1 {} {} V {} A {br} {br} 0 0 1 {} {} H {} A {bl} {bl} 0 0 1 {x} {} V {} A {tl} {tl} 0 0 1 {} {y} Z",
        x + tl,
        x + w - tr,
        x + w,
        y + tr,
        y + h - br,
        x + w - br,
        y + h,
        x + bl,
        y + h - bl,
        y + tl,
        x + tl
    )
}
