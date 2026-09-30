use super::*;
use uic::components::{
    color_picker::{AlphaSlider, ColorPicker, ColorPickerAppearance},
    dropdown::dropdown,
    input::{Input, InputAppearance},
};

impl Workspace {
    pub(in crate::workspace) fn color_assets(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let scope = self.assets.scope;
        let query = self.assets.search.read(cx).value().trim().to_lowercase();
        let palette = self.color_palette(scope, cx);
        let available = scope == Scope::Document
            || self
                .assets
                .library
                .as_ref()
                .is_some_and(|l| l.read(cx).ready && !l.read(cx).busy);
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(t("color-styles")),
                    )
                    .child(
                        button("add-color-style", t("color-style-add"))
                            .debug_selector(|| "add-color-style".into())
                            .text_color(rgb(ACCENT))
                            .opacity(if available { 1. } else { 0.4 })
                            .when(available, |el| {
                                el.on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_color_dialog(scope, None, false, window, cx);
                                }))
                            }),
                    ),
            )
            .child(
                div()
                    .id("asset-colors-list")
                    .track_scroll(&self.colors.scroll)
                    .max_h(px(180.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .when(palette.is_empty(), |el| {
                        el.child(
                            div()
                                .py(px(8.))
                                .text_size(px(11.))
                                .text_color(rgb(MUTED))
                                .child(t("color-styles-empty")),
                        )
                    })
                    .children(
                        palette
                            .into_iter()
                            .filter(|(_, style)| style.name.to_lowercase().contains(&query))
                            .map(|(id, style)| self.color_asset_row(scope, id, style, cx)),
                    ),
            )
    }

    fn color_asset_row(
        &self,
        scope: Scope,
        id: String,
        style: ColorStyle,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let apply_id = id.clone();
        let edit_id = id.clone();
        let add = style.clone();
        let stroke = self.selected_shape().is_some_and(|shape| !shape.can_fill());
        let can_apply = self.color_source(stroke, cx).is_some();
        let selected = self
            .colors
            .selected
            .as_ref()
            .is_some_and(|(s, selected)| *s == scope && selected == &id);
        div()
            .id(gpui::SharedString::from(format!("color-asset-{id}")))
            .h(px(34.))
            .px(px(6.))
            .rounded(px(7.))
            .when(selected, |el| el.bg(rgb(0x302a40)))
            .flex()
            .items_center()
            .gap(px(8.))
            .hover(|s| s.bg(rgb(0x292632)))
            .child(
                div()
                    .id(gpui::SharedString::from(format!("apply-color-{id}")))
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .child(paint_swatch(style.background(), 22.))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.))
                            .child(style.name),
                    )
                    .when(selected, |el| {
                        el.child(icon(LucideIcons::Check, 12.).text_color(rgb(ACCENT)))
                    })
                    .when(can_apply, |el| {
                        el.cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_color_style(scope, &apply_id, stroke, window, cx);
                            }))
                    }),
            )
            .when(scope == Scope::Local, |el| {
                el.child(
                    button(gpui::SharedString::from(format!("import-color-{id}")), "+")
                        .text_color(rgb(MUTED))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_document_color(
                                uuid::Uuid::new_v4().to_string(),
                                Some(add.clone()),
                                window,
                                cx,
                            );
                            this.assets.scope = Scope::Document;
                        })),
                )
            })
            .child(
                div()
                    .id(gpui::SharedString::from(format!("edit-color-{id}")))
                    .size(px(24.))
                    .rounded(px(5.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_color(rgb(MUTED))
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .child(icon(LucideIcons::Pencil, 13.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_color_dialog(scope, Some(edit_id.clone()), false, window, cx);
                    })),
            )
    }

    pub(in crate::workspace) fn color_style_control(
        &self,
        stroke: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let source = self.color_source(stroke, cx);
        let linked = source
            .as_ref()
            .and_then(|s| s.1.as_ref())
            .and_then(|id| self.colors.palette.get(id));
        let scope = self.colors.scope;
        let palette = self.color_palette(scope, cx);
        let can_save = scope == Scope::Document
            || self.assets.library.as_ref().is_some_and(|library| {
                let library = library.read(cx);
                library.ready && !library.busy
            });
        let trigger_id = if stroke {
            "stroke-color-style"
        } else {
            "fill-color-style"
        };
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                dropdown(&self.colors.menu[usize::from(stroke)])
                    .priority(1100)
                    .w(px(260.))
                    .p(px(6.))
                    .rounded(px(10.))
                    .bg(rgb(0x23212c))
                    .border_color(rgb(BORDER))
                    .shadow_lg()
                    .trigger(
                        div()
                            .id(trigger_id)
                            .debug_selector(move || trigger_id.into())
                            .h(px(26.))
                            .px(px(7.))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .cursor_pointer()
                            .text_size(px(11.))
                            .text_color(rgb(if linked.is_some() { ACCENT } else { MUTED }))
                            .hover(|s| s.bg(rgb(0x302b3d)))
                            .child(icon(LucideIcons::Palette, 13.))
                            .child(div().max_w(px(150.)).truncate().child(
                                linked.map_or_else(
                                    || t("color-styles").to_owned(),
                                    |s| s.name.clone(),
                                ),
                            )),
                    )
                    .menu(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div().flex().gap(px(3.)).children(
                                    [
                                        (Scope::Document, "assets-document"),
                                        (Scope::Local, "assets-local"),
                                    ]
                                    .map(|(choice, key)| {
                                        button(key, t(key))
                                            .flex_1()
                                            .when(choice == scope, |el| {
                                                el.bg(rgb(0x393047)).text_color(rgb(ACCENT))
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.colors.scope = choice;
                                                cx.notify();
                                            }))
                                    }),
                                ),
                            )
                            .child(
                                div()
                                    .id(if stroke {
                                        "stroke-style-list"
                                    } else {
                                        "fill-style-list"
                                    })
                                    .max_h(px(230.))
                                    .overflow_y_scroll()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .when(palette.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .p(px(12.))
                                                .text_size(px(11.))
                                                .text_color(rgb(MUTED))
                                                .child(t("color-styles-empty")),
                                        )
                                    })
                                    .children(palette.into_iter().map(|(id, style)| {
                                        div()
                                            .id(gpui::SharedString::from(format!(
                                                "pick-color-{id}"
                                            )))
                                            .h(px(34.))
                                            .px(px(8.))
                                            .rounded(px(6.))
                                            .flex()
                                            .items_center()
                                            .gap(px(9.))
                                            .text_size(px(12.))
                                            .text_color(rgb(TEXT))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(0x34303f)))
                                            .child(paint_swatch(style.background(), 20.))
                                            .child(div().flex_1().truncate().child(style.name))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.apply_color_style(
                                                    scope, &id, stroke, window, cx,
                                                );
                                            }))
                                    })),
                            )
                            .child(
                                button(
                                    if stroke {
                                        "save-stroke-style"
                                    } else {
                                        "save-fill-style"
                                    },
                                    t("color-style-create"),
                                )
                                .text_color(rgb(ACCENT))
                                .opacity(if can_save { 1. } else { 0.4 })
                                .when(can_save, |el| {
                                    el.on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_color_dialog(scope, None, stroke, window, cx);
                                    }))
                                }),
                            ),
                    ),
            )
            .when(linked.is_some(), |el| {
                el.child(
                    div()
                        .id(if stroke {
                            "detach-stroke-style"
                        } else {
                            "detach-fill-style"
                        })
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .text_color(rgb(MUTED))
                        .hover(|s| s.text_color(rgb(TEXT)))
                        .child(icon(LucideIcons::Unlink, 13.))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.detach_color_style(stroke, cx)),
                        ),
                )
            })
    }

    fn gradient_style_controls(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let dialog = self.colors.dialog.as_ref().unwrap();
        let gradient = dialog.gradient.as_ref().unwrap();
        let mut ramp = gradient.clone();
        ramp.kind = gpui::GradientKind::Linear;
        ramp.angle = 90.;
        let bounds = self.colors.ramp_bounds.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div().flex().gap(px(3.)).children(
                    [
                        (gpui::GradientKind::Linear, "gradient-linear"),
                        (gpui::GradientKind::Radial, "gradient-radial"),
                        (gpui::GradientKind::Angular, "gradient-angular"),
                        (gpui::GradientKind::Diamond, "gradient-diamond"),
                    ]
                    .map(|(kind, key)| {
                        button(key, t(key))
                            .flex_1()
                            .text_color(rgb(if gradient.kind == kind { ACCENT } else { MUTED }))
                            .when(gradient.kind == kind, |el| el.bg(rgb(0x2e293b)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(gradient) = this
                                    .colors
                                    .dialog
                                    .as_mut()
                                    .and_then(|d| d.gradient.as_mut())
                                {
                                    gradient.kind = kind;
                                }
                                cx.notify();
                            }))
                    }),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .mx(px(9.))
                            .relative()
                            .h(px(62.))
                            .on_paint_before_children(move |rect, _, _, _| bounds.set(rect))
                            .id("style-gradient-track")
                            .debug_selector(|| "style-gradient-track".into())
                            .child(self.gradient_midpoints(true, gradient, cx))
                            .child(
                                div()
                                    .id("style-gradient-ramp")
                                    .debug_selector(|| "style-gradient-ramp".into())
                                    .h(px(22.))
                                    .w_full()
                                    .rounded(px(5.))
                                    .bg(gpui::checkerboard(rgb(0x50515b), 5.))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event, window, cx| {
                                            this.begin_style_stop(None, event, window, cx)
                                        }),
                                    )
                                    .child(div().size_full().rounded(px(5.)).bg(ramp.background())),
                            )
                            .children(gradient.stops().iter().map(|stop| {
                                let id = stop.id;
                                div()
                                    .id(("style-stop", id))
                                    .debug_selector(move || format!("style-stop-{id}"))
                                    .absolute()
                                    .left(gpui::relative(stop.position))
                                    .ml(px(-9.))
                                    .top(px(36.))
                                    .w(px(18.))
                                    .h(px(24.))
                                    .rounded(px(5.))
                                    .bg(rgb(0x1d1e25))
                                    .border_1()
                                    .border_color(rgb(if id == dialog.active_stop {
                                        ACCENT
                                    } else {
                                        0x3a3b45
                                    }))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor(gpui::CursorStyle::ResizeLeftRight)
                                    .child(swatch(stop.color, 12.))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, event, window, cx| {
                                            this.begin_style_stop(Some(id), event, window, cx)
                                        }),
                                    )
                            })),
                    )
                    .child(
                        button("add-style-stop", "+")
                            .debug_selector(|| "add-style-stop".into())
                            .opacity(if gradient.stops().len() < 4 { 1. } else { 0.3 })
                            .when(gradient.stops().len() < 4, |el| {
                                el.on_click(
                                    cx.listener(|this, _, _, cx| this.change_style_stops(true, cx)),
                                )
                            })
                            .when(gradient.stops().len() >= 4, |el| el.cursor_default()),
                    )
                    .child(
                        button("remove-style-stop", "−")
                            .opacity(if gradient.stops().len() > 2 { 1. } else { 0.3 })
                            .when(gradient.stops().len() > 2, |el| {
                                el.on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.change_style_stops(false, cx)
                                    }),
                                )
                            })
                            .when(gradient.stops().len() <= 2, |el| el.cursor_default()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(self.style_number_input(true, cx))
                    .child(self.style_number_input(false, cx)),
            )
            .when(gradient.kind == gpui::GradientKind::Angular, |el| {
                el.child(self.gradient_seam_control(true, cx))
            })
    }

    fn style_number_input(&self, angle: bool, cx: &mut Context<Self>) -> Div {
        let input = if angle {
            &self.colors.angle
        } else {
            &self.colors.position
        };
        let selector = if angle {
            "style-angle-drag"
        } else {
            "style-position-drag"
        };
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(5.))
            .child(label(t(if angle { "angle" } else { "stop-position" })))
            .child(
                div()
                    .h(px(32.))
                    .rounded(px(6.))
                    .bg(rgb(0x14151b))
                    .border_1()
                    .border_color(rgb(0x34353e))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .id(selector)
                            .debug_selector(move || selector.into())
                            .h_full()
                            .w(px(28.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .cursor(gpui::CursorStyle::ResizeLeftRight)
                            .hover(|s| s.text_color(rgb(TEXT)))
                            .child(if angle { "∠" } else { "%" })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event, window, cx| {
                                    this.begin_style_scrub(angle, event, window, cx);
                                }),
                            ),
                    )
                    .child(
                        style_input(input)
                            .bg(gpui::rgba(0))
                            .border_0()
                            .h(px(30.))
                            .px(px(4.)),
                    ),
            )
    }

    pub(in crate::workspace) fn color_style_dialog(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let dialog = self.colors.dialog.as_ref().unwrap();
        let color = parse_color(&self.colors.value.read(cx).value()).unwrap_or(rgb(0x303039));
        let preview = dialog
            .gradient
            .as_ref()
            .map_or_else(|| color.into(), |g| g.background());
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::rgba(0x00000070))
            .flex()
            .items_start()
            .pt(px(24.))
            .justify_center()
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("color-style-dialog")
                    .debug_selector(|| "color-style-dialog".into())
                    .w(px(360.))
                    .max_h(window.viewport_size().height - px(48.))
                    .overflow_y_scroll()
                    .p(px(20.))
                    .rounded(px(14.))
                    .bg(rgb(0x1d1e25))
                    .text_color(rgb(TEXT))
                    .border_1()
                    .border_color(rgb(0x393342))
                    .shadow_xl()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(paint_swatch(preview, 32.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.))
                                    .child(div().text_size(px(14.)).child(t("color-style-editor")))
                                    .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(
                                        t(if dialog.scope == Scope::Document {
                                            "assets-document"
                                        } else {
                                            "assets-local"
                                        }),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(label(t("name")))
                            .child(style_input(&self.colors.name)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .rounded(px(7.))
                            .p(px(3.))
                            .bg(rgb(0x14151b))
                            .flex_row()
                            .children([(false, "solid"), (true, "gradient")].map(
                                |(enabled, key)| {
                                    button(key, t(key))
                                        .debug_selector(move || key.into())
                                        .flex_1()
                                        .when(enabled == dialog.gradient.is_some(), |el| {
                                            el.bg(rgb(0x363043)).text_color(rgb(ACCENT))
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.set_style_gradient(enabled, cx)
                                        }))
                                },
                            )),
                    )
                    .when(dialog.gradient.is_some(), |el| {
                        el.child(self.gradient_style_controls(cx))
                    })
                    .child(
                        ColorPicker::new(&self.colors.picker)
                            .rounded(px(6.))
                            .overflow_hidden()
                            .horizontal_hue(true)
                            .gap(px(10.))
                            .p_0()
                            .border_0()
                            .bg(gpui::rgba(0))
                            .appearance(ColorPickerAppearance {
                                area_height: px(132.),
                                hue_width: px(10.),
                                marker_size: px(10.),
                                accent: rgb(ACCENT).into(),
                                ..Default::default()
                            }),
                    )
                    .child(AlphaSlider::new(&self.colors.picker))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(label("HEX"))
                            .child(style_input(&self.colors.value)),
                    )
                    .when_some(self.colors.error.clone(), |el, error| {
                        el.child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(0xea9894))
                                .child(error),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .pt(px(12.))
                            .border_t_1()
                            .border_color(rgb(0x30313a))
                            .items_center()
                            .gap(px(8.))
                            .when(dialog.existing, |el| {
                                el.child(
                                    button("delete-color-style", t("delete"))
                                        .text_color(rgb(0xe6a29e))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.delete_color_dialog(window, cx)
                                        })),
                                )
                            })
                            .child(div().flex_1())
                            .child(
                                button("cancel-color-style", t("cancel")).on_click(cx.listener(
                                    |this, _, window, cx| this.close_color_dialog(window, cx),
                                )),
                            )
                            .child(
                                button("save-color-style", t("color-style-save"))
                                    .debug_selector(|| "save-color-style".into())
                                    .bg(rgb(ACCENT))
                                    .text_color(rgb(0x20172f))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.save_color_dialog(window, cx)
                                    })),
                            ),
                    ),
            )
    }
}

