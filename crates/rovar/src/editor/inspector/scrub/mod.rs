use super::properties::{apply_field, apply_shape_field, apply_text_field};
use super::*;

impl Workspace {
    pub(in crate::editor) fn numeric_limits(property: Property) -> (f32, f32, f32, f32) {
        use Property::*;
        // Minimum, maximum, units per screen pixel, quantization.
        match property {
            X | Y | StartX | StartY | EndX | EndY | Rotation => (-1_000_000., 1_000_000., 1., 1.),
            Width | Height => (MIN_SIZE, MAX_SIZE, 1., 1.),
            Opacity | LayerOpacity | GradientPosition | PointX | PointY => (0., 100., 1., 1.),
            PointRadius => (1., 400., 1., 1.),
            GradientAngle => (0., 360., 1., 1.),
            FontSize => (1., 1000., 1., 1.),
            LineHeight => (0.5, 5., 0.01, 0.01),
            LetterSpacing => (0., 200., 0.1, 0.1),
            FontWeight => (100., 900., 10., 100.),
            Vertices => (3., 60., 0.1, 1.),
            InnerRadius => (1., 99., 1., 1.),
            _ => (0., MAX_SIZE / 2., 1., 1.),
        }
    }

    pub(in crate::editor) fn begin_property_scrub(
        &mut self,
        index: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some() {
            return;
        }
        if !self.multi_selection.is_empty() && index != 18 {
            if let Some(property) = self.field_property(index) {
                self.begin_multi_property(property, event, window, cx);
            }
            return;
        }
        if self.selected_shape.is_some() && matches!(index, 5 | 6 | 14 | 16 | 17) {
            let stroke = self.field_edits_stroke(index);
            self.activate_paint(stroke, cx);
        }
        let Some(original) = self
            .field_value(index, cx)
            .and_then(|s| s.parse::<f32>().ok())
        else {
            return;
        };
        self.seal_text_edits(cx);
        self.begin(
            GestureKind::Property { index, original },
            event.position,
            event.button,
            window,
            cx,
        );
        self.history.borrow_mut().begin_preview();
    }

    pub(in crate::editor) fn scrub_property(
        &mut self,
        index: usize,
        original: f32,
        delta: f32,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        // A click or tiny hand movement must not change a value or consume redo.
        if delta.abs() < 3. && !self.history.borrow().can_merge(None) {
            return;
        }
        let Some(property) = self.field_property(index) else {
            return;
        };
        let (min, max, step, quantum) = Self::numeric_limits(property);
        let delta = (delta * step * if shift { 10. } else { 1. } / quantum).round() * quantum;
        let value = number((original + delta).clamp(min, max));
        if self.field_value(index, cx).as_deref() == Some(value.as_str()) {
            return;
        }
        let stop = self.inspector.active_stop;
        if property == Property::LayerOpacity {
            self.edit_layer_opacity(&value);
        } else if property == Property::Rotation {
            self.edit_rotation(&value);
        } else if let Some(text) = self.selected_text_mut() {
            let before = text.rect;
            let board = text.board;
            let id = text.id;
            apply_text_field(text, property, &value, stop, cx);
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
        } else if self.selected_shape.is_some() {
            let stroke = self.field_edits_stroke(index);
            self.edit_shape(|shape| apply_shape_field(shape, stroke, stop, property, &value));
        } else {
            self.edit_board(None, |board| apply_field(board, stop, property, &value));
        }
        self.sync_fields(cx);
        cx.notify();
    }

    pub(in crate::editor) fn finish_property_scrub(
        &mut self,
        commit: bool,
        cx: &mut Context<Self>,
    ) {
        let changes = self.history.borrow_mut().end_preview(commit);
        for change in changes {
            match change {
                Change::Hierarchy { value, .. } => self.hierarchy = value,
                Change::Layer { id, value } => {
                    if let Some(layer) = self.layer_state_mut(id) {
                        *layer = value;
                    }
                }
                Change::Board {
                    id,
                    value: Some(before),
                    ..
                } => {
                    if let Some(board) = self.boards.iter_mut().find(|b| b.id == id) {
                        *board = before;
                    }
                }
                Change::Shape {
                    id,
                    value: Some(before),
                    ..
                } => {
                    if let Some(shape) = self.shapes.iter_mut().find(|s| s.id == id) {
                        *shape = *before;
                    }
                }
                Change::TextRect { id, board, value } => {
                    if let Some(text) = self.texts.iter_mut().find(|t| t.id == id) {
                        text.rect = value;
                        text.board = board;
                    }
                }
                Change::Text { id, value } => {
                    if let Some(text) = self.texts.iter().find(|t| t.id == id) {
                        text.editor.update(cx, |editor, cx| {
                            editor.restore(value);
                            cx.notify();
                        });
                    }
                }
                _ => unreachable!("numeric scrubbing only edits the selected object"),
            }
        }
        self.auto_layout.revision = None;
        self.reflow_layout(cx);
        self.sync_fields(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
