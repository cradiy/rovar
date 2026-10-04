use super::*;
use crate::ui::theme::Color;
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
                    .bg(Color::Selected.color().opacity(0.8000))
                    .border_1()
                    .border_color(Color::Component.color().opacity(0.3333))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(cursor)
                    .hover(|s| {
                        s.bg(Color::Hover.color())
                            .border_color(Color::Component.color())
                    })
                    .child(
                        div()
                            .w(px(if horizontal { 2. } else { 10. }))
                            .h(px(if horizontal { 10. } else { 2. }))
                            .rounded(px(1.))
                            .bg(Color::Component.color()),
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
                                .text_color(TEXT.color())
                                .bg(Color::Selected.color())
                                .rounded(px(5.))
                                .border_color(if self.spacing.invalid {
                                    Color::Danger.color()
                                } else {
                                    Color::Component.color()
                                })
                                .appearance(InputAppearance {
                                    focus_border: if self.spacing.invalid {
                                        Color::Danger.color()
                                    } else {
                                        Color::Component.color()
                                    }
                                    .into(),
                                    caret: Color::Component.color().into(),
                                    selection: Color::Component.color().opacity(0.2667).into(),
                                    ..crate::ui::theme::input_appearance()
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
                                .bg(Color::Selected.color())
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(11.))
                                .text_color(Color::Component.color())
                                .cursor_text()
                                .hover(|s| s.bg(Color::Hover.color()))
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
