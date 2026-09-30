use super::*;
mod resize;
mod spacing;
use super::measurement::{Dimension, dimension_overlay};

#[derive(Clone, Copy)]
struct Target {
    value: f32,
    low: f32,
    high: f32,
}
#[derive(Clone, Copy)]
struct Guide {
    axis: usize,
    value: f32,
    low: f32,
    high: f32,
}
pub(in crate::workspace) struct Snapping {
    pub enabled: bool,
    pub bypass: bool,
    original: Option<Rect>,
    targets: [Vec<Target>; 2],
    guides: Vec<Guide>,
    neighbors: [Vec<Rect>; 2],
    gaps: Vec<Dimension>,
}
impl Default for Snapping {
    fn default() -> Self {
        Self {
            enabled: true,
            bypass: false,
            original: None,
            targets: Default::default(),
            guides: Vec::new(),
            neighbors: Default::default(),
            gaps: Vec::new(),
        }
    }
}
impl Snapping {
    pub(in crate::workspace) fn clear_guides(&mut self) {
        self.guides.clear();
        self.gaps.clear();
    }

    pub(in crate::workspace) fn clear(&mut self) {
        self.original = None;
        self.targets.iter_mut().for_each(Vec::clear);
        self.guides.clear();
        self.neighbors.iter_mut().for_each(Vec::clear);
        self.gaps.clear();
    }
}
impl Workspace {
    pub(in crate::workspace) fn begin_snapping(&mut self, kind: GestureKind) {
        self.snapping.clear();
        if !self.snapping.enabled {
            return;
        }
        let ids: BTreeSet<_> = match kind {
            GestureKind::SelectionMove | GestureKind::SelectionResize { .. } => {
                self.selection_ids()
            }
            GestureKind::Move { id, .. }
            | GestureKind::Resize { id, .. }
            | GestureKind::Text { id, .. }
            | GestureKind::Shape { id, .. }
            | GestureKind::LineEnd { id, .. } => BTreeSet::from([id]),
            GestureKind::Draw
                if matches!(
                    self.draw_tool,
                    Some(
                        DrawTool::Board
                            | DrawTool::Text
                            | DrawTool::Shape(
                                ShapeKind::Rectangle
                                    | ShapeKind::Ellipse
                                    | ShapeKind::Line
                                    | ShapeKind::Arrow
                                    | ShapeKind::Polygon
                                    | ShapeKind::Star
                            )
                    )
                ) =>
            {
                BTreeSet::new()
            }
            _ => return,
        };
        self.snapping.original = ids
            .iter()
            .filter_map(|id| self.world_bounds(*id))
            .reduce(union);
        let moving = self.descendants(&ids);
        let parent = ids.first().map(|id| self.layer_parent(*id));
        let measure_spacing = matches!(
            kind,
            GestureKind::SelectionMove
                | GestureKind::Move { .. }
                | GestureKind::Text { handle: None, .. }
                | GestureKind::Shape { handle: None, .. }
        ) && parent
            .is_some_and(|parent| ids.iter().all(|id| self.layer_parent(*id) == parent));
        // Do not attract an individual member back to its own group bounds.
        let ancestors: BTreeSet<_> = ids
            .iter()
            .flat_map(|id| self.ancestors(*id))
            .filter(|id| self.hierarchy.groups.contains_key(id))
            .collect();
        let mut references = self.paint_order();
        references.extend(self.hierarchy.groups.keys().copied().filter(|id| {
            self.layer_info(*id)
                .is_some_and(|(own, parent)| !self.effective_layer(own, parent).hidden)
        }));
        for id in references
            .into_iter()
            .filter(|id| !moving.contains(id) && !ancestors.contains(id))
        {
            let Some(r) = self.world_bounds(id) else {
                continue;
            };
            if measure_spacing && Some(self.layer_parent(id)) == parent {
                for neighbors in &mut self.snapping.neighbors {
                    neighbors.push(r);
                }
            }
            for (axis, values, low, high) in [
                (
                    0,
                    [r.x, r.x + r.width / 2., r.x + r.width],
                    r.y,
                    r.y + r.height,
                ),
                (
                    1,
                    [r.y, r.y + r.height / 2., r.y + r.height],
                    r.x,
                    r.x + r.width,
                ),
            ] {
                for value in values {
                    self.snapping.targets[axis].push(Target { value, low, high });
                }
            }
        }
        for targets in &mut self.snapping.targets {
            targets.sort_by(|a, b| a.value.total_cmp(&b.value));
        }
        for (axis, neighbors) in self.snapping.neighbors.iter_mut().enumerate() {
            neighbors.sort_by(|a, b| {
                super::measurement::interval(*a, axis)
                    .0
                    .total_cmp(&super::measurement::interval(*b, axis).0)
            });
        }
    }
    fn nearest_target(&self, axis: usize, value: f32, tolerance: f32) -> Option<Target> {
        let targets = &self.snapping.targets[axis];
        let at = targets.partition_point(|t| t.value < value);
        [at.checked_sub(1), (at < targets.len()).then_some(at)]
            .into_iter()
            .flatten()
            .map(|i| targets[i])
            .filter(|t| (t.value - value).abs() <= tolerance)
            .min_by(|a, b| (a.value - value).abs().total_cmp(&(b.value - value).abs()))
    }

