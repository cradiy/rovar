use super::*;
use crate::artboard::{MAX_SIZE, MIN_SIZE};

fn handle_point(rect: Rect, handle: Handle) -> Point<f32> {
    point(
        rect.x + (handle.0 as f32 + 1.) * rect.width / 2.,
        rect.y + (handle.1 as f32 + 1.) * rect.height / 2.,
    )
}

fn resize(
    original: Rect,
    handle: Handle,
    delta: Point<f32>,
    proportional: bool,
    limits: (Point<f32>, Point<f32>),
) -> Rect {
    let (min, max) = limits;
    if !proportional || original.width <= 0. || original.height <= 0. {
        return handle.resize(original, delta, limits);
    }
    let sx = 1. + delta.x * handle.0 as f32 / original.width;
    let sy = 1. + delta.y * handle.1 as f32 / original.height;
    let scale = if handle.1 == 0 || (handle.0 != 0 && (sx - 1.).abs() > (sy - 1.).abs()) {
        sx
    } else {
        sy
    };
    let min_scale = (min.x / original.width).max(min.y / original.height);
    let max_scale = (max.x / original.width).min(max.y / original.height);
    let scale = scale.clamp(min_scale, max_scale);
    let width = original.width * scale;
    let height = original.height * scale;
    Rect {
        x: if handle.0 < 0 {
            original.x + original.width - width
        } else if handle.0 == 0 {
            original.x + (original.width - width) / 2.
        } else {
            original.x
        },
        y: if handle.1 < 0 {
            original.y + original.height - height
        } else if handle.1 == 0 {
            original.y + (original.height - height) / 2.
        } else {
            original.y
        },
        width,
        height,
    }
}

impl Workspace {
    pub(in crate::workspace) fn snap_box_rect(
        &mut self,
        start: Point<f32>,
        mut end: Point<f32>,
        square: bool,
    ) -> Rect {
        if end == start {
            self.snapping.guides.clear();
            return creation::drag_rect(start, end, false);
        }
        let sign = point(
            if end.x < start.x { -1. } else { 1. },
            if end.y < start.y { -1. } else { 1. },
        );
        if square {
            let side = (end.x - start.x).abs().max((end.y - start.y).abs());
            end = start + sign * side;
        }
        end = self.snap_point(end, [true, true], square.then_some(sign));
        let rect = creation::drag_rect(start, end, false);
        // Size clamping can reject a reference beyond the supported dimensions.
        let endpoint = point(
            if end.x < start.x {
                rect.x
            } else {
                rect.x + rect.width
            },
            if end.y < start.y {
                rect.y
            } else {
                rect.y + rect.height
            },
        );
        self.point_guides(endpoint, [true, true]);
        self.extend_snap_guides(rect);
        rect
    }

    pub(in crate::workspace) fn resize_with_snapping(
        &mut self,
        id: usize,
        original: Rect,
        handle: Handle,
        delta: Point<f32>,
        shift: bool,
    ) -> Rect {
        let parent = self.object_rect(id).and_then(|(p, _)| p);
        let origin = self.parent_origin(parent);
        let world = Rect {
            x: original.x + origin.x,
            y: original.y + origin.y,
            ..original
        };
        let proportional = (shift && handle.0 != 0 && handle.1 != 0)
            || self.layer_info(id).is_some_and(|(s, _)| s.aspect_locked);
        let angle = self.object_rotation(id);
        let limits = (point(MIN_SIZE, MIN_SIZE), point(MAX_SIZE, MAX_SIZE));
        if angle != 0. {
            use crate::rotation::{around, bounds, center, vector};
            let pivot = center(world);
            let start = around(handle_point(world, handle), pivot, angle);
            let build = |delta| {
                let mut r = resize(world, handle, vector(delta, -angle), proportional, limits);
                let c = pivot + vector(center(r) - pivot, angle);
                r.x = c.x - r.width / 2.;
                r.y = c.y - r.height / 2.;
                r
            };
            let mut rect = build(delta);
            if delta.x.hypot(delta.y) * self.view.zoom >= 3. {
                let end = around(handle_point(rect, handle), center(rect), angle);
                let direction = if proportional {
                    Some(vector(
                        point(
                            handle.0 as f32 * world.width,
                            handle.1 as f32 * world.height,
                        ),
                        angle,
                    ))
                } else if handle.0 == 0 || handle.1 == 0 {
                    Some(vector(point(handle.0 as f32, handle.1 as f32), angle))
                } else {
                    None
                };
                let snapped = self.snap_point(end, [true, true], direction);
                rect = build(snapped - start);
                self.point_guides(
                    around(handle_point(rect, handle), center(rect), angle),
                    [true, true],
                );
                self.extend_snap_guides(bounds(rect, angle));
            }
            return Rect {
                x: rect.x - origin.x,
                y: rect.y - origin.y,
                ..rect
            };
        }
        let rect = self.resize_bounds_with_snapping(world, handle, delta, proportional, limits);
        Rect {
            x: rect.x - origin.x,
            y: rect.y - origin.y,
            ..rect
        }
    }

    pub(in crate::workspace) fn resize_bounds_with_snapping(
        &mut self,
        world: Rect,
        handle: Handle,
        delta: Point<f32>,
        proportional: bool,
        limits: (Point<f32>, Point<f32>),
    ) -> Rect {
        let mut rect = resize(world, handle, delta, proportional, limits);
        let axes = [handle.0 != 0, handle.1 != 0];
        // Clicking a handle must never change its geometry.
        if (delta.x.abs() + delta.y.abs()) * self.view.zoom >= 3. {
            let direction = proportional.then(|| {
                point(
                    handle.0 as f32 * world.width,
                    handle.1 as f32 * world.height,
                )
            });
            let corner = self.snap_point(handle_point(rect, handle), axes, direction);
            rect = resize(
                world,
                handle,
                corner - handle_point(world, handle),
                proportional,
                limits,
            );
            // Clamping at the opposite edge or size limits can reject a target.
            self.point_guides(handle_point(rect, handle), axes);
            self.extend_snap_guides(rect);
        }
        rect
    }
}
