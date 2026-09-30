use super::*;
use crate::artboard::LinearGradient;

impl Workspace {
    pub(super) fn begin_style_scrub(
        &mut self,
        angle: bool,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some() {
            return;
        }
        let Some(original) = self.style_number(angle) else {
            return;
        };
        self.begin(
            GestureKind::ColorStyleProperty { angle, original },
            event.position,
            event.button,
            window,
            cx,
        );
    }

    fn style_number(&self, angle: bool) -> Option<f32> {
        let dialog = self.colors.dialog.as_ref()?;
        let gradient = dialog.gradient.as_ref()?;
        Some(if angle {
            gradient.angle
        } else {
            gradient.stop(dialog.active_stop)?.position * 100.
        })
    }

    pub(in crate::workspace) fn set_style_number(
        &mut self,
        angle: bool,
        value: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &mut self.colors.dialog else {
            return;
        };
        let Some(gradient) = &mut dialog.gradient else {
            return;
        };
        if angle {
            gradient.angle = value;
        } else {
            gradient.set_position(dialog.active_stop, value / 100.);
        }
        let input = if angle {
            &self.colors.angle
        } else {
            &self.colors.position
        };
        input.update(cx, |input, cx| {
            input.set_value(inspector::number(value), cx)
        });
        self.colors.error = None;
        cx.notify();
    }

    pub(in crate::workspace) fn scrub_style_number(
        &mut self,
        angle: bool,
        original: f32,
        delta: f32,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        if delta.abs() < 3. && self.style_number(angle) == Some(original) {
            return;
        }
        let property = if angle {
            Property::GradientAngle
        } else {
            Property::GradientPosition
        };
        let (min, max, step, quantum) = Self::numeric_limits(property);
        let delta = (delta * step * if shift { 10. } else { 1. } / quantum).round() * quantum;
        self.set_style_number(angle, (original + delta).clamp(min, max), cx);
    }

    pub(super) fn selected_gradient(&self, stroke: bool, cx: &gpui::App) -> Option<LinearGradient> {
        let (mode, gradient) = if let Some(shape) = self.selected_shape() {
            if stroke {
                (shape.stroke.fill_mode, &shape.stroke.gradient)
            } else {
                (shape.fill_mode, &shape.gradient)
            }
        } else if let Some(text) = self.selected_text() {
            let style = text.editor.read(cx).effective_style();
            (style.fill_mode, &style.gradient)
        } else {
            let board = self.selected_board()?;
            (board.fill_mode, &board.gradient)
        };
        (mode == FillMode::Linear).then(|| gradient.clone())
    }

    pub(super) fn update_gradient_color(&mut self, color: gpui::Rgba) {
        if let Some(dialog) = &mut self.colors.dialog
            && let Some(gradient) = &mut dialog.gradient
            && let Some(stop) = gradient.stop_mut(dialog.active_stop)
        {
            stop.color = color;
        }
    }

    pub(super) fn valid_gradient_inputs(&self, cx: &gpui::App) -> bool {
        self.colors
            .angle
            .read(cx)
            .value()
            .parse::<f32>()
            .is_ok_and(|value| value.is_finite() && (0. ..=360.).contains(&value))
            && self
                .colors
                .position
                .read(cx)
                .value()
                .parse::<f32>()
                .is_ok_and(|value| value.is_finite() && (0. ..=100.).contains(&value))
    }

    pub(super) fn sync_gradient_inputs(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = &self.colors.dialog else {
            return;
        };
        let Some(gradient) = &dialog.gradient else {
            return;
        };
        let Some(stop) = gradient.stop(dialog.active_stop) else {
            return;
        };
        let (color, position, angle) = (stop.color, stop.position, gradient.angle);
        self.colors
            .value
            .update(cx, |input, cx| input.set_value(color_hex(color), cx));
        self.colors
            .picker
            .update(cx, |picker, cx| picker.set_value(color, cx));
        self.colors.angle.update(cx, |input, cx| {
            input.set_value(inspector::number(angle), cx)
        });
        self.colors.position.update(cx, |input, cx| {
            input.set_value(inspector::number(position * 100.), cx)
        });
        self.colors.error = None;
        cx.notify();
    }

    pub(super) fn set_style_gradient(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let color = parse_color(&self.colors.value.read(cx).value()).unwrap_or(rgb(ACCENT));
        let Some(dialog) = &mut self.colors.dialog else {
            return;
        };
        if enabled == dialog.gradient.is_some() {
            return;
        }
        dialog.gradient = enabled.then(|| {
            let mut gradient = LinearGradient::default();
            gradient.stop_mut(0).unwrap().color = color;
            gradient.stop_mut(1).unwrap().color = gpui::Rgba { a: 0., ..color };
            gradient
        });
        dialog.active_stop = 0;
        self.sync_gradient_inputs(cx);
        cx.notify();
    }

    pub(super) fn select_style_stop(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.colors.dialog {
            dialog.active_stop = id;
        }
        self.sync_gradient_inputs(cx);
    }

    pub(super) fn change_style_stops(&mut self, add: bool, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.colors.dialog else {
            return;
        };
        let Some(gradient) = &mut dialog.gradient else {
            return;
        };
        if add {
            if let Some(id) = gradient.add_stop() {
                dialog.active_stop = id;
            }
        } else if gradient.remove_stop(dialog.active_stop) {
            dialog.active_stop = gradient.stops()[0].id;
        }
        self.sync_gradient_inputs(cx);
    }
}
