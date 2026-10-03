//! Property value conversion, input synchronization, and edit transactions.

use super::properties::{apply_field, apply_shape_field, apply_text_field};
use super::*;

impl Workspace {
    pub(in crate::editor) fn field_value(&self, index: usize, cx: &gpui::App) -> Option<String> {
        use Property::*;
        let property = self.field_property(index)?;
        if property == Rotation {
            return self
                .selected_text
                .or(self.selected_shape)
                .filter(|_| self.multi_selection.is_empty())
                .map(|id| number(self.object_rotation(id)));
        }
        if !self.multi_selection.is_empty() {
            return self.multi_field_value(property, cx);
        }
        if let Some(text) = self.selected_text() {
            let editor = text.editor.read(cx);
            let style = editor.effective_style();
            if property.text_style().is_some_and(|p| editor.mixed(p)) {
                return Some(String::new());
            }
            if matches!(property, Color | Opacity | GradientAngle | GradientPosition)
                && (editor.mixed(TextProperty::FillMode)
                    || (style.fill_mode == FillMode::Linear
                        && editor.mixed(TextProperty::Gradient)))
            {
                return Some(String::new());
            }
            return Some(match property {
                Name => return None,
                X => number(text.rect.x),
                Y => number(text.rect.y),
                Width => number(text.rect.width),
                Height => number(text.rect.height),
                Color => hex(style.editable_color(self.inspector.active_stop)),
                Opacity => number(style.editable_color(self.inspector.active_stop).a * 100.),
                FontSize => number(style.size),
                LineHeight => number(style.line_height),
                LetterSpacing => number(style.spacing),
                FontWeight => number(style.weight),
                GradientAngle => number(style.gradient.angle),
                GradientPosition => number(
                    style
                        .gradient
                        .stop(self.inspector.active_stop)
                        .unwrap_or(&style.gradient.stops()[0])
                        .position
                        * 100.,
                ),
                _ => return None,
            });
        }
        if let Some(shape) = self.selected_shape() {
            let stroke = self.field_edits_stroke(index);
            let stop = self.shape_paint_stop(stroke);
            let color = shape.paint_color(stop, stroke);
            return Some(match property {
                Name => shape.name.clone(),
                StartX if shape.kind.is_line() => number(shape.display_path_point(0).x),
                StartY if shape.kind.is_line() => number(shape.display_path_point(0).y),
                EndX if shape.kind.is_line() => number(shape.display_path_point(1).x),
                EndY if shape.kind.is_line() => number(shape.display_path_point(1).y),
                X => number(shape.rect.x),
                Y => number(shape.rect.y),
                Width => number(shape.rect.width),
                Height => number(shape.rect.height),
                Color if !stroke && shape.fill_mode == FillMode::Image => t("shape-image").into(),
                Color => hex(color),
                Opacity if !stroke && shape.fill_mode == FillMode::Image => {
                    number(shape.image_fill.opacity * 100.)
                }
                Opacity => number(color.a * 100.),
                GradientAngle => number(shape.paint_gradient(stroke).angle),
                GradientPosition => {
                    number(shape.paint_gradient(stroke).stop(stop)?.position * 100.)
                }
                Radius if shape.kind.supports_corners() => number(shape.radius),
                Vertices if shape.kind.is_polygon() => shape.vertices.to_string(),
                InnerRadius if shape.kind == ShapeKind::Star => number(shape.inner_radius * 100.),
                Corner(corner) if shape.kind.supports_corners() => {
                    number(shape.corners.unwrap_or([shape.radius; 4])[corner.index()])
                }
                StrokeWidth => number(shape.stroke.width),
                _ => return None,
            });
        }
        let board = self.selected_board()?;
        let color = board.editable_color(self.inspector.active_stop);
        Some(match property {
            Name => board.name.clone(),
            X => number(board.rect.x),
            Y => number(board.rect.y),
            Width => number(board.rect.width),
            Height => number(board.rect.height),
            Color if board.fill_mode == FillMode::Image => t("shape-image").into(),
            Color => format!(
                "{:02X}{:02X}{:02X}",
                (color.r * 255.).round() as u8,
                (color.g * 255.).round() as u8,
                (color.b * 255.).round() as u8
            ),
            Opacity if board.fill_mode == FillMode::Image => {
                number(board.image_fill.opacity * 100.)
            }
            Opacity => number(color.a * 100.),
            GradientAngle => number(board.gradient.angle),
            GradientPosition => {
                number(board.gradient.stop(self.inspector.active_stop)?.position * 100.)
            }
            _ => return None,
        })
    }

