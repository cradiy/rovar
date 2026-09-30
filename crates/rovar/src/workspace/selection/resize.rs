use super::*;
use crate::artboard::{MAX_SIZE, MIN_SIZE};

#[cfg(test)]
mod tests;
mod view;

struct Item {
    id: usize,
    parent: Option<usize>,
    local: Rect,
    world: Rect,
}

pub(in crate::workspace) struct Resize {
    items: Vec<Item>,
    before: Vec<Change>,
    proportional: bool,
}

impl Workspace {
    fn selection_resize_bounds(&self) -> Option<Rect> {
        if self.multi_selection.is_empty()
            || self
                .operation_ids()
                .iter()
                .any(|id| !self.layer_editable(*id) || self.is_layout_flow_item(*id))
        {
            return None;
        }
        self.selection_ids()
            .into_iter()
            .filter_map(|id| self.world_bounds(id))
            .reduce(layout::union)
            .filter(|r| r.width > 0. && r.height > 0.)
    }

    fn begin_selection_resize(
        &mut self,
        handle: Handle,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some() {
            return;
        }
        let Some(original) = self.selection_resize_bounds() else {
            return;
        };
        self.finish_spacing_input(false, cx);
        self.seal_text_edits(cx);
        let items: Vec<_> = self
            .operation_ids()
            .into_iter()
            .filter_map(|id| {
                let (parent, local) = self.object_rect(id)?;
                Some(Item {
                    id,
                    parent,
                    local,
                    world: self.world_rect(id)?,
                })
            })
            .collect();
        if items.is_empty() {
            return;
        }
        // Non-uniform world scaling of a rotated rectangle would require shear.
        let proportional = items.iter().any(|item| {
            self.object_rotation(item.id) != 0.
                || self
                    .layer_info(item.id)
                    .is_some_and(|(layer, _)| layer.aspect_locked)
        });
        // Container resizing can reflow children; undo must restore them too.
        let before = self.geometry_changes(self.descendants(&self.selection_ids()));
        self.selection_resize = Some(Resize {
            items,
            before,
            proportional,
        });
        self.begin(
            GestureKind::SelectionResize { original, handle },
            event.position,
            event.button,
            window,
            cx,
        );
    }

    pub(in crate::workspace) fn resize_selection(
        &mut self,
        original: Rect,
        handle: Handle,
        delta: Point<f32>,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.selection_resize.take() else {
            return;
        };
        // Always start from the captured geometry, including translated group children.
        self.restore_batch(&state.before, cx);
        if delta.x == 0. && delta.y == 0. {
            self.snapping.clear_guides();
        } else {
            let proportional = shift || state.proportional;
            let (min, max) = scale_limits(&state.items);
            let limits = (
                point(
                    (original.width * min.x).max(MIN_SIZE.min(original.width)),
                    (original.height * min.y).max(MIN_SIZE.min(original.height)),
                ),
                point(original.width * max.x, original.height * max.y),
            );
            let rect =
                self.resize_bounds_with_snapping(original, handle, delta, proportional, limits);
            let scale = point(rect.width / original.width, rect.height / original.height);
            for item in &state.items {
                let world = Rect {
                    x: rect.x + (item.world.x - original.x) * scale.x,
                    y: rect.y + (item.world.y - original.y) * scale.y,
                    width: item.world.width * scale.x,
                    height: item.world.height * scale.y,
                };
                let origin = self.parent_origin(item.parent);
                self.set_object_rect(
                    item.id,
                    item.parent,
                    Rect {
                        x: world.x - origin.x,
                        y: world.y - origin.y,
                        ..world
                    },
                );
            }
        }
        self.selection_resize = Some(state);
        self.sync_fields(cx);
    }

    pub(in crate::workspace) fn finish_selection_resize(
        &mut self,
        commit: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.selection_resize.take() else {
            return;
        };
        if !commit {
            self.restore_batch(&state.before, cx);
        } else if self.batch_changed(&state.before, cx) {
            for item in &state.items {
                if let Some((_, after)) = self.object_rect(item.id) {
                    self.fix_layout_size(item.id, item.local, after);
                }
            }
            self.duplicate = None;
            self.history.borrow_mut().record(state.before, None);
        }
        self.sync_fields(cx);
    }
}

fn scale_limits(items: &[Item]) -> (Point<f32>, Point<f32>) {
    let mut min = point(0., 0.);
    let mut max = point(f32::INFINITY, f32::INFINITY);
    for item in items {
        // Horizontal/vertical paths can legitimately have a zero-size axis.
        if item.local.width > 0. {
            min.x = f32::max(min.x, MIN_SIZE.min(item.local.width) / item.local.width);
            max.x = max.x.min(MAX_SIZE.max(item.local.width) / item.local.width);
        }
        if item.local.height > 0. {
            min.y = f32::max(min.y, MIN_SIZE.min(item.local.height) / item.local.height);
            max.y = max
                .y
                .min(MAX_SIZE.max(item.local.height) / item.local.height);
        }
    }
    (min, max)
}
