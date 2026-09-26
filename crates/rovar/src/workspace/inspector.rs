use super::*;
use crate::i18n::t;
#[cfg(test)]
mod fill_tests;
mod image_fill;
mod name_input;
mod scrub;
use crate::artboard::{LinearGradient, MAX_SIZE, MIN_SIZE};
use uic::components::{
    color_picker::{ColorPicker, ColorPickerAppearance},
    input::{Input, InputAppearance},
    popover::{Popover, PopoverPlacement},
};

impl Workspace {
    pub(super) fn shape_field_target(&self, index: usize) -> (usize, bool) {
        match index {
            16 | 17 => (index - 11, true),
            5 | 6 => (index, false),
            14 => (index, true),
            _ => (index, self.stroke_editing),
        }
    }
    pub(super) fn shape_paint_stop(&self, stroke: bool) -> usize {
        let id = if stroke == self.stroke_editing {
            self.active_stop
        } else {
            self.paint_stops[stroke as usize]
        };
        let gradient = self.selected_shape().unwrap().paint_gradient(stroke);
        if gradient.stop(id).is_some() {
            id
        } else {
            gradient.stops()[0].id
        }
    }
    fn fill_state(&self, cx: &gpui::App) -> Option<(FillMode, LinearGradient)> {
        if let Some(text) = self.selected_text() {
            let editor = text.editor.read(cx);
            let style = editor.effective_style();
            Some((style.fill_mode, style.gradient.clone()))
        } else if let Some(shape) = self.selected_shape() {
            Some((
                shape.paint_mode(self.stroke_editing),
                shape.paint_gradient(self.stroke_editing).clone(),
            ))
        } else {
            self.selected_board()
                .map(|board| (board.fill_mode, board.gradient.clone()))
        }
    }
    fn mutate_gradient<R>(
        &mut self,
        change: impl FnOnce(&mut LinearGradient) -> R,
        cx: &mut Context<Self>,
    ) -> Option<R> {
        self.history.borrow_mut().break_group();
        let (_, mut gradient) = self.fill_state(cx)?;
        let result = change(&mut gradient);
        if self.selected_text.is_some() {
            self.change_text_style(StyleChange::Gradient(gradient), cx);
        } else if self.selected_shape.is_some() {
            let stroke = self.stroke_editing;
            self.edit_shape(|shape| *shape.paint_gradient_mut(stroke) = gradient);
        } else {
            self.edit_board(None, |board| board.gradient = gradient);
        }
        Some(result)
    }
    fn gradient_kind_control(
        &self,
        selected: gpui::GradientKind,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        use gpui::GradientKind;
        use uic::components::dropdown::dropdown;
        let choices = [
            (
                GradientKind::Linear,
                t("gradient-linear"),
                "gradient-linear",
            ),
            (
                GradientKind::Radial,
                t("gradient-radial"),
                "gradient-radial",
            ),
            (
                GradientKind::Angular,
                t("gradient-angular"),
                "gradient-angular",
            ),
            (
                GradientKind::Diamond,
                t("gradient-diamond"),
                "gradient-diamond",
            ),
        ];
        let label = choices
            .iter()
            .find(|(kind, _, _)| *kind == selected)
            .unwrap()
            .1;
        div().flex_1().min_w_0().child(
            dropdown(&self.gradient_menu)
                .priority(1100)
                .w(px(168.))
                .min_w(px(168.))
                .p(px(4.))
                .rounded(px(8.))
                .shadow_lg()
                .bg(rgb(0x282b33))
                .border_color(rgb(BORDER))
                .text_size(px(12.))
                .text_color(rgb(TEXT))
                .trigger(
                    div()
                        .id("gradient-kind")
                        .debug_selector(|| "gradient-kind".into())
                        .w_full()
                        .h(px(30.))
                        .px(px(10.))
                        .rounded(px(6.))
                        .bg(rgb(0x282b33))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(label)
                        .child(icon(LucideIcons::ChevronDown, 14.)),
                )
                .menu(
                    div()
                        .debug_selector(|| "gradient-kind-menu".into())
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .children(choices.into_iter().map(|(kind, label, id)| {
                            div()
                                .id(id)
                                .debug_selector(move || id.into())
                                .h(px(28.))
                                .px(px(8.))
                                .rounded(px(4.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0x353044)))
                                .child(icon(LucideIcons::Check, 13.).opacity(if selected == kind {
                                    1.
                                } else {
                                    0.
                                }))
                                .child(label)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.mutate_gradient(|g| g.kind = kind, cx);
                                    this.gradient_menu
                                        .update(cx, |state, cx| state.close(window, cx));
                                    cx.notify();
                                }))
                        })),
                ),
        )
    }

    pub(super) fn fill_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some((mode, gradient)) = self.fill_state(cx) else {
            return div();
        };
        let mut preview = gradient.clone();
        preview.angle = 90.;
        preview.kind = gpui::GradientKind::Linear;
        let text_selected = self.selected_text.is_some();
        let color_index = if self.selected_shape.is_some() && self.stroke_editing {
            16
        } else {
            5
        };
        let alpha_index = if self.selected_shape.is_some() && self.stroke_editing {
            17
        } else {
            6
        };
        let stop_count = gradient.stops().len();
        let image_allowed = !text_selected
            && (self.selected_shape.is_none() || !self.stroke_editing)
            && self.current_image_fill().is_some()
            && self.selected_shape().is_none_or(|s| s.can_fill());
        let mixed_mode = self
            .selected_text()
            .is_some_and(|t| t.editor.read(cx).mixed(12));
        let modes = [
            (
                FillMode::Solid,
                t("solid"),
                "fill-solid",
                LucideIcons::PaintBucket,
            ),
            (
                FillMode::Linear,
                t("gradient"),
                "fill-linear",
                LucideIcons::Blend,
            ),
            (
                FillMode::Image,
                t("shape-image"),
                "fill-image",
                LucideIcons::Image,
            ),
        ];
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(8.))
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .pb(px(8.))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .children(
                        modes
                            .into_iter()
                            .filter(|(m, _, _, _)| *m != FillMode::Image || image_allowed)
                            .map(|(kind, label, id, glyph)| {
                                icon_button(id, label, glyph, mode == kind && !mixed_mode).on_click(
                                    cx.listener(move |this, _, window, cx| {
                                        let surface = usize::from(
                                            this.selected_shape.is_some() && this.stroke_editing,
                                        );
                                        this.paint_popovers[surface]
                                            .focus_handle(cx)
                                            .focus(window, cx);
                                        this.history.borrow_mut().break_group();
                                        if this.selected_text.is_some() {
                                            this.change_text_style(StyleChange::FillMode(kind), cx);
                                        } else if this.selected_shape.is_some() {
                                            let stroke = this.stroke_editing;
                                            this.edit_shape(|s| {
                                                if stroke {
                                                    s.stroke.fill_mode = kind
                                                } else {
                                                    s.fill_mode = kind
                                                }
                                            });
                                        } else {
                                            this.edit_board(None, |b| b.fill_mode = kind);
                                        }
                                        this.sync_fields(cx);
                                        cx.notify();
                                    }),
                                )
                            }),
                    )
                    .child(div().flex_1())
                    .child(
                        icon_button("color-close", t("close"), LucideIcons::X, false).on_click(
                            cx.listener(|this, _, window, cx| {
                                for popover in &this.paint_popovers {
                                    popover.update(cx, |state, cx| state.close(window, cx));
                                }
                            }),
                        ),
                    ),
            )
            .when(mode == FillMode::Image, |el| {
                el.child(self.image_fill_controls(cx))
            })
            .when(mode == FillMode::Linear, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(self.gradient_kind_control(gradient.kind, cx))
                        .when(gradient.kind != gpui::GradientKind::Radial, |el| {
                            el.child(div().w(px(74.)).child(self.paint_input_field(
                                if text_selected { 12 } else { 7 },
                                t("angle"),
                                cx,
                            )))
                        })
                        .child(
                            icon_button(
                                "gradient-reverse",
                                t("gradient-reverse"),
                                LucideIcons::ArrowLeftRight,
                                false,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.mutate_gradient(|g| g.reverse(), cx);
                                this.sync_fields(cx);
                                cx.notify();
                            })),
                        )
                        .when(gradient.kind != gpui::GradientKind::Radial, |el| {
                            el.child(
                                icon_button(
                                    "gradient-rotate",
                                    t("rotate-quarter"),
                                    LucideIcons::RotateCw,
                                    false,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.mutate_gradient(
                                            |g| g.angle = (g.angle + 90.).rem_euclid(360.),
                                            cx,
                                        );
                                        this.sync_fields(cx);
                                        cx.notify();
                                    },
                                )),
                            )
                        }),
                )
                .child(
                    div()
                        .relative()
                        .h(px(24.))
                        .rounded(px(6.))
                        .overflow_hidden()
                        .bg(rgb(0xffffff))
                        .child(
                            div()
                                .absolute()
                                .inset_0()
                                .bg(gpui::checkerboard(rgb(0xd9dce2), 6.)),
                        )
                        .child(div().absolute().inset_0().bg(preview.background())),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(MUTED))
                                .child(t("stops")),
                        )
                        .child(
                            gradient_button(
                                "gradient-add",
                                t("stop-add"),
                                LucideIcons::Plus,
                                stop_count < 4,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(Some(id)) = this.mutate_gradient(|g| g.add_stop(), cx) {
                                    this.active_stop = id;
                                    this.sync_fields(cx);
                                    cx.notify();
                                }
                            })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .children(gradient.stops().iter().map(|stop| {
                            let id = stop.id;
                            let active = id == self.active_stop;
                            div()
                                .id(("gradient-stop", id))
                                .debug_selector(move || format!("gradient-stop-{id}"))
                                .h(px(38.))
                                .rounded(px(6.))
                                .p(px(3.))
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .bg(rgb(if active { 0x353044 } else { 0x22252d }))
                                .text_size(px(12.))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        if this.active_stop != id {
                                            this.history.borrow_mut().break_group();
                                            this.active_stop = id;
                                            this.sync_fields(cx);
                                            cx.notify();
                                        }
                                    }),
                                )
                                .when(active, |el| {
                                    el.child(div().w(px(78.)).child(self.paint_input_field(
                                        if text_selected { 13 } else { 8 },
                                        t("stop-position"),
                                        cx,
                                    )))
                                    .child(self.paint_input_field(color_index, t("color-hex"), cx))
                                    .child(
                                        div().w(px(70.)).child(self.paint_input_field(
                                            alpha_index,
                                            t("opacity"),
                                            cx,
                                        )),
                                    )
                                })
                                .when(!active, |el| {
                                    el.child(
                                        div()
                                            .w(px(78.))
                                            .h(px(32.))
                                            .flex()
                                            .items_center()
                                            .child(
                                                div()
                                                    .w(px(28.))
                                                    .text_center()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(MUTED))
                                                    .child("%"),
                                            )
                                            .child(
                                                div()
                                                    .pl(px(5.))
                                                    .child(number(stop.position * 100.)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .h(px(32.))
                                            .flex()
                                            .items_center()
                                            .child(
                                                div().px(px(7.)).child(
                                                    div().size(px(18.)).bg(rgb(0xffffff)).child(
                                                        div()
                                                            .size_full()
                                                            .bg(gpui::checkerboard(
                                                                rgb(0xd9dce2),
                                                                4.,
                                                            ))
                                                            .child(
                                                                div().size_full().bg(stop.color),
                                                            ),
                                                    ),
                                                ),
                                            )
                                            .child(div().pl(px(5.)).child(hex(stop.color))),
                                    )
                                    .child(
                                        div()
                                            .w(px(70.))
                                            .h(px(32.))
                                            .flex()
                                            .items_center()
                                            .child(
                                                div()
                                                    .w(px(28.))
                                                    .text_center()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(MUTED))
                                                    .child("%"),
                                            )
                                            .child(
                                                div().pl(px(5.)).child(number(stop.color.a * 100.)),
                                            ),
                                    )
                                })
                                .child(
                                    div()
                                        .id(("gradient-delete", id))
                                        .debug_selector(move || {
                                            if active {
                                                "gradient-remove".into()
                                            } else {
                                                format!("gradient-delete-{id}")
                                            }
                                        })
                                        .w(px(22.))
                                        .h(px(28.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .opacity(if stop_count > 2 { 1. } else { 0.3 })
                                        .child(icon(LucideIcons::Minus, 14.))
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation()
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(true) =
                                                this.mutate_gradient(|g| g.remove_stop(id), cx)
                                            {
                                                this.sync_fields(cx);
                                                cx.notify();
                                            }
                                        })),
                                )
                        })),
                )
            })
            .when(mode != FillMode::Image, |el| {
                el.child(self.color_picker())
                    .child(
                        uic::components::color_picker::AlphaSlider::new(&self.alpha_picker)
                            .h(px(12.))
                            .w_full(),
                    )
                    .when(mode == FillMode::Solid, |el| {
                        el.child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .child(self.paint_input_field(color_index, t("color-hex"), cx))
                                .child(div().w(px(82.)).child(self.paint_input_field(
                                    alpha_index,
                                    t("opacity"),
                                    cx,
                                ))),
                        )
                    })
            })
    }

    pub(super) fn field_value(&self, index: usize, cx: &gpui::App) -> Option<String> {
        if index == 15 {
            return self
                .selected_text
                .or(self.selected_shape)
                .filter(|_| self.multi_selection.is_empty())
                .map(|id| number(self.object_rotation(id)));
        }
        if !self.multi_selection.is_empty() {
            return self.multi_field_value(index, cx);
        }
        if let Some(text) = self.selected_text() {
            let editor = text.editor.read(cx);
            let style = editor.effective_style();
            if (5..=10).contains(&index) && editor.mixed(index) {
                return Some(String::new());
            }
            if matches!(index, 5 | 6 | 12 | 13)
                && (editor.mixed(12) || (style.fill_mode == FillMode::Linear && editor.mixed(13)))
            {
                return Some(String::new());
            }
            return Some(match index {
                0 => return None,
                1 => number(text.rect.x),
                2 => number(text.rect.y),
                3 => number(text.rect.width),
                4 => number(text.rect.height),
                5 => hex(style.editable_color(self.active_stop)),
                6 => number(style.editable_color(self.active_stop).a * 100.),
                7 => number(style.size),
                8 => number(style.line_height),
                9 => number(style.spacing),
                10 => number(style.weight),
                12 => number(style.gradient.angle),
                13 => number(
                    style
                        .gradient
                        .stop(self.active_stop)
                        .unwrap_or(&style.gradient.stops()[0])
                        .position
                        * 100.,
                ),
                _ => return None,
            });
        }
        if let Some(shape) = self.selected_shape() {
            let (index, stroke) = self.shape_field_target(index);
            let stop = self.shape_paint_stop(stroke);
            let color = shape.paint_color(stop, stroke);
            return Some(match index {
                0 => shape.name.clone(),
                1 if shape.kind.is_line() => number(shape.display_path_point(0).x),
                2 if shape.kind.is_line() => number(shape.display_path_point(0).y),
                3 if shape.kind.is_line() => number(shape.display_path_point(1).x),
                4 if shape.kind.is_line() => number(shape.display_path_point(1).y),
                1 => number(shape.rect.x),
                2 => number(shape.rect.y),
                3 => number(shape.rect.width),
                4 => number(shape.rect.height),
                5 if !stroke && shape.fill_mode == FillMode::Image => t("shape-image").into(),
                5 => hex(color),
                6 if !stroke && shape.fill_mode == FillMode::Image => {
                    number(shape.image_fill.opacity * 100.)
                }
                6 => number(color.a * 100.),
                7 => number(shape.paint_gradient(stroke).angle),
                8 => number(shape.paint_gradient(stroke).stop(stop)?.position * 100.),
                9 if shape.kind.supports_corners() => number(shape.radius),
                9 if shape.kind.is_polygon() => shape.vertices.to_string(),
                10 if shape.kind == ShapeKind::Star => number(shape.inner_radius * 100.),
                10..=13 if shape.kind.supports_corners() => {
                    number(shape.corners.unwrap_or([shape.radius; 4])[index - 10])
                }
                14 => number(shape.stroke.width),
                _ => return None,
            });
        }
        if index >= 9 {
            return None;
        }
        let board = self.selected_board()?;
        let color = board.editable_color(self.active_stop);
        Some(match index {
            0 => board.name.clone(),
            1 => number(board.rect.x),
            2 => number(board.rect.y),
            3 => number(board.rect.width),
            4 => number(board.rect.height),
            5 if board.fill_mode == FillMode::Image => t("shape-image").into(),
            5 => format!(
                "{:02X}{:02X}{:02X}",
                (color.r * 255.).round() as u8,
                (color.g * 255.).round() as u8,
                (color.b * 255.).round() as u8
            ),
            6 if board.fill_mode == FillMode::Image => number(board.image_fill.opacity * 100.),
            6 => number(color.a * 100.),
            7 => number(board.gradient.angle),
            _ => number(board.gradient.stop(self.active_stop)?.position * 100.),
        })
    }

    pub(super) fn sync_picker(&mut self, color: gpui::Rgba, cx: &mut Context<Self>) {
        for picker in [&self.picker, &self.alpha_picker] {
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

    pub(super) fn sync_field(&mut self, index: usize, cx: &mut Context<Self>) {
        for surface in 0..PROPERTY_SURFACES {
            self.sync_input_field(index + surface * PROPERTY_COUNT, cx);
        }
    }

    pub(super) fn sync_input_field(&mut self, slot: usize, cx: &mut Context<Self>) {
        if slot >= PROPERTY_COUNT && slot / PROPERTY_COUNT != self.paint_surface(cx) {
            return;
        }
        let index = slot % PROPERTY_COUNT;
        if self.invalid[slot] {
            self.invalid[slot] = false;
            cx.notify();
        }
        if let Some(value) = self.field_value(index, cx) {
            self.fields[slot].update(cx, |input, _| {
                input.set_placeholder(if value.is_empty() { t("mixed") } else { "" });
            });
            let value: gpui::SharedString = value.into();
            if self.fields[slot].read(cx).value() != value {
                // set_value emits Change. Consume exactly that programmatic
                // event, including when selection changes before dispatch.
                self.pending.push((slot, value.clone()));
                self.fields[slot].update(cx, |input, cx| {
                    input.set_value(value, cx);
                });
            }
        }
    }

    pub(super) fn sync_fields(&mut self, cx: &mut Context<Self>) {
        if let Some((_, gradient)) = self.fill_state(cx)
            && gradient.stop(self.active_stop).is_none()
        {
            self.active_stop = gradient.stops()[0].id;
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
            let color = style.editable_color(self.active_stop);
            let family = if editor.mixed(0) {
                t("font-mixed").into()
            } else {
                style.family.clone()
            };
            self.font_picker
                .update(cx, |picker, cx| picker.set_selected(family, cx));
            self.sync_picker(color, cx);
        } else if let Some(shape) = self.selected_shape() {
            let color = shape.paint_color(self.active_stop, self.stroke_editing);
            self.sync_picker(color, cx);
        } else if let Some(board) = self.selected_board() {
            let color = board.editable_color(self.active_stop);
            self.sync_picker(color, cx);
        }
    }
    pub(super) fn sync_text_inspector(&mut self, window: &Window, cx: &mut Context<Self>) {
        for index in 0..self.fields.len() {
            if !self.fields[index].focus_handle(cx).is_focused(window) {
                self.sync_input_field(index, cx);
            }
        }
        if let Some(text) = self.selected_text() {
            let editor = text.editor.read(cx);
            let style = editor.effective_style();
            let color = style.editable_color(self.active_stop);
            let family = if editor.mixed(0) {
                t("font-mixed").into()
            } else {
                style.family.clone()
            };
            if style.gradient.stop(self.active_stop).is_none() {
                self.active_stop = style.gradient.stops()[0].id;
            }
            self.font_picker
                .update(cx, |picker, cx| picker.set_selected(family, cx));
            self.sync_picker(color, cx);
        }
    }

    pub(super) fn edit_field(&mut self, slot: usize, event: &InputEvent, cx: &mut Context<Self>) {
        let index = slot % PROPERTY_COUNT;
        let value = event.text();
        if matches!(event, InputEvent::Change(_)) {
            if let Some(pos) = self
                .pending
                .iter()
                .position(|(field, text)| *field == slot && text == value)
            {
                self.pending.remove(pos);
                return;
            }
            if self.fields[slot].read(cx).value() != *value {
                return;
            }
        }
        if slot >= PROPERTY_COUNT && slot / PROPERTY_COUNT != self.paint_surface(cx) {
            return;
        }
        if !self.multi_selection.is_empty() {
            self.invalid[slot] = !self.edit_multi_field(index, value, cx);
            if !self.invalid[slot] && matches!(index, 3 | 4) {
                self.sync_field(7 - index, cx);
            }
            if !self.invalid[slot]
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
            let (_, stroke) = self.shape_field_target(index);
            if stroke != self.stroke_editing {
                self.history.borrow_mut().break_group();
                self.set_paint_target(stroke);
            }
        }
        let stop = self.active_stop;
        self.history
            .borrow_mut()
            .set_scope(Group::Property(id, index, stop), false);
        if index == 15 {
            self.invalid[slot] = !self.edit_rotation(value);
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
            let valid = apply_text_field(text, index, value, stop, cx);
            let color = text.editor.read(cx).effective_style().editable_color(stop);
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
            (valid, color)
        } else if self.selected_shape.is_some() {
            let (local_index, stroke) = self.shape_field_target(index);
            let valid = self
                .edit_shape(|shape| apply_shape_field(shape, stroke, stop, local_index, value))
                .unwrap_or(false);
            (
                valid,
                self.selected_shape().unwrap().paint_color(stop, stroke),
            )
        } else {
            let valid = self
                .edit_board(None, |board| apply_field(board, stop, index, value))
                .unwrap_or(false);
            (valid, self.selected_board().unwrap().editable_color(stop))
        };
        self.history.borrow_mut().clear_scope();
        self.invalid[slot] = !valid;
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
        if self.invalid[slot] {
            return;
        }
        for surface in 0..PROPERTY_SURFACES {
            let peer = slot % PROPERTY_COUNT + surface * PROPERTY_COUNT;
            if peer != slot {
                self.sync_input_field(peer, cx);
            }
        }
    }

    fn paint_surface(&self, cx: &gpui::App) -> usize {
        match self.fill_state(cx).map(|(mode, _)| mode) {
            Some(FillMode::Linear) => 2,
            Some(FillMode::Image) => 3,
            _ => 1,
        }
    }

    pub(super) fn property_field(
        &self,
        index: usize,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        self.property_field_impl(index, label, true, cx)
    }
    fn paint_input_field(
        &self,
        index: usize,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        self.property_field_impl(index, label, false, cx)
    }
    fn property_field_impl(
        &self,
        index: usize,
        label: &'static str,
        popup: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let is_color = matches!(index, 5 | 16);
        let slot = index
            + if popup {
                0
            } else {
                PROPERTY_COUNT * self.paint_surface(cx)
            };
        let image_color = index == 5
            && self.selected_text.is_none()
            && (self
                .selected_shape()
                .is_some_and(|s| s.fill_mode == FillMode::Image)
                || (self.selected_shape.is_none()
                    && self
                        .selected_board()
                        .is_some_and(|b| b.fill_mode == FillMode::Image)));
        let sidebar_duplicate = popup
            && matches!(index, 5 | 6 | 16 | 17)
            && self.paint_popovers[usize::from(index >= 16)]
                .read(cx)
                .is_open();
        let numeric = !matches!(index, 0 | 5 | 16);
        let draggable = if !self.multi_selection.is_empty() {
            self.multi_can_scrub(index, cx)
        } else {
            numeric
                && self
                    .field_value(index, cx)
                    .is_some_and(|value| value.parse::<f32>().is_ok())
        };
        let short_label = match label {
            value if value == t("width") => "W",
            value if value == t("height") => "H",
            value if value == t("opacity") => "%",
            value if value == t("color-hex") => "#",
            value if value == t("line-height") => t("line-height-short"),
            value if value == t("letter-spacing") => t("letter-spacing-short"),
            value if value == t("stroke-width") => t("stroke-width-short"),
            value if value == t("gradient-angular") || value == t("angle") => "∠",
            value if value == t("stop-position") => "%",
            _ => label,
        };
        let label_icon = match label {
            value if value == t("rotation") => Some(LucideIcons::RotateCw),
            value if value == t("corner-radius") => Some(LucideIcons::Radius),
            value if value == t("top-left") => Some(LucideIcons::CornerUpLeft),
            value if value == t("top-right") => Some(LucideIcons::CornerUpRight),
            value if value == t("bottom-left") => Some(LucideIcons::CornerDownLeft),
            value if value == t("bottom-right") => Some(LucideIcons::CornerDownRight),
            value if value == t("stroke-width") => Some(LucideIcons::Minus),
            value if value == t("name") => Some(LucideIcons::Pencil),
            _ => None,
        };
        let swatch = if index == 5 && !self.multi_selection.is_empty() {
            self.multi_field_value(5, cx)
                .and_then(|s| u32::from_str_radix(&s, 16).ok())
                .map(|v| rgb(v).into())
        } else if is_color && self.selected_shape.is_some() {
            let shape = self.selected_shape().unwrap();
            let stroke = index == 16;
            Some(if popup && shape.paint_mode(stroke) == FillMode::Linear {
                shape.paint_gradient(stroke).background()
            } else {
                shape
                    .paint_color(self.shape_paint_stop(stroke), stroke)
                    .into()
            })
        } else if is_color {
            self.fill_state(cx).map(|(mode, gradient)| {
                if popup && mode == FillMode::Linear {
                    gradient.background()
                } else {
                    self.picker.read(cx).value().into()
                }
            })
        } else {
            None
        };
        div()
            .flex_1()
            .min_w_0()
            .flex_shrink_0()
            .h(px(32.))
            .rounded(px(6.))
            .bg(rgb(0x282b33))
            .border_1()
            .border_color(rgb(if self.invalid[slot] {
                0xdd7272
            } else {
                0x30333d
            }))
            .when(!popup, |el| {
                el.bg(gpui::rgba(0)).border_color(gpui::rgba(0))
            })
            .flex()
            .items_center()
            .child({
                let prefix = div()
                    .id(("property-label", index))
                    .debug_selector(move || format!("property-drag-{index}"))
                    .h_full()
                    .min_w(px(28.))
                    .px(px(7.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .when(draggable, |el| {
                        el.cursor(gpui::CursorStyle::ResizeLeftRight)
                            .hover(|s| s.text_color(rgb(TEXT)).bg(rgb(0x383d47)))
                    })
                    .tooltip(move |_, cx| {
                        cx.new(|_| {
                            toolbar::ToolTip(if draggable {
                                crate::i18n::message("scrub-hint", &[("label", label.into())])
                            } else if is_color {
                                t("edit-color").into()
                            } else {
                                label.to_owned()
                            })
                        })
                        .into()
                    })
                    .when(is_color, |el| {
                        el.cursor_pointer().child(
                            div()
                                .size(px(18.))
                                .rounded(px(3.))
                                .overflow_hidden()
                                .relative()
                                .bg(gpui::checkerboard(rgb(0xd9dce2), 4.))
                                .when_some(swatch, |el, paint| {
                                    el.child(div().absolute().inset_0().bg(paint))
                                })
                                .when(image_color, |el| {
                                    let fill = self
                                        .selected_shape()
                                        .map(|s| &s.image_fill)
                                        .or_else(|| self.selected_board().map(|b| &b.image_fill));
                                    el.when_some(fill, |el, fill| {
                                        el.child(fill.element().absolute().inset_0())
                                    })
                                }),
                        )
                    })
                    .when(!is_color, |el| match label_icon {
                        Some(glyph) => el.child(icon(glyph, 15.).text_color(rgb(MUTED))),
                        None => el.child(short_label),
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event, window, cx| {
                            if draggable {
                                this.begin_property_scrub(index, event, window, cx);
                            }
                            if !is_color {
                                cx.stop_propagation();
                            }
                        }),
                    );
                if is_color && popup {
                    let weak = cx.entity().downgrade();
                    Popover::new(&self.paint_popovers[usize::from(index == 16)])
                        .label(t("color"))
                        .placement(PopoverPlacement::LeftStart)
                        .gap(px(20.))
                        .p_0()
                        .border_0()
                        .bg(gpui::rgba(0))
                        .trigger(prefix)
                        .content(move |window, cx| {
                            div().children(
                                weak.update(cx, |this, cx| this.color_panel(window, cx))
                                    .ok(),
                            )
                        })
                        .into_any_element()
                } else {
                    prefix.into_any_element()
                }
            })
            .child(
                div()
                    .id(("property", index))
                    .debug_selector(move || {
                        if sidebar_duplicate {
                            format!("property-sidebar-{index}")
                        } else {
                            format!("property-{index}")
                        }
                    })
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .when(index == 0, |el| el.child(self.name_input(cx)))
                    .when(index != 0 && image_color, |el| {
                        el.child(
                            div()
                                .text_size(px(12.))
                                .overflow_hidden()
                                .child(self.field_value(index, cx).unwrap_or_default()),
                        )
                    })
                    .when(index != 0 && !image_color, |el| {
                        el.child(
                            Input::new(&self.fields[slot])
                                .w_full()
                                .h(px(30.))
                                .px(px(4.))
                                .rounded(px(4.))
                                .text_size(px(12.))
                                .text_color(rgb(TEXT))
                                .bg(rgb(0x282b33))
                                .border_color(rgb(0x282b33))
                                .when(!popup, |el| {
                                    el.bg(gpui::rgba(0)).border_color(gpui::rgba(0))
                                })
                                .appearance(InputAppearance {
                                    focus_border: rgb(ACCENT).into(),
                                    caret: rgb(ACCENT).into(),
                                    selection: gpui::rgba(0xb4a2ee44).into(),
                                    caret_height: px(16.),
                                    ..Default::default()
                                }),
                        )
                    }),
            )
    }

    pub(super) fn paint_value_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        self.paint_value_row_for(false, cx)
    }
    pub(super) fn paint_value_row_for(
        &self,
        stroke: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .debug_selector(move || {
                if stroke {
                    "stroke-paint-value-row".into()
                } else {
                    "paint-value-row".into()
                }
            })
            .h(px(32.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .bg(rgb(0x282b33))
            .child(self.property_field(if stroke { 16 } else { 5 }, t("color-hex"), cx))
            .child(div().w(px(1.)).h(px(20.)).flex_shrink_0().bg(rgb(BORDER)))
            .child(div().w(px(76.)).flex_shrink_0().child(self.property_field(
                if stroke { 17 } else { 6 },
                t("opacity"),
                cx,
            )))
    }

    pub(super) fn properties(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected_board().is_some()
            || self.selected_shape.is_some()
            || self.selected_text.is_some();
        let (title, glyph) = if !self.multi_selection.is_empty() {
            (
                if self.multi_selection.len() == 1 {
                    t("group-selection")
                } else {
                    t("multi-selection")
                },
                LucideIcons::Layers,
            )
        } else if self.selected_text.is_some() {
            (t("text"), LucideIcons::Type)
        } else if let Some(shape) = self.selected_shape() {
            (
                if shape.kind == ShapeKind::Bezier {
                    t("path")
                } else {
                    shape.kind.label()
                },
                match shape.kind {
                    ShapeKind::Rectangle => LucideIcons::Square,
                    ShapeKind::Ellipse => LucideIcons::Circle,
                    ShapeKind::Arrow => LucideIcons::ArrowUpRight,
                    ShapeKind::Polygon => LucideIcons::Triangle,
                    ShapeKind::Star => LucideIcons::Star,
                    ShapeKind::Image => LucideIcons::Image,
                    ShapeKind::Video => LucideIcons::Film,
                    ShapeKind::Line => LucideIcons::Minus,
                    ShapeKind::Pen => LucideIcons::Pencil,
                    ShapeKind::Bezier => LucideIcons::PenTool,
                },
            )
        } else if selected {
            (t("artboard"), LucideIcons::Frame)
        } else {
            (t("design"), LucideIcons::SlidersHorizontal)
        };
        layers::glass_surface()
            .id("properties-panel")
            .debug_selector(|| "properties-panel".into())
            .absolute()
            .top(px(panels::PANEL_TOP))
            .bottom(px(panels::PANEL_BOTTOM))
            .right(px(14.))
            .w(px(self.panels.width(panels::Side::Right)))
            .flex_shrink_0()
            .rounded(px(16.))
            .border_1()
            .border_color(gpui::rgba(0xb4a2ee30))
            .shadow_lg()
            .occlude()
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(48.))
                    .px(px(16.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .child(icon(glyph, 17.).text_color(rgb(ACCENT)))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .when(selected || !self.multi_selection.is_empty(), |el| {
                        el.child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(MUTED))
                                .child(t("design")),
                        )
                    }),
            )
            .child(div().h(px(1.)).flex_shrink_0().bg(rgb(BORDER)))
            .when(self.selected_text.is_some(), |el| {
                el.child(self.text_properties(cx))
            })
            .when(selected && self.selected_text.is_none(), |el| {
                el.child(
                    div()
                        .id("properties-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .px(px(14.))
                                .py(px(10.))
                                .flex()
                                .flex_shrink_0()
                                .child(self.property_field(0, t("name"), cx)),
                        )
                        .child(self.geometry_controls(cx))
                        .when(self.vector_edit.is_some(), |el| {
                            el.child(self.node_controls(cx))
                        })
                        .when(
                            self.selected_shape()
                                .is_some_and(|s| s.kind.supports_corners()),
                            |el| {
                                el.child(
                                    inspector_section(t("appearance"))
                                        .child(self.shape_corner_controls(cx)),
                                )
                            },
                        )
                        .when(
                            self.selected_shape().is_some_and(|s| s.kind.is_polygon()),
                            |el| {
                                el.child(
                                    inspector_section(t("shape")).child(
                                        div()
                                            .flex()
                                            .gap(px(8.))
                                            .child(self.property_field(
                                                9,
                                                if self.selected_shape().unwrap().kind
                                                    == ShapeKind::Star
                                                {
                                                    t("star-points")
                                                } else {
                                                    t("polygon-sides")
                                                },
                                                cx,
                                            ))
                                            .when(
                                                self.selected_shape().unwrap().kind
                                                    == ShapeKind::Star,
                                                |el| {
                                                    el.child(self.property_field(
                                                        10,
                                                        t("inner-radius"),
                                                        cx,
                                                    ))
                                                },
                                            ),
                                    ),
                                )
                            },
                        )
                        .when(
                            self.selected_shape().is_some_and(|s| s.kind.is_media()),
                            |el| el.child(self.media_properties(cx)),
                        )
                        .when(
                            self.selected_shape().is_some_and(|s| !s.kind.is_media()),
                            |el| el.child(self.shape_paint_controls(cx)),
                        )
                        .when(self.selected_shape.is_none(), |el| {
                            el.child(inspector_section(t("fill")).child(self.paint_value_row(cx)))
                        }),
                )
            })
            .when(!self.multi_selection.is_empty(), |el| {
                el.child(self.multi_properties(cx))
            })
            .when(!selected && self.multi_selection.is_empty(), |el| {
                el.child(
                    div()
                        .pt(px(48.))
                        .px(px(20.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(12.))
                        .child(icon(LucideIcons::SlidersHorizontal, 24.).text_color(rgb(MUTED)))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(MUTED))
                                .child(t("inspector-empty")),
                        ),
                )
            })
            .when(
                !self.selection_ids().is_empty()
                    || self.export.busy
                    || self.export.status.is_some(),
                |el| el.child(self.export_controls(cx)),
            )
            .child(self.panel_resize_handle(panels::Side::Right, cx))
    }
    pub(super) fn geometry_controls(&self, cx: &mut Context<Self>) -> Div {
        let line = self.selected_shape().is_some_and(|s| s.kind.is_line());
        inspector_section(if line { t("endpoints") } else { t("layout") })
            .gap(px(8.))
            .py(px(12.))
            .child(self.alignment_controls(cx))
            .when(line, |el| el.child(property_caption(t("start-point"))))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(self.property_field(1, "X", cx))
                    .child(self.property_field(2, "Y", cx)),
            )
            .when(line, |el| el.child(property_caption(t("end-point"))))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(self.property_field(3, if line { "X" } else { t("width") }, cx))
                    .child(self.property_field(4, if line { "Y" } else { t("height") }, cx))
                    .when(!line, |el| el.child(self.aspect_button(cx))),
            )
            .when(
                self.selected_shape.is_some() || self.selected_text.is_some(),
                |el| {
                    el.child(
                        div()
                            .flex()
                            .child(self.property_field(15, t("rotation"), cx))
                            .child(div().flex_1().min_w_0()),
                    )
                },
            )
    }
    pub(super) fn color_picker(&self) -> impl IntoElement {
        ColorPicker::new(&self.picker)
            .horizontal_hue(true)
            .gap(px(10.))
            .p_0()
            .border_0()
            .bg(gpui::rgba(0))
            .appearance(ColorPickerAppearance {
                area_height: px(152.),
                hue_width: px(14.),
                marker_size: px(12.),
                accent: rgb(ACCENT).into(),
                ..Default::default()
            })
    }
    fn color_panel(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        layers::glass_surface()
            .id("color-panel")
            .debug_selector(|| "color-panel".into())
            .w(px(336.))
            .max_h(window.viewport_size().height - px(32.))
            .overflow_y_scroll()
            .p(px(14.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow_lg()
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .when(!self.multi_selection.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(if self.selected_shape.is_some() && self.stroke_editing {
                            t("stroke")
                        } else {
                            t("fill")
                        })
                        .child(
                            icon_button("color-close", t("close"), LucideIcons::X, false).on_click(
                                cx.listener(|this, _, window, cx| {
                                    for popover in &this.paint_popovers {
                                        popover.update(cx, |state, cx| state.close(window, cx));
                                    }
                                }),
                            ),
                        ),
                )
            })
            .when(!self.multi_selection.is_empty(), |el| {
                el.child(self.color_picker())
            })
            .when(self.multi_selection.is_empty(), |el| {
                el.child(self.fill_controls(cx))
            })
    }
}

