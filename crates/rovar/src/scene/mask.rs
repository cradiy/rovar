//! Vector clipping uses geometry only; mask fills, opacity and effects are ignored.
use super::{
    artboard::{Artboard, Rect},
    boolean,
    layer::Hierarchy,
    shape::Shape,
};
use gpui::{Point, point};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn source(h: &Hierarchy, group: usize) -> Option<usize> {
    h.groups
        .get(&group)?
        .mask
        .filter(|id| h.parents.get(id) == Some(&group))
}

/// Ancestor masks apply only when the group belongs to the requested render root.
pub(crate) fn ancestors(h: &Hierarchy, id: usize, included: &BTreeSet<usize>) -> Vec<usize> {
    let mut result = Vec::new();
    let mut next = h.parents.get(&id).copied();
    let mut seen = BTreeSet::new();
    while let Some(parent) = next.filter(|p| seen.insert(*p)) {
        if included.contains(&parent) && source(h, parent).is_some() {
            result.push(parent);
        }
        next = h.parents.get(&parent).copied();
    }
    result
}

pub(crate) fn is_source(h: &Hierarchy, id: usize, included: &BTreeSet<usize>) -> bool {
    let mut child = id;
    let mut seen = BTreeSet::new();
    while let Some(parent) = h.parents.get(&child).filter(|p| seen.insert(**p)) {
        if included.contains(parent) && source(h, *parent) == Some(child) {
            return true;
        }
        child = *parent;
    }
    false
}

#[derive(Clone, Default)]
pub(crate) struct Outline(pub Vec<Vec<Point<f32>>>);

impl Outline {
    pub fn contains(&self, p: Point<f32>) -> bool {
        let mut inside = false;
        for contour in &self.0 {
            for (a, b) in contour
                .iter()
                .zip(contour.iter().cycle().skip(1))
                .take(contour.len())
            {
                if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
                {
                    inside = !inside;
                }
            }
        }
        inside
    }
    pub fn bounds(&self) -> Option<Rect> {
        let mut points = self.0.iter().flatten();
        let first = points.next()?;
        let (mut l, mut t, mut r, mut b) = (first.x, first.y, first.x, first.y);
        for p in points {
            l = l.min(p.x);
            t = t.min(p.y);
            r = r.max(p.x);
            b = b.max(p.y);
        }
        Some(Rect {
            x: l,
            y: t,
            width: r - l,
            height: b - t,
        })
    }
}

pub(crate) fn outline(
    h: &Hierarchy,
    shapes: &[Shape],
    boards: &[Artboard],
    cache: &mut boolean::Cache,
    id: usize,
) -> Outline {
    let (board, mut contours) =
        if let Some(s) = shapes.iter().find(|s| s.id == id && boolean::supported(s)) {
            (
                s.board,
                vec![
                    boolean::flatten(s)
                        .into_iter()
                        .map(|[x, y]| point(x as f32, y as f32))
                        .collect::<Vec<_>>(),
                ],
            )
        } else if let Some(g) = cache.get(h, shapes, id) {
            (
                g.shape.board,
                g.contours
                    .iter()
                    .map(|c| {
                        c.iter()
                            .map(|p| {
                                point(
                                    g.shape.rect.x + p.x * g.shape.rect.width,
                                    g.shape.rect.y + p.y * g.shape.rect.height,
                                )
                            })
                            .collect()
                    })
                    .collect(),
            )
        } else {
            return Outline::default();
        };
    if let Some(b) = board.and_then(|id| boards.iter().find(|b| b.id == id)) {
        for p in contours.iter_mut().flatten() {
            p.x += b.rect.x;
            p.y += b.rect.y;
        }
    }
    Outline(contours)
}

pub(crate) fn outlines(
    h: &Hierarchy,
    shapes: &[Shape],
    boards: &[Artboard],
    cache: &mut boolean::Cache,
) -> BTreeMap<usize, Outline> {
    h.groups
        .keys()
        .filter_map(|id| source(h, *id).map(|s| (*id, outline(h, shapes, boards, cache, s))))
        .collect()
}

pub(crate) fn clip_bounds(
    mut rect: Rect,
    masks: impl IntoIterator<Item = Option<Rect>>,
) -> Option<Rect> {
    for mask in masks {
        let mask = mask?;
        let r = (rect.x + rect.width).min(mask.x + mask.width);
        let b = (rect.y + rect.height).min(mask.y + mask.height);
        rect.x = rect.x.max(mask.x);
        rect.y = rect.y.max(mask.y);
        rect.width = (r - rect.x).max(0.);
        rect.height = (b - rect.y).max(0.);
        if rect.width == 0. || rect.height == 0. {
            return None;
        }
    }
    Some(rect)
}

pub(crate) fn validate(h: &Hierarchy, shapes: &[Shape]) -> anyhow::Result<()> {
    for (id, g) in &h.groups {
        if let Some(mask) = g.mask {
            anyhow::ensure!(
                g.boolean.is_none() && !h.layouts.contains_key(id),
                "Mask groups cannot be Boolean or auto layout containers"
            );
            anyhow::ensure!(
                h.parents.get(&mask) == Some(id),
                "Mask must be a direct child of its group"
            );
            anyhow::ensure!(
                boolean::is_boolean(h, mask)
                    || shapes.iter().any(|s| s.id == mask
                        && (boolean::supported(s) || s.kind == super::shape::ShapeKind::Bezier)),
                "Mask requires a closed vector shape"
            );
        }
    }
    Ok(())
}