fn button(id: impl Into<gpui::ElementId>, text: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .h(px(28.))
        .px(px(8.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.))
        .cursor_pointer()
        .hover(|s| s.opacity(0.8))
        .child(text)
}

fn label(text: &'static str) -> Div {
    div().text_size(px(11.)).text_color(rgb(MUTED)).child(text)
}

fn swatch(color: gpui::Rgba, size: f32) -> Div {
    paint_swatch(color.into(), size)
}

fn paint_swatch(background: gpui::Background, size: f32) -> Div {
    div()
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(5.))
        .overflow_hidden()
        .bg(gpui::checkerboard(rgb(0xc7c7ce), 6.))
        .child(div().size_full().rounded(px(5.)).bg(background))
}

fn style_input(state: &Entity<TextInput>) -> Input {
    Input::new(state)
        .w_full()
        .h(px(32.))
        .px(px(10.))
        .rounded(px(6.))
        .text_size(px(12.))
        .text_color(rgb(TEXT))
        .bg(rgb(0x14151b))
        .border_color(rgb(0x34353e))
        .appearance(InputAppearance {
            focus_border: rgb(ACCENT).into(),
            caret: rgb(ACCENT).into(),
            selection: gpui::rgba(0xb4a2ee44).into(),
            caret_height: px(16.),
            ..Default::default()
        })
}