    pub(in crate::editor) fn sync_picker(&mut self, color: gpui::Rgba, cx: &mut Context<Self>) {
        for picker in [&self.inspector.picker, &self.inspector.alpha_picker] {
            let old = picker.read(cx).value();
            if [
                old.r - color.r,
                old.g - color.g,
                old.b - color.b,
                old.a - color.a,
            ]
            .into_iter()
            .any(|delta| delta.abs() > 0.000001)
            {
                picker.update(cx, |picker, cx| picker.set_value(color, cx));
            }
        }
    }

    pub(in crate::editor) fn sync_field(&mut self, index: usize, cx: &mut Context<Self>) {
        for surface in 0..PROPERTY_SURFACES {
            self.sync_input_field(index + surface * PROPERTY_COUNT, cx);
        }
    }

    pub(in crate::editor) fn sync_input_field(&mut self, slot: usize, cx: &mut Context<Self>) {
        if slot >= PROPERTY_COUNT && slot / PROPERTY_COUNT != self.paint_surface(cx) {
            return;
        }
        let index = slot % PROPERTY_COUNT;
        if self.inspector.invalid[slot] {
            self.inspector.invalid[slot] = false;
            cx.notify();
        }
        if let Some(value) = self.field_value(index, cx) {
            self.inspector.fields[slot].update(cx, |input, _| {
                input.set_placeholder(if value.is_empty() { t("mixed") } else { "" });
            });
            let value: gpui::SharedString = value.into();
            if self.inspector.fields[slot].read(cx).value() != value {
                // set_value emits Change. Consume exactly that programmatic
                // event, including when selection changes before dispatch.
                self.inspector.pending.push((slot, value.clone()));
                self.inspector.fields[slot].update(cx, |input, cx| {
                    input.set_value(value, cx);
                });
            }
        }
    }

    pub(in crate::editor) fn sync_fields(&mut self, cx: &mut Context<Self>) {
        if let Some((_, gradient)) = self.fill_state(cx)
            && gradient.stop(self.inspector.active_stop).is_none()
        {
            self.inspector.active_stop = gradient.stops()[0].id;
        }
        for index in 0..PROPERTY_COUNT {
            self.sync_field(index, cx);
        }
        if !self.multi_selection.is_empty() {
            if let Some(color) = self.multi_picker_color(cx) {
                self.sync_picker(color, cx);
            }
            return;
        }
        if let Some(text) = self.selected_text() {
            let editor = text.editor.read(cx);
            let style = editor.effective_style();
            let color = style.editable_color(self.inspector.active_stop);
            let family = if editor.mixed(TextProperty::Family) {
                t("font-mixed").into()
            } else {
                style.family.clone()
            };
            self.inspector
                .font_picker
                .update(cx, |picker, cx| picker.set_selected(family, cx));
            self.sync_picker(color, cx);
        } else if let Some(shape) = self.selected_shape() {
            let color =
                shape.paint_color(self.inspector.active_stop, self.inspector.stroke_editing);
            self.sync_picker(color, cx);
        } else if let Some(board) = self.selected_board() {
            let color = board.editable_color(self.inspector.active_stop);
            self.sync_picker(color, cx);
        }
    }
    pub(in crate::editor) fn sync_text_inspector(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        for index in 0..self.inspector.fields.len() {
            if !self.inspector.fields[index]
                .focus_handle(cx)
                .is_focused(window)
            {
                self.sync_input_field(index, cx);
            }
        }
        if let Some(text) = self.selected_text() {
            let editor = text.editor.read(cx);
            let style = editor.effective_style();
            let color = style.editable_color(self.inspector.active_stop);
            let family = if editor.mixed(TextProperty::Family) {
                t("font-mixed").into()
            } else {
                style.family.clone()
            };
            if style.gradient.stop(self.inspector.active_stop).is_none() {
                self.inspector.active_stop = style.gradient.stops()[0].id;
            }
            self.inspector
                .font_picker
                .update(cx, |picker, cx| picker.set_selected(family, cx));
            self.sync_picker(color, cx);
        }
    }

