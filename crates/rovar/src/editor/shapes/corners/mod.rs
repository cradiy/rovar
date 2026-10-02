use super::*;

#[cfg(test)]
mod tests;
mod view;

// The diagonal point on a quarter-circle, measured from its bounding corner.
const ARC_OFFSET: f32 = 1. - std::f32::consts::FRAC_1_SQRT_2;
const SIGNS: [(f32, f32); 4] = [(1., 1.), (-1., 1.), (-1., -1.), (1., -1.)];

#[derive(Clone, Copy, PartialEq, Eq)]
struct Target {
    id: usize,
    corner: usize,
}

struct Edit {
    target: Target,
    original: Shape,
    single: bool,
}

pub(in crate::editor) struct State {
    hover: Option<Target>,
    drag: Option<Edit>,
    drag_moved: bool,
    edit: Option<Edit>,
    input: Entity<TextInput>,
    invalid: bool,
    _subscriptions: Vec<Subscription>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let input = cx.new(TextInput::new);
        let submit = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Submit(_)) && this.finish_corner_input(true, cx) {
                this.focus.focus(window, cx);
            }
        });
        let blur = cx.on_blur(&input.focus_handle(cx), window, |this, _, cx| {
            if !this.finish_corner_input(true, cx) {
                this.finish_corner_input(false, cx);
            }
        });
        Self {
            input,
            hover: None,
            drag: None,
            drag_moved: false,
            edit: None,
            invalid: false,
            _subscriptions: vec![submit, blur],
        }
    }

    pub fn editing(&self) -> bool {
        self.edit.is_some()
    }
    pub fn clear_hover(&mut self) -> bool {
        self.hover.take().is_some()
    }

    fn active(&self) -> Option<Target> {
        self.drag
            .as_ref()
            .or(self.edit.as_ref())
            .map(|e| e.target)
            .or(self.hover)
    }
}

fn radius_limit(shape: &Shape) -> f32 {
    shape.rect.width.min(shape.rect.height) / 2.
}

fn apply(shape: &mut Shape, original: &Shape, corner: usize, single: bool, radius: f32) {
    if single {
        let mut corners = if original.independent_corners {
            original.corners.unwrap_or([original.radius; 4])
        } else {
            [original.radius; 4]
        };
        corners[corner] = radius;
        shape.independent_corners = true;
        shape.corners = Some(corners);
    } else {
        shape.independent_corners = original.independent_corners;
        shape.corners = original.corners;
        shape.radius = radius;
    }
}

impl Workspace {
    fn corner_shape(&self, cx: &gpui::App) -> Option<&Shape> {
        if !self.multi_selection.is_empty()
            || self.vector_edit.is_some()
            || self.space_down
            || self.toolbar.hand
            || self.draw_tool.is_some()
            || self.preview_read_only()
            || self.image_crop.is_some()
            || self.colors.dialog.is_some()
            || self.assets.dialog.is_some()
            || uic::components::context_menu::is_open(cx)
            || self
                .gesture
                .is_some_and(|g| !matches!(g.kind, GestureKind::CornerRadius))
        {
            return None;
        }
        self.selected_shape().filter(|shape| {
            shape.kind == ShapeKind::Rectangle
                && self.layer_editable(shape.id)
                && shape.rect.width.min(shape.rect.height) * self.view.zoom >= 80.
        })
    }

    fn corner_point(&self, shape: &Shape, corner: usize) -> Point<f32> {
        let origin = self.parent_origin(shape.board);
        let (sx, sy) = SIGNS[corner];
        let inset = 16. / self.view.zoom + shape.displayed_radii()[corner] * ARC_OFFSET;
        let point = point(
            origin.x
                + shape.rect.x
                + if sx > 0. {
                    inset
                } else {
                    shape.rect.width - inset
                },
            origin.y
                + shape.rect.y
                + if sy > 0. {
                    inset
                } else {
                    shape.rect.height - inset
                },
        );
        let center = origin + crate::scene::rotation::center(shape.rect);
        self.view.screen(crate::scene::rotation::around(
            point,
            center,
            shape.layer.rotation,
        ))
    }

