//! Fill and gradient editor presentation and mutations.

use super::*;

impl Workspace {
    pub(in crate::editor) fn field_edits_stroke(&self, index: usize) -> bool {
        match index {
            14 | 16 | 17 => true,
            5 | 6 => false,
            _ => self.inspector.stroke_editing,
        }
    }
    pub(in crate::editor) fn shape_paint_stop(&self, stroke: bool) -> usize {
        let id = if stroke == self.inspector.stroke_editing {
            self.inspector.active_stop
        } else {
            self.inspector.paint_stops[stroke as usize]
        };
        let gradient = self.selected_shape().unwrap().paint_gradient(stroke);
        if gradient.stop(id).is_some() {
            id
        } else {
            gradient.stops()[0].id
        }
    }
    pub(in crate::editor) fn fill_state(
        &self,
        cx: &gpui::App,
    ) -> Option<(FillMode, LinearGradient)> {
        if let Some(text) = self.selected_text() {
            let editor = text.editor.read(cx);
            let style = editor.effective_style();
            Some((style.fill_mode, style.gradient.clone()))
        } else if let Some(shape) = self.selected_shape() {
            Some((
                shape.paint_mode(self.inspector.stroke_editing),
                shape.paint_gradient(self.inspector.stroke_editing).clone(),
            ))
        } else {
            self.selected_board()
                .map(|board| (board.fill_mode, board.gradient.clone()))
        }
    }
    pub(in crate::editor) fn mutate_gradient<R>(
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
            let stroke = self.inspector.stroke_editing;
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
            dropdown(&self.inspector.gradient_menu)
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
                                    this.inspector
                                        .gradient_menu
                                        .update(cx, |state, cx| state.close(window, cx));
                                    cx.notify();
                                }))
                        })),
                ),
        )
    }

    pub(in crate::editor) fn fill_controls(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some((mode, gradient)) = self.fill_state(cx) else {
            return div();
        };
        let text_selected = self.selected_text.is_some();
        let color_index = if self.selected_shape.is_some() && self.inspector.stroke_editing {
            16
        } else {
            5
        };
        let alpha_index = if self.selected_shape.is_some() && self.inspector.stroke_editing {
            17
        } else {
            6
        };
        let stop_count = gradient.stops().len();
        // Reserve room for the picker, controls, and the panel's viewport margin.
        let reserved_height = if gradient.kind == gpui::GradientKind::Angular {
            520.
        } else {
            482.
        };
        let stop_list_height =
            (window.viewport_size().height - px(reserved_height)).clamp(px(38.), px(374.));
        let image_allowed = !text_selected
            && (self.selected_shape.is_none() || !self.inspector.stroke_editing)
            && self.current_image_fill().is_some()
            && self.selected_shape().is_none_or(|s| s.can_fill());
        let mixed_mode = self
            .selected_text()
            .is_some_and(|t| t.editor.read(cx).mixed(TextProperty::FillMode));
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
                                            this.selected_shape.is_some()
                                                && this.inspector.stroke_editing,
                                        );
                                        this.inspector.paint_popovers[surface]
                                            .focus_handle(cx)
                                            .focus(window, cx);
                                        this.history.borrow_mut().break_group();
                                        if this.selected_text.is_some() {
                                            this.change_text_style(StyleChange::FillMode(kind), cx);
                                        } else if this.selected_shape.is_some() {
                                            let stroke = this.inspector.stroke_editing;
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
                                for popover in &this.inspector.paint_popovers {
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
                        .flex()
                        .child(self.gradient_track(false, &gradient, cx)),
                )
                .when(gradient.kind == gpui::GradientKind::Angular, |el| {
                    el.child(self.gradient_seam_control(false, cx))
                })
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
                            gradient_button("gradient-add", t("stop-add"), LucideIcons::Plus, true)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(Some(id)) =
                                        this.mutate_gradient(|g| g.add_stop(), cx)
                                    {
                                        this.inspector.active_stop = id;
                                        this.sync_fields(cx);
                                        this.reveal_gradient_stop(cx);
                                        cx.notify();
                                    }
                                })),
                        ),
                )
                .child(
                    div()
                        .id("gradient-stop-list")
                        .debug_selector(|| "gradient-stop-list".into())
                        .track_scroll(&self.inspector.gradient_stop_scroll)
                        .max_h(stop_list_height)
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .children(gradient.stops().iter().map(|stop| {
                            let id = stop.id;
                            let active = id == self.inspector.active_stop;
                            div()
                                .id(("gradient-stop", id))
                                .debug_selector(move || format!("gradient-stop-{id}"))
                                .h(px(38.))
                                .flex_shrink_0()
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
                                        if this.inspector.active_stop != id {
                                            this.history.borrow_mut().break_group();
                                            this.inspector.active_stop = id;
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
                        uic::components::color_picker::AlphaSlider::new(
                            &self.inspector.alpha_picker,
                        )
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

    pub(in crate::editor) fn color_picker(&self) -> impl IntoElement {
        ColorPicker::new(&self.inspector.picker)
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
    pub(super) fn color_panel(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
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
                        .child(
                            if self.selected_shape.is_some() && self.inspector.stroke_editing {
                                t("stroke")
                            } else {
                                t("fill")
                            },
                        )
                        .child(
                            icon_button("color-close", t("close"), LucideIcons::X, false).on_click(
                                cx.listener(|this, _, window, cx| {
                                    for popover in &this.inspector.paint_popovers {
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
                el.child(self.fill_controls(window, cx))
            })
    }
}

fn gradient_button(
    id: &'static str,
    label: &'static str,
    glyph: LucideIcons,
    enabled: bool,
) -> gpui::Stateful<Div> {
    icon_button(id, label, glyph, false).when(!enabled, |el| el.opacity(0.3))
}