    pub(in crate::editor) fn edit_field(
        &mut self,
        slot: usize,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        let index = slot % PROPERTY_COUNT;
        let Some(property) = self.field_property(index) else {
            return;
        };
        let value = event.text();
        if matches!(event, InputEvent::Change(_)) {
            if let Some(pos) = self
                .inspector
                .pending
                .iter()
                .position(|(field, text)| *field == slot && text == value)
            {
                self.inspector.pending.remove(pos);
                return;
            }
            if self.inspector.fields[slot].read(cx).value() != *value {
                return;
            }
        }
        if slot >= PROPERTY_COUNT && slot / PROPERTY_COUNT != self.paint_surface(cx) {
            return;
        }
        if !self.multi_selection.is_empty() {
            self.inspector.invalid[slot] = !self.edit_multi_field(property, value, cx);
            if !self.inspector.invalid[slot] && matches!(index, 3 | 4) {
                self.sync_field(7 - index, cx);
            }
            if !self.inspector.invalid[slot]
                && matches!(index, 5 | 6)
                && let Some(color) = self.multi_picker_color(cx)
            {
                self.sync_picker(color, cx);
            }
            if matches!(event, InputEvent::Submit(_)) {
                self.history.borrow_mut().break_group();
                self.sync_field(index, cx);
            }
            self.sync_input_peers(slot, cx);
            return;
        }
        let Some(id) = self.selected_text.or(self.selected_shape).or(self.selected) else {
            return;
        };
        if self.selected_shape.is_some() && matches!(index, 5 | 6 | 14 | 16 | 17) {
            let stroke = self.field_edits_stroke(index);
            if stroke != self.inspector.stroke_editing {
                self.history.borrow_mut().break_group();
                self.set_paint_target(stroke);
            }
        }
        let stop = self.inspector.active_stop;
        self.history.borrow_mut().set_scope(
            Group::Property {
                id,
                property,
                stop,
                stroke: self.selected_shape.is_some() && self.field_edits_stroke(index),
            },
            false,
        );
        if property == Property::Rotation {
            self.inspector.invalid[slot] = !self.edit_rotation(value);
            self.history.borrow_mut().clear_scope();
            if matches!(event, InputEvent::Submit(_)) {
                self.history.borrow_mut().break_group();
                self.sync_field(index, cx);
            }
            cx.notify();
            return;
        }
        let (valid, color) = if self.selected_text.is_some() {
            let text = self.selected_text_mut().unwrap();
            let before = text.rect;
            let board = text.board;
            let valid = apply_text_field(text, property, value, stop, cx);
            let color = text.editor.read(cx).effective_style().editable_color(stop);
            if text.rect != before {
                let after = text.rect;
                let mut changes: Vec<_> = self
                    .fix_layout_size(id, before, after)
                    .into_iter()
                    .collect();
                changes.push(Change::TextRect {
                    id,
                    board,
                    value: before,
                });
                self.history.borrow_mut().record(changes, None);
            }
            (valid, color)
        } else if self.selected_shape.is_some() {
            let stroke = self.field_edits_stroke(index);
            let valid = self
                .edit_shape(|shape| apply_shape_field(shape, stroke, stop, property, value))
                .unwrap_or(false);
            (
                valid,
                self.selected_shape().unwrap().paint_color(stop, stroke),
            )
        } else {
            let valid = self
                .edit_board(None, |board| apply_field(board, stop, property, value))
                .unwrap_or(false);
            (valid, self.selected_board().unwrap().editable_color(stop))
        };
        self.history.borrow_mut().clear_scope();
        self.inspector.invalid[slot] = !valid;
        if valid && matches!(index, 3 | 4) {
            self.sync_field(7 - index, cx);
        }
        if valid && matches!(index, 5 | 6 | 16 | 17) {
            self.sync_picker(color, cx);
        }
        if matches!(event, InputEvent::Submit(_)) {
            self.history.borrow_mut().break_group();
            self.sync_field(index, cx);
        }
        self.sync_input_peers(slot, cx);
        cx.notify();
    }

    fn sync_input_peers(&mut self, slot: usize, cx: &mut Context<Self>) {
        if self.inspector.invalid[slot] {
            return;
        }
        for surface in 0..PROPERTY_SURFACES {
            let peer = slot % PROPERTY_COUNT + surface * PROPERTY_COUNT;
            if peer != slot {
                self.sync_input_field(peer, cx);
            }
        }
    }

    pub(super) fn paint_surface(&self, cx: &gpui::App) -> usize {
        match self.fill_state(cx).map(|(mode, _)| mode) {
            Some(FillMode::Linear) => 2,
            Some(FillMode::Image) => 3,
            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_properties_preserve_geometry_and_hex_preserves_alpha() {
        let mut board = Artboard {
            uid: uuid::Uuid::new_v4(),
            color_style: None,
            id: 1,
            layer: Default::default(),
            name: "A".into(),
            image_fill: Default::default(),
            rect: Rect {
                x: 10.,
                y: 20.,
                width: 640.,
                height: 480.,
            },
            color: rgb(0xffffff),
            fill_mode: FillMode::Solid,
            gradient: Default::default(),
        };
        let original = board.rect;
        for value in ["", "-", "0", "-20", "NaN", "inf", "100001"] {
            assert!(!apply_field(&mut board, 0, Property::Width, value));
            assert_eq!(board.rect, original);
        }
        assert!(apply_field(&mut board, 0, Property::Opacity, "25"));
        assert!(apply_field(&mut board, 0, Property::Color, "#345678"));
        assert_eq!(board.color.a, 0.25);
        let color = board.color;
        assert!(!apply_field(&mut board, 0, Property::Color, "GGGGGG"));
        assert_eq!(board.color, color);
    }
}