    pub(in crate::editor) fn update_corner_hover(
        &mut self,
        position: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.corner_editor.drag.is_some() || self.corner_editor.editing() {
            return;
        }
        let local = (position - self.bounds.get().origin).map(f32::from);
        let (left, right) = self.canvas_insets();
        let visible = self.focus.is_focused(window)
            && self.bounds.get().contains(&position)
            && local.x >= left
            && local.x <= f32::from(self.bounds.get().size.width) - right
            && local.y >= 56.
            && local.y < f32::from(self.bounds.get().size.height) - 76.;
        let target = self.corner_shape(cx).filter(|_| visible).and_then(|shape| {
            if let Some(target) = self.corner_editor.hover.filter(|t| t.id == shape.id) {
                let p = self.corner_point(shape, target.corner);
                if (local.x - p.x).hypot(local.y - p.y) <= 34. {
                    return Some(target);
                }
            }
            (0..4)
                .filter_map(|corner| {
                    let p = self.corner_point(shape, corner);
                    let distance = (local.x - p.x).hypot(local.y - p.y);
                    (distance <= 30.).then_some((corner, distance))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(corner, _)| Target {
                    id: shape.id,
                    corner,
                })
        });
        if self.corner_editor.hover != target {
            self.corner_editor.hover = target;
            cx.notify();
        }
    }

    fn begin_corner_radius(
        &mut self,
        target: Target,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some() {
            return;
        }
        let Some(original) = self.corner_shape(cx).filter(|s| s.id == target.id).cloned() else {
            return;
        };
        if self.selection_pointer(target.id, event, window, cx) {
            return;
        }
        if event.click_count >= 2 {
            self.finish_corner_input(false, cx);
            self.enter_vector_edit(target.id, window, cx);
            cx.stop_propagation();
            return;
        }
        self.finish_corner_input(false, cx);
        self.corner_editor.drag_moved = false;
        let single = original.independent_corners || event.modifiers.alt;
        self.corner_editor.drag = Some(Edit {
            target,
            original,
            single,
        });
        self.begin(
            GestureKind::CornerRadius,
            event.position,
            event.button,
            window,
            cx,
        );
    }

    pub(in crate::editor) fn move_corner_radius(
        &mut self,
        delta: Point<f32>,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.corner_editor.drag.as_ref() else {
            return;
        };
        self.corner_editor.drag_moved |= delta.x.hypot(delta.y) * self.view.zoom >= 3.;
        let Some(shape) = self.shapes.iter_mut().find(|s| s.id == edit.target.id) else {
            return;
        };
        let delta = crate::scene::rotation::vector(delta, -edit.original.layer.rotation);
        let (sx, sy) = SIGNS[edit.target.corner];
        let movement = (delta.x * sx + delta.y * sy) / 2.;
        if movement.abs() * self.view.zoom < 2. {
            *shape = edit.original.clone();
        } else {
            let radius = (edit.original.displayed_radii()[edit.target.corner]
                + movement / ARC_OFFSET)
                .round()
                .clamp(0., radius_limit(&edit.original));
            apply(
                shape,
                &edit.original,
                edit.target.corner,
                edit.single || self.snapping.bypass,
                radius,
            );
        }
        self.sync_fields(cx);
    }

    pub(in crate::editor) fn commit_corner_radius(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let click = self
            .corner_editor
            .drag
            .as_ref()
            .filter(|_| !self.corner_editor.drag_moved)
            .map(|edit| (edit.target, edit.single));
        self.finish_corner_radius(true, cx);
        if let Some((target, single)) = click {
            self.edit_corner_radius(target, single, window, cx);
        }
    }

    pub(in crate::editor) fn finish_corner_radius(&mut self, commit: bool, cx: &mut Context<Self>) {
        if let Some(edit) = self.corner_editor.drag.take()
            && let Some(index) = self.shapes.iter().position(|s| s.id == edit.target.id)
        {
            if !commit {
                self.shapes[index] = edit.original;
            } else if self.shapes[index] != edit.original {
                self.history.borrow_mut().record(
                    vec![Change::Shape {
                        id: edit.target.id,
                        index,
                        value: Some(edit.original),
                    }],
                    None,
                );
            }
            self.sync_fields(cx);
        }
    }

    fn edit_corner_radius(
        &mut self,
        target: Target,
        alt: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some() {
            return;
        }
        let Some(original) = self.corner_shape(cx).filter(|s| s.id == target.id).cloned() else {
            return;
        };
        let value = original.displayed_radii()[target.corner].to_string();
        self.corner_editor
            .input
            .update(cx, |input, cx| input.set_value(value, cx));
        let single = original.independent_corners || alt;
        self.corner_editor.edit = Some(Edit {
            target,
            original,
            single,
        });
        self.corner_editor.invalid = false;
        self.corner_editor.input.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(in crate::editor) fn finish_corner_input(
        &mut self,
        commit: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = self.corner_editor.edit.as_ref() else {
            return true;
        };
        let unchanged = self.corner_editor.input.read(cx).value().as_ref()
            == edit.original.displayed_radii()[edit.target.corner].to_string();
        if !commit
            || unchanged
            || self.selected_shape != Some(edit.target.id)
            || !self.layer_editable(edit.target.id)
        {
            self.corner_editor.edit = None;
            self.corner_editor.invalid = false;
            cx.notify();
            return true;
        }
        let value = self
            .corner_editor
            .input
            .read(cx)
            .value()
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0. && *v <= radius_limit(&edit.original));
        let Some(value) = value else {
            self.corner_editor.invalid = true;
            cx.notify();
            return false;
        };
        let edit = self.corner_editor.edit.take().unwrap();
        self.corner_editor.invalid = false;
        if let Some(index) = self.shapes.iter().position(|s| s.id == edit.target.id) {
            let before = self.shapes[index].clone();
            apply(
                &mut self.shapes[index],
                &edit.original,
                edit.target.corner,
                edit.single,
                value,
            );
            if self.shapes[index] != before {
                self.history.borrow_mut().record(
                    vec![Change::Shape {
                        id: edit.target.id,
                        index,
                        value: Some(before),
                    }],
                    None,
                );
            }
        }
        self.sync_fields(cx);
        cx.notify();
        true
    }
}
