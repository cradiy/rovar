use super::*;
use crate::scene::{point_gradient::PointGradient, rotation};

impl Workspace {
    pub(super) fn point_gradient_controls(
        &self,
        gradient: PointGradient,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().text_color(MUTED.color()).child(t("gradient-points")))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .children(gradient.points.iter().enumerate().map(|(index, p)| {
                        div()
                            .id(("gradient-point-select", index))
                            .debug_selector(move || format!("gradient-point-select-{index}"))
                            .flex_1()
                            .h(px(32.))
                            .p(px(3.))
                            .rounded(px(6.))
                            .border_2()
                            .border_color(if self.inspector.active_stop == index {
                                ACCENT.color()
                            } else {
                                BORDER.color()
                            })
                            .cursor_pointer()
                            .child(
                                div()
                                    .size_full()
                                    .rounded(px(3.))
                                    .overflow_hidden()
                                    .bg(gpui::checkerboard(Color::Checker.color(), 4.))
                                    .child(div().size_full().bg(p.color)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.history.borrow_mut().break_group();
                                this.inspector.active_stop = index;
                                this.sync_fields(cx);
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(self.paint_input_field(19, "X %", cx))
                    .child(self.paint_input_field(20, "Y %", cx)),
            )
            .child(self.paint_input_field(21, t("point-influence"), cx))
    }

    fn point_gradient_target(&self) -> Option<(usize, Rect, f32, PointGradient)> {
        if !self.multi_selection.is_empty()
            || self.selected_text.is_some()
            || self.preview.is_some()
            || self.image_crop.is_some()
            || self.vector_edit.is_some()
            || self.draw_tool.is_some()
            || self.toolbar.hand
            || self.space_down
            || (self.selected_shape.is_some() && self.inspector.stroke_editing)
        {
            return None;
        }
        let id = self.selected_shape.or(self.selected)?;
        if !self.layer_editable(id) {
            return None;
        }
        let mode = if let Some(s) = self.selected_shape() {
            if !s.fill_enabled || !s.can_fill() || s.kind.is_media() {
                return None;
            }
            s.fill_mode
        } else {
            self.selected_board()?.fill_mode
        };
        let FillMode::Points(g) = mode else {
            return None;
        };
        Some((id, self.world_rect(id)?, self.object_rotation(id), g))
    }

    pub(in crate::editor) fn point_gradient_handles(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let Some((_, rect, angle, gradient)) = self.point_gradient_target() else {
            return Vec::new();
        };
        gradient
            .points
            .iter()
            .enumerate()
            .map(|(index, p)| {
                let world = rotation::around(
                    point(
                        rect.x + p.position.x * rect.width,
                        rect.y + p.position.y * rect.height,
                    ),
                    rotation::center(rect),
                    angle,
                );
                let screen = self.view.screen(world);
                div()
                    .id(("gradient-point-handle", index))
                    .debug_selector(move || format!("gradient-point-handle-{index}"))
                    .absolute()
                    .left(px(screen.x - 9.))
                    .top(px(screen.y - 9.))
                    .size(px(18.))
                    .rounded_full()
                    .border_2()
                    .border_color(if self.inspector.active_stop == index {
                        ACCENT.color()
                    } else {
                        Color::Handle.color()
                    })
                    .bg(Color::Panel.color())
                    .p(px(2.))
                    .shadow_sm()
                    .cursor(gpui::CursorStyle::ClosedHand)
                    .child(div().size_full().rounded_full().bg(p.color))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event, window, cx| {
                            this.begin_point_gradient(index, event, window, cx);
                        }),
                    )
                    .into_any_element()
            })
            .collect()
    }

    fn begin_point_gradient(
        &mut self,
        index: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if self.gesture.is_some() {
            return;
        }
        let Some((_, rect, angle, gradient)) = self.point_gradient_target() else {
            return;
        };
        if rect.width <= 0. || rect.height <= 0. {
            return;
        }
        self.seal_text_edits(cx);
        self.history.borrow_mut().break_group();
        self.inspector.active_stop = index;
        self.sync_fields(cx);
        self.begin(
            GestureKind::PointGradient {
                index,
                original: gradient.points[index].position,
                rect,
                angle,
                zoom: self.view.zoom,
            },
            event.position,
            event.button,
            window,
            cx,
        );
        self.history.borrow_mut().begin_preview();
        cx.notify();
    }

    pub(in crate::editor) fn move_point_gradient(
        &mut self,
        index: usize,
        position: Point<f32>,
        cx: &mut Context<Self>,
    ) {
        let change = |mode: &mut FillMode| {
            if let FillMode::Points(g) = mode {
                g.points[index].position = position;
            }
        };
        if self.selected_shape.is_some() {
            self.edit_shape(|s| change(&mut s.fill_mode));
        } else {
            self.edit_board(None, |b| change(&mut b.fill_mode));
        }
        self.sync_fields(cx);
        cx.notify();
    }
}
