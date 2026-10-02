use super::*;
use crate::scene::artboard::LinearGradient;

pub(super) struct Scrub {
    delta: f32,
    value: f32,
    started: bool,
}

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
        self.colors.dialog.as_mut().unwrap().scrub = Some(Scrub {
            delta: 0.,
            value: original,
            started: false,
        });
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

    pub(in crate::editor) fn set_style_number(
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
        self.colors.baselines[if angle { 2 } else { 3 }] = input.read(cx).value();
        self.colors.error = None;
        cx.notify();
    }

    pub(in crate::editor) fn scrub_style_number(
        &mut self,
        angle: bool,
        delta: f32,
        shift: bool,
        fine: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(scrub) = self.colors.dialog.as_mut().and_then(|d| d.scrub.as_mut()) else {
            return;
        };
        if !scrub.started && delta.abs() < 3. {
            return;
        }
        if scrub.started && delta == scrub.delta {
            return;
        }
        scrub.started = true;
        let property = if angle {
            Property::GradientAngle
        } else {
            Property::GradientPosition
        };
        let (min, max, step, _) = Self::numeric_limits(property);
        let scale = if fine {
            0.1
        } else if shift {
            10.
        } else {
            1.
        };
        scrub.value = (scrub.value + (delta - scrub.delta) * step * scale).clamp(min, max);
        scrub.delta = delta;
        let quantum = if fine { 0.1 } else { 1. };
        let value = (scrub.value / quantum).round() * quantum;
        self.set_style_number(angle, value.clamp(min, max), cx);
    }

    pub(in crate::editor) fn begin_style_stop(
        &mut self,
        id: Option<usize>,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some() {
            return;
        }
        let bounds = self.colors.ramp_bounds.get();
        let width = f32::from(bounds.size.width);
        if width <= 0. {
            return;
        }
        let position = (f32::from(event.position.x - bounds.left()) / width).clamp(0., 1.);
        let Some(dialog) = &mut self.colors.dialog else {
            return;
        };
        let Some(gradient) = &mut dialog.gradient else {
            return;
        };
        dialog.stop_before = Some((gradient.clone(), dialog.active_stop));
        let id = match id {
            Some(id) => id,
            None => {
                if let Some(stop) = gradient
                    .stops()
                    .iter()
                    .find(|s| (s.position - position).abs() * width < 10.)
                {
                    stop.id
                } else if let Some(id) = gradient.add_stop_at(position) {
                    id
                } else {
                    return;
                }
            }
        };
        let original = gradient.stop(id).unwrap().position * 100.;
        self.select_style_stop(id, cx);
        // Preserve the pointer's offset within the handle to avoid a jump on click.
        let left = f32::from(event.position.x) - original / 100. * width;
        self.begin(
            GestureKind::ColorStyleStop { left, width },
            event.position,
            event.button,
            window,
            cx,
        );
    }

    pub(in crate::editor) fn finish_style_stop(&mut self, commit: bool, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.colors.dialog
            && let Some((gradient, active)) = dialog.stop_before.take()
            && !commit
        {
            dialog.gradient = Some(gradient);
            dialog.active_stop = active;
            self.sync_gradient_inputs(cx);
        }
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
        if let Some(dialog) = &mut self.colors.dialog {
            if let Some(gradient) = &mut dialog.gradient {
                if let Some(stop) = gradient.stop_mut(dialog.active_stop) {
                    stop.color = color;
                }
            } else {
                dialog.solid = color;
            }
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

    pub(in crate::editor) fn sync_gradient_inputs(&mut self, cx: &mut Context<Self>) {
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
        for index in 1..4 {
            self.colors.baselines[index] = self.style_inputs()[index].read(cx).value();
        }
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
        if enabled {
            dialog.solid = color;
            dialog.gradient = Some(dialog.saved_gradient.take().unwrap_or_else(|| {
                let mut gradient = LinearGradient::default();
                gradient.stop_mut(0).unwrap().color = color;
                gradient.stop_mut(1).unwrap().color = gpui::Rgba { a: 0., ..color };
                gradient
            }));
            if !dialog
                .gradient
                .as_ref()
                .unwrap()
                .stops()
                .iter()
                .any(|s| s.id == dialog.active_stop)
            {
                dialog.active_stop = dialog.gradient.as_ref().unwrap().stops()[0].id;
            }
            self.sync_gradient_inputs(cx);
        } else {
            dialog.saved_gradient = dialog.gradient.take();
            let color = dialog.solid;
            self.colors
                .value
                .update(cx, |i, cx| i.set_value(color_hex(color), cx));
            self.colors
                .picker
                .update(cx, |p, cx| p.set_value(color, cx));
        }
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
