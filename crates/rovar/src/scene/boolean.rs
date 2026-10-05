//! Non-destructive composition of closed contours. Operands stay in the layer tree.

use super::{
    artboard::Rect,
    layer::Hierarchy,
    shape::{Shape, ShapeKind},
};
use gpui::{Point, point};
use i_overlay::{
    core::{fill_rule::FillRule, overlay_rule::OverlayRule},
    float::single::SingleFloatOverlay,
};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Operation {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

impl Operation {
    pub const ALL: [Self; 4] = [Self::Union, Self::Subtract, Self::Intersect, Self::Exclude];
    pub fn label(self) -> &'static str {
        crate::i18n::t(self.key())
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Union => "boolean-union",
            Self::Subtract => "boolean-subtract",
            Self::Intersect => "boolean-intersect",
            Self::Exclude => "boolean-exclude",
        }
    }
    fn rule(self) -> OverlayRule {
        match self {
            Self::Union => OverlayRule::Union,
            Self::Subtract => OverlayRule::Difference,
            Self::Intersect => OverlayRule::Intersect,
            Self::Exclude => OverlayRule::Xor,
        }
    }
}

pub(crate) type Contours = Arc<Vec<Vec<Point<f32>>>>;

#[derive(Clone)]
pub(crate) struct Geometry {
    /// Appearance follows the bottom operand; its identity still belongs to that operand.
    pub source: usize,
    /// A transient shape provides shared paint, bounds and stroke behavior.
    pub shape: Shape,
    /// Normalized to shape.rect, with even-odd holes and disjoint contours.
    pub contours: Contours,
}

#[derive(Clone, PartialEq)]
enum Input {
    Shape(Box<Shape>),
    Group(Operation, Vec<Input>),
}

#[derive(Default)]
pub(crate) struct Cache(HashMap<usize, (Input, Geometry)>);

pub(crate) fn supported(shape: &Shape) -> bool {
    matches!(
        shape.kind,
        ShapeKind::Rectangle | ShapeKind::Ellipse | ShapeKind::Polygon | ShapeKind::Star
    ) || (shape.kind == ShapeKind::Bezier && shape.closed && shape.nodes.0.len() >= 2)
}

pub(crate) fn is_boolean(h: &Hierarchy, id: usize) -> bool {
    h.groups.get(&id).is_some_and(|g| g.boolean.is_some())
}

/// Suppress operands only when their composed ancestor is also being exported/painted.
pub(crate) fn consumed(h: &Hierarchy, id: usize, included: &BTreeSet<usize>) -> bool {
    let mut parent = h.parents.get(&id).copied();
    let mut seen = BTreeSet::new();
    while let Some(id) = parent.filter(|id| seen.insert(*id)) {
        if included.contains(&id) && is_boolean(h, id) {
            return true;
        }
        parent = h.parents.get(&id).copied();
    }
    false
}

fn input(h: &Hierarchy, shapes: &[Shape], id: usize, seen: &mut BTreeSet<usize>) -> Option<Input> {
    if !seen.insert(id) {
        return None;
    }
    if let Some(s) = shapes.iter().find(|s| s.id == id) {
        return supported(s).then(|| Input::Shape(Box::new(s.clone())));
    }
    let operation = h.groups.get(&id)?.boolean.unwrap_or(Operation::Union);
    let ranks: HashMap<_, _> = h.order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut ids: Vec<_> = h
        .parents
        .iter()
        .filter_map(|(child, parent)| (*parent == id).then_some(*child))
        .collect();
    ids.sort_by_key(|id| ranks.get(id).map_or((1, *id), |rank| (0, *rank)));
    let children = ids
        .into_iter()
        .filter(|id| {
            !shapes
                .iter()
                .find(|s| s.id == *id)
                .is_some_and(|s| s.layer.hidden)
                && !h.groups.get(id).is_some_and(|g| g.layer.hidden)
        })
        .filter_map(|id| input(h, shapes, id, seen))
        .collect();
    Some(Input::Group(operation, children))
}