pub(super) fn inspector_section(title: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .px(px(14.))
        .py(px(14.))
        .border_b_1()
        .border_color(rgb(BORDER))
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT))
                .line_height(px(16.))
                .child(title),
        )
}

pub(super) fn property_caption(label: &'static str) -> Div {
    div().text_size(px(10.)).text_color(rgb(MUTED)).child(label)
}

pub(super) fn hex(color: gpui::Rgba) -> String {
    format!(
        "{:02X}{:02X}{:02X}",
        (color.r * 255.).round() as u8,
        (color.g * 255.).round() as u8,
        (color.b * 255.).round() as u8
    )
}

fn apply_shape_field(
    shape: &mut Shape,
    stroke: bool,
    stop: usize,
    index: usize,
    text: &str,
) -> bool {
    if index == 0 {
        shape.name = text.to_owned();
        return true;
    }
    if index == 5 {
        let hex = text.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
        let Ok(value) = u32::from_str_radix(hex, 16) else {
            return false;
        };
        let Some(target) = shape.paint_color_mut(stop, stroke) else {
            return false;
        };
        let alpha = target.a;
        *target = rgb(value);
        target.a = alpha;
        return true;
    }
    let Ok(value) = text.trim().parse::<f32>() else {
        return false;
    };
    if !value.is_finite() {
        return false;
    }
    if shape.kind.is_line() && (1..=4).contains(&index) {
        if value.abs() > 1_000_000. {
            return false;
        }
        let end = usize::from(index >= 3);
        let pivot = crate::rotation::center(shape.rect);
        let mut p = shape.display_path_point(end);
        if index % 2 == 1 {
            p.x = value;
        } else {
            p.y = value;
        }
        shape.set_endpoint(
            end,
            crate::rotation::around(p, pivot, -shape.layer.rotation),
        );
        shape.preserve_rotation_pivot(pivot);
        return true;
    }
    match index {
        1 if value.abs() <= 1_000_000. => shape.rect.x = value,
        2 if value.abs() <= 1_000_000. => shape.rect.y = value,
        3 | 4 => {
            return editing::set_dimension(
                &mut shape.rect,
                index,
                value,
                shape.layer.aspect_locked,
            );
        }
        6 if !stroke && shape.fill_mode == FillMode::Image && (0. ..=100.).contains(&value) => {
            shape.image_fill.opacity = value / 100.
        }
        6 if (0. ..=100.).contains(&value) => {
            let Some(target) = shape.paint_color_mut(stop, stroke) else {
                return false;
            };
            target.a = value / 100.;
        }
        7 if (0. ..=360.).contains(&value) => shape.paint_gradient_mut(stroke).angle = value,
        8 if (0. ..=100.).contains(&value) => {
            return shape
                .paint_gradient_mut(stroke)
                .set_position(stop, value / 100.);
        }
        9 if shape.kind.supports_corners() && (0. ..=MAX_SIZE / 2.).contains(&value) => {
            shape.radius = value
        }
        9 if shape.kind.is_polygon() && (3. ..=60.).contains(&value) && value.fract() == 0. => {
            shape.vertices = value as usize
        }
        10 if shape.kind == ShapeKind::Star && (1. ..=99.).contains(&value) => {
            shape.inner_radius = value / 100.
        }
        10..=13 if shape.kind.supports_corners() && (0. ..=MAX_SIZE / 2.).contains(&value) => {
            shape.corners.get_or_insert([shape.radius; 4])[index - 10] = value;
        }
        14 if (0. ..=MAX_SIZE / 2.).contains(&value) => shape.stroke.width = value,
        _ => return false,
    }
    true
}

