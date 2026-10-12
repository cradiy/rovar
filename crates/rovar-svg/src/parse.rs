use crate::{Node, Point, Svg};
use usvg::{
    Paint,
    tiny_skia_path::{Path, PathSegment},
};

pub fn parse(bytes: &[u8], options: &usvg::Options<'_>) -> Result<Svg, String> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("SVG exceeds the 16 MiB import limit".into());
    }
    let xml = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let document = usvg::roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    for node in document.descendants().filter(|n| n.is_element()) {
        if matches!(
            node.tag_name().name(),
            "foreignObject" | "animate" | "animateTransform" | "animateMotion" | "set"
        ) {
            return Err("SVG animation and foreign content are not supported".into());
        }
        if node.attributes().any(|a| {
            a.name() == "href" && !a.value().starts_with('#') && !a.value().starts_with("data:")
        }) {
            return Err(
                "SVG references external content; embed it in the SVG before importing".into(),
            );
        }
    }
    let tree = usvg::Tree::from_data(bytes, options).map_err(|e| e.to_string())?;
    let size = tree.size().to_int_size();
    if size.width() > 16_384 || size.height() > 16_384 {
        return Err("SVG dimensions exceed 16384".into());
    }
    let viewport = [tree.size().width(), tree.size().height()];
    let editable = group(tree.root(), viewport).and_then(|nodes| {
        if nodes.is_empty() {
            Err("SVG has no editable paths".into())
        } else {
            Ok(nodes)
        }
    });
    Ok(Svg {
        width: size.width(),
        height: size.height(),
        size: viewport,
        source: tree.to_string(&Default::default()),
        editable,
    })
}

fn group(g: &usvg::Group, viewport: [f32; 2]) -> Result<Vec<Node>, String> {
    if g.clip_path().is_some() || g.mask().is_some() || !g.filters().is_empty() {
        return Err("Clipping, masks or filters cannot yet be converted to editable paths".into());
    }
    if g.blend_mode() != usvg::BlendMode::Normal || g.isolate() {
        return Err("SVG blend groups cannot yet be converted accurately".into());
    }
    let mut nodes = Vec::new();
    for node in g.children() {
        match node {
            usvg::Node::Group(child) => {
                let children = group(child, viewport)?;
                if child.id().is_empty() && child.opacity().get() == 1. && children.len() == 1 {
                    nodes.extend(children);
                } else if !children.is_empty() {
                    nodes.push(Node::Group {
                        name: child.id().into(),
                        opacity: child.opacity().get(),
                        children,
                    });
                }
            }
            usvg::Node::Path(p) if p.is_visible() => {
                let fill = || -> Result<Option<Node>, String> {
                    let Some(fill) = p.fill() else {
                        return Ok(None);
                    };
                    path(
                        p.data().clone(),
                        p,
                        fill.paint(),
                        fill.opacity().get(),
                        None,
                        viewport,
                    )
                    .map(Some)
                };
                let stroke = || -> Result<Option<Node>, String> {
                    let Some(stroke) = p.stroke() else {
                        return Ok(None);
                    };
                    if stroke.dasharray().is_some() {
                        return Err("Dashed SVG strokes cannot yet be converted accurately".into());
                    }
                    if let Some(width) = editable_stroke_width(stroke, p.abs_transform()) {
                        return path(
                            p.data().clone(),
                            p,
                            stroke.paint(),
                            stroke.opacity().get(),
                            Some(width),
                            viewport,
                        )
                        .map(Some);
                    }
                    let data = p
                        .data()
                        .stroke(&stroke.to_tiny_skia(), 1.)
                        .ok_or("Could not outline SVG stroke")?;
                    path(
                        data,
                        p,
                        stroke.paint(),
                        stroke.opacity().get(),
                        None,
                        viewport,
                    )
                    .map(Some)
                };
                let parts = if p.paint_order() == usvg::PaintOrder::FillAndStroke {
                    [fill()?, stroke()?]
                } else {
                    [stroke()?, fill()?]
                };
                nodes.extend(parts.into_iter().flatten());
            }
            usvg::Node::Path(_) => (),
            usvg::Node::Text(_) => {
                return Err(
                    "SVG text needs a vector material to preserve its font appearance".into(),
                );
            }
            usvg::Node::Image(_) => {
                return Err("Embedded images cannot be converted to editable paths".into());
            }
        }
    }
    Ok(nodes)
}