    /// Snap a moving corner or endpoint. A direction constrains every candidate
    /// to the same ray, preserving a square, aspect ratio, or line angle.
    pub(in crate::workspace) fn snap_point(
        &mut self,
        p: Point<f32>,
        axes: [bool; 2],
        direction: Option<Point<f32>>,
    ) -> Point<f32> {
        self.snapping.guides.clear();
        if !self.snapping.enabled || self.snapping.bypass {
            return p;
        }
        let tolerance = 6. / self.view.zoom;
        let mut result = p;
        let mut best: Option<Point<f32>> = None;
        for (axis, enabled) in axes.into_iter().enumerate() {
            if !enabled {
                continue;
            }
            let value = if axis == 0 { p.x } else { p.y };
            let Some(target) = self.nearest_target(axis, value, tolerance) else {
                continue;
            };
            let correction = target.value - value;
            if let Some(direction) = direction {
                let component = if axis == 0 { direction.x } else { direction.y };
                if component.abs() < 0.00001 {
                    continue;
                }
                let delta = direction * (correction / component);
                let distance = delta.x.hypot(delta.y);
                if distance <= tolerance && best.is_none_or(|old| distance < old.x.hypot(old.y)) {
                    best = Some(delta);
                }
            } else if axis == 0 {
                result.x = target.value;
            } else {
                result.y = target.value;
            }
        }
        if let Some(delta) = best {
            result = p + delta;
        }
        self.point_guides(result, axes);
        result
    }

    pub(in crate::workspace) fn point_guides(&mut self, p: Point<f32>, axes: [bool; 2]) {
        self.snapping.guides.clear();
        if !self.snapping.enabled || self.snapping.bypass {
            return;
        }
        for (axis, enabled) in axes.into_iter().enumerate() {
            if !enabled {
                continue;
            }
            if let Some(target) = self.nearest_target(
                axis,
                if axis == 0 { p.x } else { p.y },
                0.01 / self.view.zoom,
            ) {
                let perpendicular = if axis == 0 { p.y } else { p.x };
                self.snapping.guides.push(Guide {
                    axis,
                    value: target.value,
                    low: target.low.min(perpendicular),
                    high: target.high.max(perpendicular),
                });
            }
        }
    }

    pub(in crate::workspace) fn extend_snap_guides(&mut self, rect: Rect) {
        for guide in &mut self.snapping.guides {
            let (low, high) = if guide.axis == 0 {
                (rect.y, rect.y + rect.height)
            } else {
                (rect.x, rect.x + rect.width)
            };
            guide.low = guide.low.min(low);
            guide.high = guide.high.max(high);
        }
    }