fn apply_text_field(
    text: &mut TextBox,
    index: usize,
    value: &str,
    stop: usize,
    cx: &mut Context<Workspace>,
) -> bool {
    if index == 5 {
        let hex = value.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
        let Ok(color) = u32::from_str_radix(hex, 16) else {
            return false;
        };
        text.editor.update(cx, |editor, cx| {
            editor.apply_color(rgb(color), stop, false, cx)
        });
        return true;
    }
    let Ok(value) = value.trim().parse::<f32>() else {
        return false;
    };
    if !value.is_finite() {
        return false;
    }
    match index {
        1 if value.abs() <= 1_000_000. => text.rect.x = value,
        2 if value.abs() <= 1_000_000. => text.rect.y = value,
        3 | 4 => {
            return editing::set_dimension(&mut text.rect, index, value, text.layer.aspect_locked);
        }
        6 if (0. ..=100.).contains(&value) => text.editor.update(cx, |editor, cx| {
            let mut color = editor.effective_style().editable_color(stop);
            color.a = value / 100.;
            editor.apply_color(color, stop, true, cx);
        }),
        7 if (1. ..=1000.).contains(&value) => text
            .editor
            .update(cx, |e, cx| e.apply_style(StyleChange::Size(value), cx)),
        8 if (0.5..=5.).contains(&value) => text.editor.update(cx, |e, cx| {
            e.apply_style(StyleChange::LineHeight(value), cx)
        }),
        9 if (0. ..=200.).contains(&value) => text
            .editor
            .update(cx, |e, cx| e.apply_style(StyleChange::Spacing(value), cx)),
        10 if (100. ..=900.).contains(&value) && value % 100. == 0. => text
            .editor
            .update(cx, |e, cx| e.apply_style(StyleChange::Weight(value), cx)),
        12 if (0. ..=360.).contains(&value) => text.editor.update(cx, |e, cx| {
            let mut gradient = e.effective_style().gradient.clone();
            gradient.angle = value;
            e.apply_style(StyleChange::Gradient(gradient), cx);
        }),
        13 if (0. ..=100.).contains(&value) => text.editor.update(cx, |e, cx| {
            let mut gradient = e.effective_style().gradient.clone();
            gradient.set_position(stop, value / 100.);
            e.apply_style(StyleChange::Gradient(gradient), cx);
        }),
        _ => return false,
    }
    true
}