fn editable_stroke_width(stroke: &usvg::Stroke, transform: usvg::Transform) -> Option<f32> {
    if stroke.linecap() != usvg::LineCap::Round || stroke.linejoin() != usvg::LineJoin::Round {
        return None;
    }
    let x = transform.sx.hypot(transform.ky);
    let y = transform.kx.hypot(transform.sy);
    let dot = transform.sx * transform.kx + transform.ky * transform.sy;
    // A circular pen stays circular only under a similarity transform.
    if x <= 0. || y <= 0. || (x - y).abs() > 1e-6 * x.max(y) || dot.abs() > 1e-6 * x * y {
        return None;
    }
    Some(stroke.width().get() * x)
}

fn path(
    data: Path,
    original: &usvg::Path,
    paint: &Paint,
    opacity: f32,
    stroke_width: Option<f32>,
    viewport: [f32; 2],
) -> Result<Node, String> {
    let Paint::Color(color) = paint else {
        return Err("SVG gradients or patterns cannot yet be converted accurately".into());
    };
    let data = data
        .transform(original.abs_transform())
        .ok_or("Invalid SVG transform")?;
    let bounds = data
        .compute_tight_bounds()
        .ok_or("Invalid SVG path bounds")?;
    let radius = stroke_width.unwrap_or(0.) / 2.;
    // Use the centerline's exact extrema for round strokes. The stroked outline's
    // approximate Beziers can overshoot the viewport even for our own exports.
    if bounds.left() - radius < -0.001
        || bounds.top() - radius < -0.001
        || bounds.right() + radius > viewport[0] + 0.001
        || bounds.bottom() + radius > viewport[1] + 0.001
    {
        return Err("SVG viewport clipping cannot yet be converted accurately".into());
    }
    let mut points: Vec<Point> = Vec::new();
    let mut closed = stroke_width.is_none();
    let pair = |p: usvg::tiny_skia_path::Point| [p.x, p.y];
    let point = |p| Point {
        anchor: p,
        incoming: p,
        outgoing: p,
    };
    for segment in data.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                if !points.is_empty() {
                    return Err(
                        "Compound SVG paths need a vector material to preserve holes and winding"
                            .into(),
                    );
                }
                points.push(point(pair(p)));
            }
            PathSegment::LineTo(p) => points.push(point(pair(p))),
            PathSegment::QuadTo(control, end) => {
                let last = points.last_mut().ok_or("Invalid SVG path")?;
                last.outgoing = [
                    last.anchor[0] + (control.x - last.anchor[0]) * 2. / 3.,
                    last.anchor[1] + (control.y - last.anchor[1]) * 2. / 3.,
                ];
                let mut next = point(pair(end));
                next.incoming = [
                    end.x + (control.x - end.x) * 2. / 3.,
                    end.y + (control.y - end.y) * 2. / 3.,
                ];
                points.push(next);
            }
            PathSegment::CubicTo(a, b, end) => {
                points.last_mut().ok_or("Invalid SVG path")?.outgoing = pair(a);
                let mut next = point(pair(end));
                next.incoming = pair(b);
                points.push(next);
            }
            PathSegment::Close => closed = true,
        }
    }
    if closed && points.len() > 1 && points.first().unwrap().anchor == points.last().unwrap().anchor
    {
        let last = points.pop().unwrap();
        points[0].incoming = last.incoming;
    }
    if points.len() < 2 {
        return Err("Degenerate SVG path".into());
    }
    Ok(Node::Path {
        name: original.id().into(),
        color: [
            color.red as f32 / 255.,
            color.green as f32 / 255.,
            color.blue as f32 / 255.,
            opacity,
        ],
        points,
        closed,
        stroke_width,
    })
}
