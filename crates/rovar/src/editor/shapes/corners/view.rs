use super::*;
use crate::ui::theme::Color;
use uic::components::input::{Input, InputAppearance};

impl Workspace {
    pub(in crate::editor) fn corner_controls(&self, cx: &mut Context<Self>) -> Div {
        let overlay = div().absolute().inset_0();
        let Some(shape) = self.corner_shape(cx) else {
            return overlay;
        };
        let Some(target) = self.corner_editor.active().filter(|t| t.id == shape.id) else {
            return overlay;
        };
        let p = self.corner_point(shape, target.corner);
        let editing = self.corner_editor.editing();
        let dragging = self.corner_editor.drag.is_some();
        if self.corner_editor.drag.is_none()
            && !editing
            && self.layers_at(self.bounds.get().origin + p.map(px)).first() != Some(&target.id)
        {
            return overlay;
        }
        overlay
            .child(
                div()
                    .id("corner-radius-handle")
                    .debug_selector(|| "corner-radius-handle".into())
                    .absolute()
                    .left(px(p.x - 9.))
                    .top(px(p.y - 9.))
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(
                        div()
                            .size(px(7.))
                            .rounded_full()
                            .border_1()
                            .border_color(ACCENT.color())
                            .bg(Color::Handle.color())
                            .when(dragging || editing, |el| el.bg(ACCENT.color())),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event, window, cx| {
                            this.begin_corner_radius(target, event, window, cx);
                        }),
                    ),
            )
            .when(dragging || editing, |overlay| {
                overlay.child(
                    div()
                        .absolute()
                        .left(px(p.x + 22.))
                        .top(px(p.y + 20.))
                        .when(editing, |el| {
                            el.child(
                                Input::new(&self.corner_editor.input)
                                    .w(px(56.))
                                    .h(px(24.))
                                    .px(px(5.))
                                    .py_0()
                                    .text_size(px(11.))
                                    .bg(PANEL.color())
                                    .text_color(TEXT.color())
                                    .rounded(px(5.))
                                    .appearance(InputAppearance {
                                        focus_border: if self.corner_editor.invalid {
                                            Color::Danger.color()
                                        } else {
                                            ACCENT.color()
                                        }
                                        .into(),
                                        ..crate::ui::theme::input_appearance()
                                    }),
                            )
                        })
                        .when(!editing, |el| {
                            el.child(
                                div()
                                    .id("corner-radius-value")
                                    .debug_selector(|| "corner-radius-value".into())
                                    .h(px(20.))
                                    .px(px(5.))
                                    .rounded(px(4.))
                                    .bg(PANEL.color())
                                    .text_color(TEXT.color())
                                    .text_size(px(11.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(4.))
                                    .child(div().text_color(MUTED.color()).child("R"))
                                    .child(crate::editor::inspector::number(
                                        shape.displayed_radii()[target.corner],
                                    )),
                            )
                        }),
                )
            })
    }
}
