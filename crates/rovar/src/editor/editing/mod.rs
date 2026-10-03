use super::*;
use crate::i18n::t;
use crate::scene::artboard::{MAX_SIZE, MIN_SIZE};
use std::sync::Arc;

pub(super) fn set_dimension(rect: &mut Rect, property: Property, value: f32, locked: bool) -> bool {
    if !matches!(property, Property::Width | Property::Height)
        || !value.is_finite()
        || !(MIN_SIZE..=MAX_SIZE).contains(&value)
    {
        return false;
    }
    if locked && rect.width > 0. && rect.height > 0. {
        let scale = (value
            / if property == Property::Width {
                rect.width
            } else {
                rect.height
            })
        .clamp(
            (MIN_SIZE / rect.width).max(MIN_SIZE / rect.height),
            (MAX_SIZE / rect.width).min(MAX_SIZE / rect.height),
        );
        rect.width *= scale;
        rect.height *= scale;
    } else if property == Property::Width {
        rect.width = value;
    } else {
        rect.height = value;
    }
    true
}
impl Workspace {
    pub(super) fn can_flip(&self) -> bool {
        let ids = self.operation_ids();
        !ids.is_empty()
            && ids.iter().all(|id| {
                self.shapes
                    .iter()
                    .any(|s| s.id == *id && !s.kind.is_media())
            })
    }
    pub(super) fn flip_selection(&mut self, horizontal: bool, cx: &mut Context<Self>) {
        if !self.can_flip() {
            return;
        }
        self.seal_text_edits(cx);
        let ids = self.operation_ids();
        let Some(bounds) = ids
            .iter()
            .filter_map(|id| self.world_bounds(*id))
            .reduce(layout::union)
        else {
            return;
        };
        let before = self.before_geometry();
        let positions: Vec<_> = ids
            .iter()
            .filter_map(|id| self.world_rect(*id).map(|r| (*id, r)))
            .collect();
        for (id, mut rect) in positions {
            let board = self.shapes.iter().find(|s| s.id == id).unwrap().board;
            let origin = self.parent_origin(board);
            if horizontal {
                rect.x = bounds.x * 2. + bounds.width - rect.x - rect.width;
            } else {
                rect.y = bounds.y * 2. + bounds.height - rect.y - rect.height;
            }
            rect.x -= origin.x;
            rect.y -= origin.y;
            let shape = self.shapes.iter_mut().find(|s| s.id == id).unwrap();
            shape.rect = rect;
            shape.layer.rotation = crate::scene::rotation::normalize(-shape.layer.rotation);
            if shape.kind.is_polygon() {
                shape.mirrored[usize::from(!horizontal)] ^= true;
            }
            let mirror = |p: Point<f32>| {
                if horizontal {
                    point(1. - p.x, p.y)
                } else {
                    point(p.x, 1. - p.y)
                }
            };
            shape.points.0 = Arc::new(shape.points.0.iter().copied().map(mirror).collect());
            shape.nodes.0 = Arc::new(shape.nodes.0.iter().map(|n| n.map(mirror)).collect());
            if let Some(c) = shape.corners {
                shape.corners = Some(if horizontal {
                    [c[1], c[0], c[3], c[2]]
                } else {
                    [c[3], c[2], c[1], c[0]]
                });
            }
            for gradient in [&mut shape.gradient, &mut shape.stroke.gradient] {
                gradient.angle = (if horizontal {
                    -gradient.angle
                } else {
                    180. - gradient.angle
                })
                .rem_euclid(360.);
            }
        }
        if self.batch_changed(&before, cx) {
            self.history.borrow_mut().record(before, None);
        }
        self.sync_fields(cx);
        cx.notify();
    }
    fn aspect_target(&self) -> Option<usize> {
        let ids = self.selection_ids();
        if ids.len() != 1 {
            return None;
        }
        let id = *ids.first()?;
        let (_, r) = self.object_rect(id)?;
        (r.width > 0.
            && r.height > 0.
            && !self.shapes.iter().any(|s| s.id == id && s.kind.is_line()))
        .then_some(id)
    }
    pub(super) fn aspect_button(&self, cx: &mut Context<Self>) -> Div {
        let Some(id) = self.aspect_target() else {
            return div();
        };
        let locked = self.layer_info(id).unwrap().0.aspect_locked;
        div().flex().items_center().child(
            div()
                .id("aspect-lock")
                .debug_selector(|| "aspect-lock".into())
                .w(px(26.))
                .h(px(30.))
                .flex_shrink_0()
                .rounded(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .when(locked, |el| el.bg(gpui::rgba(0xb4a2ee28)))
                .hover(|s| s.bg(gpui::rgba(0xb4a2ee33)))
                .tooltip(move |_, cx| {
                    cx.new(|_| {
                        toolbar::ToolTip(
                            if locked {
                                t("unlock-aspect")
                            } else {
                                t("lock-aspect")
                            }
                            .into(),
                        )
                    })
                    .into()
                })
                .child(icon(
                    if locked {
                        LucideIcons::Link
                    } else {
                        LucideIcons::Unlink
                    },
                    15.,
                ))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !this.layer_editable(id) {
                        return;
                    }
                    this.seal_text_edits(cx);
                    if let Some(layer) = this.layer_state_mut(id) {
                        let before = *layer;
                        layer.aspect_locked = !layer.aspect_locked;
                        this.history
                            .borrow_mut()
                            .record(vec![Change::Layer { id, value: before }], None);
                    }
                    this.focus.focus(window, cx);
                    cx.notify();
                })),
        )
    }
}
#[cfg(test)]
mod tests;
