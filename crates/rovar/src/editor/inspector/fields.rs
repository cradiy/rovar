//! Reusable inspector input controls.

use super::*;
use crate::ui::theme::Color;

impl Workspace {
    pub(in crate::editor) fn property_field(
        &self,
        index: usize,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        self.property_field_impl(index, label, true, cx)
    }
    pub(super) fn paint_input_field(
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
    ) -> Div {
        let is_color = matches!(index, 5 | 16);
        let sizing =
            if matches!(index, 3 | 4) && !self.selected_shape().is_some_and(|s| s.kind.is_line()) {
                self.sizing_control(index - 3, cx)
            } else {
                None
            };
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
            && self.inspector.paint_popovers[usize::from(index >= 16)]
                .read(cx)
                .is_open();
        let readonly = image_color || (matches!(index, 3 | 4) && self.boolean_result_empty());
        let numeric = !matches!(index, 0 | 5 | 16);
        let draggable = if !self.multi_selection.is_empty() && index != 18 {
            self.field_property(index)
                .is_some_and(|property| self.multi_can_scrub(property, cx))
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
            self.multi_field_value(Property::Color, cx)
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
                    self.inspector.picker.read(cx).value().into()
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
            .bg(Color::Input.color())
            .border_1()
            .border_color(if self.inspector.invalid[slot] {
                Color::Danger.color()
            } else {
                Color::Hover.color()
            })
            .when(!popup, |el| {
                el.bg(Color::Transparent.color())
                    .border_color(Color::Transparent.color())
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
                    .text_color(MUTED.color())
                    .when(draggable, |el| {
                        el.cursor(gpui::CursorStyle::ResizeLeftRight)
                            .hover(|s| s.text_color(ACCENT.color()))
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
                                .bg(gpui::checkerboard(Color::Checker.color(), 4.))
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
                        Some(glyph) => el.child(icon(glyph, 15.).text_color(MUTED.color())),
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
                    Popover::new(&self.inspector.paint_popovers[usize::from(index == 16)])
                        .label(t("color"))
                        .placement(PopoverPlacement::LeftStart)
                        .gap(px(20.))
                        .p_0()
                        .border_0()
                        .bg(Color::Transparent.color())
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
                    .when(index != 0 && readonly, |el| {
                        el.child(
                            div()
                                .text_size(px(12.))
                                .overflow_hidden()
                                .child(self.field_value(index, cx).unwrap_or_default()),
                        )
                    })
                    .when(index != 0 && !readonly, |el| {
                        el.child(
                            inspector_input(&self.inspector.fields[slot])
                                .w_full()
                                .h(px(30.))
                                .px(px(4.))
                                .rounded(px(4.))
                                .bg(Color::Input.color())
                                .border_color(Color::Input.color())
                                .when(!popup, |el| {
                                    el.bg(Color::Transparent.color())
                                        .border_color(Color::Transparent.color())
                                }),
                        )
                    }),
            )
            .children(sizing)
    }

    pub(in crate::editor) fn paint_value_row(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        self.paint_value_row_for(false, cx)
    }
    pub(in crate::editor) fn paint_value_row_for(
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
            .bg(Color::Input.color())
            .child(self.property_field(if stroke { 16 } else { 5 }, t("color-hex"), cx))
            .child(
                div()
                    .w(px(1.))
                    .h(px(20.))
                    .flex_shrink_0()
                    .bg(BORDER.color()),
            )
            .child(div().w(px(76.)).flex_shrink_0().child(self.property_field(
                if stroke { 17 } else { 6 },
                t("opacity"),
                cx,
            )))
    }
}
