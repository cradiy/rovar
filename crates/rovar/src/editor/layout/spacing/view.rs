use super::*;
use uic::components::input::{Input, InputAppearance};

impl Workspace {
    pub(in crate::editor) fn spacing_controls(&self, cx: &mut Context<Self>) -> Div {
        let mut overlay = div().absolute().inset_0();
        if self.space_down
            || self.draw_tool.is_some()
            || self.toolbar.hand
            || self
                .gesture
                .is_some_and(|g| !matches!(g.kind, GestureKind::Spacing { .. }))
        {
            return overlay;
        }
        let Some(plan) = self.spacing_plan() else {
            return overlay;
        };
        for index in 0..plan.items.len() - 1 {
            let end = interval(plan.items[index].1, plan.axis).1;
            let start = interval(plan.items[index + 1].1, plan.axis).0;
            let midpoint = (start + end) / 2.;
            let p = self.view.screen(if plan.axis == 0 {
                point(midpoint, plan.cross)
            } else {
                point(plan.cross, midpoint)
            });
            let horizontal = plan.axis == 0;
            let cursor = if horizontal {
                gpui::CursorStyle::ResizeLeftRight
            } else {
                gpui::CursorStyle::ResizeUpDown
            };
            overlay = overlay.child(
                div()
                    .id(("selection-gap-handle", index))
                    .debug_selector(move || format!("selection-gap-handle-{index}"))
                    .absolute()
                    .left(px(p.x - 9.))
                    .top(px(p.y - 9.))
                    .size(px(18.))
                    .rounded(px(5.))
                    .bg(gpui::rgba(0x302333cc))
                    .border_1()
                    .border_color(gpui::rgba(0xf28bd955))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(cursor)
                    .hover(|s| s.bg(rgb(0x51344b)).border_color(rgb(0xf28bd9)))
                    .child(
                        div()
                            .w(px(if horizontal { 2. } else { 10. }))
                            .h(px(if horizontal { 10. } else { 2. }))
                            .rounded(px(1.))
                            .bg(rgb(0xf28bd9)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event, window, cx| {
                            this.begin_spacing(index, event, window, cx)
                        }),
                    ),
            );
            let label_position = if horizontal {
                point(p.x - 26., p.y - 36.)
            } else {
                point(p.x + 14., p.y - 12.)
            };
            let editing = self
                .spacing
                .edit
                .as_ref()
                .is_some_and(|(edit, active)| *active == index && edit.ids() == plan.ids());
            overlay = overlay.child(
                div()
                    .absolute()
                    .left(px(label_position.x))
                    .top(px(label_position.y))
                    .when(editing, |el| {
                        el.child(
                            Input::new(&self.spacing.input)
                                .w(px(64.))
                                .h(px(25.))
                                .px(px(5.))
                                .py_0()
                                .text_size(px(11.))
                                .text_color(rgb(TEXT))
                                .bg(rgb(0x302333))
                                .rounded(px(5.))
                                .border_color(rgb(if self.spacing.invalid {
                                    0xff7f79
                                } else {
                                    0xf28bd9
                                }))
                                .appearance(InputAppearance {
                                    focus_border: rgb(if self.spacing.invalid {
                                        0xff7f79
                                    } else {
                                        0xf28bd9
                                    })
                                    .into(),
                                    caret: rgb(0xf28bd9).into(),
                                    selection: gpui::rgba(0xf28bd944).into(),
                                    ..Default::default()
                                }),
                        )
                    })
                    .when(!editing, |el| {
                        el.child(
                            div()
                                .id(("selection-gap-value", index))
                                .debug_selector(move || format!("selection-gap-value-{index}"))
                                .min_w(px(52.))
                                .h(px(24.))
                                .px(px(5.))
                                .rounded(px(5.))
                                .bg(rgb(0x302333))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(11.))
                                .text_color(rgb(0xf28bd9))
                                .cursor_text()
                                .hover(|s| s.bg(rgb(0x51344b)))
                                .child(label(plan.gap(index)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, window, cx| {
                                        this.edit_spacing(index, window, cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                        )
                    }),
            );
        }
        overlay
    }
}