    pub(in crate::workspace) fn snap_delta(&mut self, delta: Point<f32>) -> Point<f32> {
        self.snapping.guides.clear();
        self.snapping.gaps.clear();
        if (delta.x.abs() + delta.y.abs()) * self.view.zoom < 3. {
            return delta;
        }
        let Some(r) = self
            .snapping
            .original
            .filter(|_| self.snapping.enabled && !self.snapping.bypass)
        else {
            return delta;
        };
        let mut snapped = delta;
        for axis in 0..2 {
            let (start, len, offset) = if axis == 0 {
                (r.x, r.width, delta.x)
            } else {
                (r.y, r.height, delta.y)
            };
            let targets = &self.snapping.targets[axis];
            let mut closest: Option<(f32, Target)> = None;
            for value in [
                start + offset,
                start + len / 2. + offset,
                start + len + offset,
            ] {
                let at = targets.partition_point(|t| t.value < value);
                for i in [at.checked_sub(1), (at < targets.len()).then_some(at)]
                    .into_iter()
                    .flatten()
                {
                    let target = targets[i];
                    let correction = target.value - value;
                    if correction.abs() <= 6. / self.view.zoom
                        && closest.is_none_or(|(old, _)| correction.abs() < old.abs())
                    {
                        closest = Some((correction, target));
                    }
                }
            }
            let moved = Rect {
                x: r.x + delta.x,
                y: r.y + delta.y,
                ..r
            };
            let gap = spacing::nearest(
                &self.snapping.neighbors[axis],
                moved,
                axis,
                6. / self.view.zoom,
            );
            if let Some(gap) = gap.filter(|g| {
                closest.is_none_or(|(correction, _)| g.correction.abs() <= correction.abs())
            }) {
                if axis == 0 {
                    snapped.x += gap.correction;
                } else {
                    snapped.y += gap.correction;
                }
            } else if let Some((correction, target)) = closest {
                if axis == 0 {
                    snapped.x += correction;
                } else {
                    snapped.y += correction;
                }
                self.snapping.guides.push(Guide {
                    axis,
                    value: target.value,
                    low: target.low,
                    high: target.high,
                });
            }
        }
        let moved = Rect {
            x: r.x + snapped.x,
            y: r.y + snapped.y,
            ..r
        };
        for axis in 0..2 {
            if let Some(gap) = spacing::nearest(
                &self.snapping.neighbors[axis],
                moved,
                axis,
                0.01 / self.view.zoom,
            ) {
                self.snapping.gaps.extend(gap.dimensions);
            }
        }
        for guide in &mut self.snapping.guides {
            let (low, high) = if guide.axis == 0 {
                (r.y + snapped.y, r.y + snapped.y + r.height)
            } else {
                (r.x + snapped.x, r.x + snapped.x + r.width)
            };
            guide.low = guide.low.min(low);
            guide.high = guide.high.max(high);
        }
        snapped
    }
    pub(in crate::workspace) fn snap_guides(&self) -> Div {
        let mut el = div().absolute().inset_0();
        for g in &self.snapping.guides {
            let start = self.view.screen(if g.axis == 0 {
                point(g.value, g.low)
            } else {
                point(g.low, g.value)
            });
            let len = (g.high - g.low) * self.view.zoom;
            el = el.child(
                div()
                    .absolute()
                    .left(px(start.x))
                    .top(px(start.y))
                    .w(px(if g.axis == 0 { 1. } else { len }))
                    .h(px(if g.axis == 0 { len } else { 1. }))
                    .bg(gpui::rgba(0xf28bd9cc)),
            );
        }
        el.child(dimension_overlay(
            self.view,
            self.snapping.gaps.clone(),
            None,
        ))
    }
}

#[cfg(test)]
mod tests;
