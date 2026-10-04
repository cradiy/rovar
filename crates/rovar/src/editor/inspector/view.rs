//! Selection-specific inspector layout.

use super::*;
use crate::ui::theme::Color;

impl Workspace {
    pub(in crate::editor) fn properties(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
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
        let collapsed = self.panels.right_collapsed;
        let toggle = icon_button(
            "toggle-properties",
            t(if collapsed {
                "expand-properties"
            } else {
                "collapse-properties"
            }),
            if collapsed {
                LucideIcons::ChevronsLeft
            } else {
                LucideIcons::ChevronsRight
            },
            false,
        )
        .size(px(28.))
        .on_click(cx.listener(|this, _, window, cx| this.toggle_properties(window, cx)));
        let surface = layers::glass_surface()
            .id("properties-panel")
            .debug_selector(|| "properties-panel".into())
            .absolute()
            .top(px(panels::PANEL_TOP))
            .right(px(14.))
            .flex_shrink_0()
            .rounded(px(16.))
            .border_1()
            .border_color(Color::Accent.color().opacity(0.1882))
            .shadow_lg()
            .occlude()
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation());
        if collapsed {
            return surface.p(px(8.)).child(toggle).into_any_element();
        }
        surface
            .bottom(px(panels::PANEL_BOTTOM))
            .w(px(self.panels.width(panels::Side::Right)))
            .flex()
            .flex_col()
            .child(self.inspector_modes(toggle, cx))
            .child(div().h(px(1.)).flex_shrink_0().bg(BORDER.color()))
            .when(selected || !self.multi_selection.is_empty(), |el| {
                el.child(
                    div()
                        .h(px(48.))
                        .px(px(12.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .size(px(28.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(icon(glyph, 16.).text_color(MUTED.color())),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .map(|el| {
                                    if self.selection_ids().len() == 1
                                        && self.selected_board().is_some()
                                        && self.selected_text.is_none()
                                        && self.selected_shape.is_none()
                                    {
                                        el.child(self.frame_preset_control(cx))
                                    } else {
                                        el.child(title)
                                    }
                                }),
                        )
                        .when(self.multi_selection.len() > 1, |el| {
                            el.child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(MUTED.color())
                                    .child(self.multi_selection.len().to_string()),
                            )
                        })
                        .child(div().flex().items_center().gap(px(2.)).when(
                            self.can_create_component(),
                            |el| {
                                el.child(
                                    icon_button(
                                        "inspector-create-component",
                                        t("component-create"),
                                        LucideIcons::Component,
                                        false,
                                    )
                                    .size(px(28.))
                                    .on_click(cx.listener(
                                        |this, _, window, cx| this.create_component(window, cx),
                                    )),
                                )
                            },
                        )),
                )
                .child(div().h(px(1.)).flex_shrink_0().bg(BORDER.color()))
            })
            .when(self.selected_text.is_some(), |el| {
                el.child(self.text_properties(cx))
            })
            .when(selected && self.selected_text.is_none(), |el| {
                el.child(
                    div()
                        .id("properties-scroll")
                        .track_scroll(&self.inspector.inspector_scroll)
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
                        .child(self.variant_controls(cx))
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
                            el.child(
                                inspector_section(t("fill"))
                                    .child(self.color_style_control(false, cx))
                                    .child(self.paint_value_row(cx)),
                            )
                        })
                        .child(self.auto_layout_controls(cx))
                        .child(self.constraint_controls(cx))
                        .child(self.size_limit_controls(cx))
                        .child(self.shadow_controls(cx))
                        .child(self.export_properties(cx)),
                )
            })
            .when(!self.multi_selection.is_empty(), |el| {
                el.child(self.multi_properties(cx))
            })
            .when(!selected && self.multi_selection.is_empty(), |el| {
                el.child(
                    div()
                        .debug_selector(|| "inspector-empty".into())
                        .flex_1()
                        .min_h_0()
                        .p(px(24.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .text_center()
                        .gap(px(12.))
                        .child(icon(LucideIcons::SlidersHorizontal, 24.).text_color(MUTED.color()))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(MUTED.color())
                                .child(t("inspector-empty")),
                        ),
                )
            })
            .when(self.export.busy || self.export.status.is_some(), |el| {
                el.child(self.export_feedback(cx))
            })
            .child(self.panel_resize_handle(panels::Side::Right, cx))
            .into_any_element()
    }
    pub(in crate::editor) fn geometry_controls(&self, cx: &mut Context<Self>) -> Div {
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
            .children(self.layout_position_control(cx))
    }
}
