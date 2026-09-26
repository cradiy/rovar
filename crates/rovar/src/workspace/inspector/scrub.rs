use super::*;

impl Workspace {
    fn numeric_limits(&self, index: usize) -> (f32, f32, f32, f32) {
        if index == 17 {
            return (0., 100., 1., 1.);
        }
        if index == 15 {
            return (-1_000_000., 1_000_000., 1., 1.);
        }
        // Minimum, maximum, units per screen pixel, quantization.
        if self.selected_shape().is_some_and(|s| s.kind.is_line()) && (1..=4).contains(&index) {
            return (-1_000_000., 1_000_000., 1., 1.);
        }
        if self.selected_text.is_some() {
            match index {
                7 => return (1., 1000., 1., 1.),
                8 => return (0.5, 5., 0.01, 0.01),
                9 => return (0., 200., 0.1, 0.1),
                10 => return (100., 900., 10., 100.),
                12 => return (0., 360., 1., 1.),
                13 => return (0., 100., 1., 1.),
                _ => {}
            }
        }
        if let Some(s) = self.selected_shape() {
            if index == 9 && s.kind.is_polygon() {
                return (3., 60., 0.1, 1.);
            }
            if index == 10 && s.kind == ShapeKind::Star {
                return (1., 99., 1., 1.);
            }
        }
        match index {
            1 | 2 => (-1_000_000., 1_000_000., 1., 1.),
            3 | 4 => (MIN_SIZE, MAX_SIZE, 1., 1.),
            6 | 8 => (0., 100., 1., 1.),
            7 => (0., 360., 1., 1.),
            _ => (0., MAX_SIZE / 2., 1., 1.),
        }
    }

    pub(in crate::workspace) fn begin_property_scrub(
        &mut self,
        index: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some() {
            return;
        }
        if !self.multi_selection.is_empty() {
            self.begin_multi_property(index, event, window, cx);
            return;
        }
        if self.selected_shape.is_some() && matches!(index, 5 | 6 | 14 | 16 | 17) {
            let (_, stroke) = self.shape_field_target(index);
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

    pub(in crate::workspace) fn scrub_property(
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
        let (min, max, step, quantum) = self.numeric_limits(index);
        let delta = (delta * step * if shift { 10. } else { 1. } / quantum).round() * quantum;
        let value = number((original + delta).clamp(min, max));
        if self.field_value(index, cx).as_deref() == Some(value.as_str()) {
            return;
        }
        let stop = self.active_stop;
        if index == 15 {
            self.edit_rotation(&value);
        } else if let Some(text) = self.selected_text_mut() {
            let before = text.rect;
            let board = text.board;
            let id = text.id;
            apply_text_field(text, index, &value, stop, cx);
            if text.rect != before {
                self.history.borrow_mut().record(
                    vec![Change::TextRect {
                        id,
                        board,
                        value: before,
                    }],
                    None,
                );
            }
        } else if self.selected_shape.is_some() {
            let (index, stroke) = self.shape_field_target(index);
            self.edit_shape(|shape| apply_shape_field(shape, stroke, stop, index, &value));
        } else {
            self.edit_board(None, |board| apply_field(board, stop, index, &value));
        }
        self.sync_fields(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn finish_property_scrub(
        &mut self,
        commit: bool,
        cx: &mut Context<Self>,
    ) {
        let changes = self.history.borrow_mut().end_preview(commit);
        for change in changes {
            match change {
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
                        *shape = before;
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
        self.sync_fields(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
