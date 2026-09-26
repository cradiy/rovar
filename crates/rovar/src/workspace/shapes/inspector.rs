use super::*;
use crate::i18n::t;
use crate::shape::StrokeAlign;
use crate::workspace::inspector::icon_button;

impl Workspace {
    pub(in crate::workspace) fn shape_field_visible(&self, index: usize) -> bool {
        let Some(shape) = self.selected_shape() else {
            return false;
        };
        match index {
            0..=4 | 15 => true,
            5 | 6 => shape.fill_enabled,
            16 | 17 => shape.stroke.enabled,
            7 | 8 => shape.paint_mode(self.stroke_editing) == FillMode::Linear,
            9 => {
                shape.kind.is_polygon()
                    || (shape.kind.supports_corners() && !shape.independent_corners)
            }
            10 if shape.kind == ShapeKind::Star => true,
            10..=13 => shape.kind.supports_corners() && shape.independent_corners,
            14 => shape.stroke.enabled,
            _ => false,
        }
    }

    pub(in crate::workspace) fn shape_corner_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let independent = self.selected_shape().unwrap().independent_corners;
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .when(!independent, |el| {
                        el.child(self.property_field(9, t("corner-radius"), cx))
                    })
                    .when(independent, |el| {
                        el.child(
                            div()
                                .flex_1()
                                .text_size(px(11.))
                                .text_color(rgb(MUTED))
                                .child(t("corner-radius")),
                        )
                    })
                    .children(
                        [
                            (
                                false,
                                "corners-unified",
                                t("corners-unified"),
                                LucideIcons::Radius,
                            ),
                            (
                                true,
                                "corners-independent",
                                t("corners-independent"),
                                LucideIcons::Scan,
                            ),
                        ]
                        .into_iter()
                        .map(|(value, id, label, glyph)| {
                            icon_button(id, label, glyph, independent == value).on_click(
                                cx.listener(move |this, _, window, cx| {
                                    this.history.borrow_mut().break_group();
                                    this.edit_shape(|shape| shape.set_independent_corners(value));
                                    this.focus.focus(window, cx);
                                    this.sync_fields(cx);
                                    cx.notify();
                                }),
                            )
                        }),
                    ),
            )
            .when(independent, |el| {
                el.child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(self.property_field(10, t("top-left"), cx))
                        .child(self.property_field(11, t("top-right"), cx)),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(self.property_field(13, t("bottom-left"), cx))
                        .child(self.property_field(12, t("bottom-right"), cx)),
                )
            })
    }

    fn paint_toggle(
        &self,
        enabled: bool,
        stroke: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        icon_button(
            if stroke == self.stroke_editing {
                "shape-paint-enabled"
            } else if stroke {
                "stroke-visibility"
            } else {
                "fill-visibility"
            },
            if enabled {
                t("paint-hide")
            } else {
                t("paint-enable")
            },
            if enabled {
                LucideIcons::Eye
            } else {
                LucideIcons::Plus
            },
            false,
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            this.history.borrow_mut().break_group();
            this.edit_shape(|shape| {
                if stroke {
                    shape.stroke.enabled = !shape.stroke.enabled;
                } else {
                    shape.fill_enabled = !shape.fill_enabled;
                }
            });
            this.activate_paint(stroke, cx);
            this.focus.focus(window, cx);
        }))
    }

    pub(in crate::workspace) fn activate_paint(&mut self, stroke: bool, cx: &mut Context<Self>) {
        self.history.borrow_mut().break_group();
        self.set_paint_target(stroke);
        self.sync_fields(cx);
        cx.notify();
    }
    pub(in crate::workspace) fn set_paint_target(&mut self, stroke: bool) {
        if self.stroke_editing != stroke {
            self.paint_stops[self.stroke_editing as usize] = self.active_stop;
            self.stroke_editing = stroke;
            self.active_stop = self.paint_stops[stroke as usize];
        }
        self.active_stop = self.shape_paint_stop(stroke);
    }

    pub(in crate::workspace) fn shape_paint_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let shape = self.selected_shape().unwrap();
        div().flex().flex_col().flex_shrink_0().children(
            [false, true]
                .into_iter()
                .filter(|stroke| *stroke || shape.can_fill())
                .map(|stroke| {
                    let enabled = if stroke {
                        shape.stroke.enabled
                    } else {
                        shape.fill_enabled
                    };
                    let label = if stroke { t("stroke") } else { t("fill") };
                    let id = if stroke { "shape-stroke" } else { "shape-fill" };
                    div()
                        .px(px(14.))
                        .py(px(12.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .flex_shrink_0()
                        .border_b_1()
                        .border_color(rgb(BORDER))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .id(id)
                                        .debug_selector(move || id.into())
                                        .flex_1()
                                        .h(px(28.))
                                        .flex()
                                        .items_center()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(label),
                                )
                                .child(self.paint_toggle(enabled, stroke, cx)),
                        )
                        .when(enabled, |el| {
                            el.child(self.active_paint_controls(stroke, cx))
                        })
                }),
        )
    }

    fn active_paint_controls(&self, stroke: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let shape = self.selected_shape().unwrap();
        let open_path = shape.kind.is_path() || shape.kind.is_polygon();
        let enabled = if stroke {
            shape.stroke.enabled
        } else {
            shape.fill_enabled
        };
        let align = shape.stroke.align;
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(10.))
            .when(enabled, |el| el.child(self.paint_value_row_for(stroke, cx)))
            .when(stroke && enabled, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(self.property_field(14, t("stroke-width"), cx))
                        .when(!open_path, |el| {
                            el.children(
                                [
                                    (
                                        StrokeAlign::Inside,
                                        "stroke-inside",
                                        t("stroke-inside"),
                                        LucideIcons::ArrowDownToLine,
                                    ),
                                    (
                                        StrokeAlign::Center,
                                        "stroke-center",
                                        t("stroke-center"),
                                        LucideIcons::Minus,
                                    ),
                                    (
                                        StrokeAlign::Outside,
                                        "stroke-outside",
                                        t("stroke-outside"),
                                        LucideIcons::ArrowUpFromLine,
                                    ),
                                ]
                                .into_iter()
                                .map(
                                    |(value, id, label, glyph)| {
                                        icon_button(id, label, glyph, align == value).on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.history.borrow_mut().break_group();
                                                this.edit_shape(|shape| shape.stroke.align = value);
                                                this.focus.focus(window, cx);
                                                this.sync_fields(cx);
                                                cx.notify();
                                            }),
                                        )
                                    },
                                ),
                            )
                        }),
                )
            })
    }
}