type Polygons = Vec<Vec<[f64; 2]>>;
fn evaluate(input: &Input) -> Option<(Shape, Polygons)> {
    match input {
        Input::Shape(shape) => Some((*shape.clone(), vec![flatten(shape)])),
        Input::Group(operation, children) => {
            let mut children = children.iter().filter_map(evaluate);
            let (style, mut result) = children.next()?;
            // Normalize even a single remaining operand, including self intersections.
            let empty: Polygons = Vec::new();
            result = result
                .overlay(&empty, OverlayRule::Union, FillRule::EvenOdd)
                .into_iter()
                .flatten()
                .collect();
            for (_, operand) in children {
                result = result
                    .overlay(&operand, operation.rule(), FillRule::EvenOdd)
                    .into_iter()
                    .flatten()
                    .collect();
            }
            Some((style, result))
        }
    }
}

pub(super) fn flatten(shape: &Shape) -> Vec<[f64; 2]> {
    let nodes = shape.editable_nodes();
    let center = super::rotation::center(shape.rect);
    let p = |p| {
        let p = super::rotation::around(p, center, shape.layer.rotation);
        kurbo::Point::new(p.x as f64, p.y as f64)
    };
    let mut path = kurbo::BezPath::new();
    path.move_to(p(nodes[0].anchor));
    for (a, b) in nodes
        .iter()
        .zip(nodes.iter().cycle().skip(1))
        .take(nodes.len())
    {
        path.curve_to(p(a.outgoing), p(b.incoming), p(b.anchor));
    }
    path.close_path();
    let mut points = Vec::new();
    kurbo::flatten(path, 0.01, |el| {
        if let kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) = el {
            points.push([p.x, p.y]);
        }
    });
    points
}

impl Cache {
    pub fn retain(&mut self, h: &Hierarchy) {
        self.0.retain(|id, _| is_boolean(h, *id));
    }
    pub fn get(&mut self, h: &Hierarchy, shapes: &[Shape], id: usize) -> Option<Geometry> {
        if !is_boolean(h, id) {
            return None;
        }
        let key = input(h, shapes, id, &mut BTreeSet::new())?;
        if let Some((old, geometry)) = self.0.get(&id)
            && *old == key
        {
            return Some(geometry.clone());
        }
        let (mut shape, contours) = evaluate(&key)?;
        let source = shape.id;
        let mut bounds: Option<(f64, f64, f64, f64)> = None;
        for &[x, y] in contours.iter().flatten() {
            bounds = Some(bounds.map_or((x, y, x, y), |(l, t, r, b)| {
                (l.min(x), t.min(y), r.max(x), b.max(y))
            }));
        }
        let (x, y, r, b) = bounds.unwrap_or((
            shape.rect.x as f64,
            shape.rect.y as f64,
            shape.rect.x as f64,
            shape.rect.y as f64,
        ));
        shape.rect = Rect {
            x: x as f32,
            y: y as f32,
            width: ((r - x) as f32).max(0.001),
            height: ((b - y) as f32).max(0.001),
        };
        shape.id = id;
        shape.board = h.groups.get(&id).and_then(|g| g.board);
        shape.layer = h.groups.get(&id)?.layer;
        shape.layer.rotation = 0.;
        shape.kind = ShapeKind::Bezier;
        shape.closed = true;
        shape.stroke.align = super::shape::StrokeAlign::Center;
        let contours = Arc::new(
            contours
                .into_iter()
                .map(|c| {
                    c.into_iter()
                        .map(|[x, y]| {
                            point(
                                (x as f32 - shape.rect.x) / shape.rect.width,
                                (y as f32 - shape.rect.y) / shape.rect.height,
                            )
                        })
                        .collect()
                })
                .collect(),
        );
        let geometry = Geometry {
            source,
            shape,
            contours,
        };
        self.retain(h);
        self.0.insert(id, (key, geometry.clone()));
        Some(geometry)
    }
}

#[cfg(test)]
mod tests;