pub(super) fn icon_button(
    id: &'static str,
    label: &'static str,
    glyph: LucideIcons,
    active: bool,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .size(px(30.))
        .flex_shrink_0()
        .rounded(px(5.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(active, |el| el.bg(rgb(0x353044)))
        .hover(|s| s.bg(rgb(BORDER)))
        .tooltip(move |_, cx| cx.new(|_| toolbar::ToolTip(label.into())).into())
        .child(icon(glyph, 17.).text_color(rgb(if active { ACCENT } else { MUTED })))
}

fn gradient_button(
    id: &'static str,
    label: &'static str,
    glyph: LucideIcons,
    enabled: bool,
) -> gpui::Stateful<Div> {
    icon_button(id, label, glyph, false).when(!enabled, |el| el.opacity(0.3))
}

pub(super) fn number(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn apply_field(board: &mut Artboard, stop: usize, index: usize, text: &str) -> bool {
    if index == 0 {
        board.name = text.to_owned();
        return true;
    }
    if index == 5 {
        let hex = text.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
        let Ok(value) = u32::from_str_radix(hex, 16) else {
            return false;
        };
        let Some(target) = board.editable_color_mut(stop) else {
            return false;
        };
        let alpha = target.a;
        *target = rgb(value);
        target.a = alpha;
        return true;
    }
    let Ok(value) = text.trim().parse::<f32>() else {
        return false;
    };
    if !value.is_finite() {
        return false;
    }
    match index {
        1 | 2 if value.abs() <= 1_000_000. => {
            if index == 1 {
                board.rect.x = value;
            } else {
                board.rect.y = value;
            }
        }
        3 | 4 if (MIN_SIZE..=MAX_SIZE).contains(&value) => {
            return editing::set_dimension(
                &mut board.rect,
                index,
                value,
                board.layer.aspect_locked,
            );
        }
        6 if board.fill_mode == FillMode::Image && (0. ..=100.).contains(&value) => {
            board.image_fill.opacity = value / 100.
        }
        6 if (0. ..=100.).contains(&value) => {
            let Some(target) = board.editable_color_mut(stop) else {
                return false;
            };
            target.a = value / 100.;
        }
        7 if (0. ..=360.).contains(&value) => board.gradient.angle = value,
        8 if (0. ..=100.).contains(&value) => {
            return board.gradient.set_position(stop, value / 100.);
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_properties_preserve_geometry_and_hex_preserves_alpha() {
        let mut board = Artboard {
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
            assert!(!apply_field(&mut board, 0, 3, value));
            assert_eq!(board.rect, original);
        }
        assert!(apply_field(&mut board, 0, 6, "25"));
        assert!(apply_field(&mut board, 0, 5, "#345678"));
        assert_eq!(board.color.a, 0.25);
        let color = board.color;
        assert!(!apply_field(&mut board, 0, 5, "GGGGGG"));
        assert_eq!(board.color, color);
    }
}
