use super::*;
use crate::i18n::t;
use crate::ui::theme::Color;

impl Workspace {
    fn crop_zoom_button(&self, increase: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let id = if increase {
            "image-crop-in"
        } else {
            "image-crop-out"
        };
        div()
            .id(id)
            .debug_selector(move || id.into())
            .size(px(28.))
            .rounded(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(MUTED.color())
            .hover(|s| {
                s.bg(Color::Text.color().opacity(0.0471))
                    .text_color(TEXT.color())
            })
            .child(icon(
                if increase {
                    LucideIcons::Plus
                } else {
                    LucideIcons::Minus
                },
                14.,
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.zoom_image_crop(if increase { 1.1 } else { 1. / 1.1 }, None, cx)
            }))
    }

    pub(in crate::editor) fn image_crop_button(
        &self,
        id: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id("image-crop-start")
            .debug_selector(|| "image-crop-start".into())
            .size(px(28.))
            .flex_shrink_0()
            .rounded(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|el| el.bg(BORDER.color()))
            .child(icon(LucideIcons::Crop, 16.).text_color(MUTED.color()))
            .tooltip(|_, cx| cx.new(|_| toolbar::ToolTip(t("image-crop").into())).into())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.start_image_crop(id, window, cx);
            }))
    }

    pub(in crate::editor) fn image_crop_overlay(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(crop) = &self.image_crop else {
            return div().into_any_element();
        };
        let p = self.view.screen(point(crop.frame.x, crop.frame.y));
        let width = crop.frame.width * self.view.zoom;
        let height = crop.frame.height * self.view.zoom;
        let frame = div()
            .absolute()
            .left(px(p.x))
            .top(px(p.y))
            .w(px(width))
            .h(px(height))
            .border_1()
            .border_color(ACCENT.color())
            .children([1., 2.].into_iter().flat_map(|i| {
                [
                    div()
                        .absolute()
                        .left(gpui::relative(i / 3.))
                        .top_0()
                        .bottom_0()
                        .w(px(1.))
                        .bg(Color::Text.color().opacity(0.1882)),
                    div()
                        .absolute()
                        .top(gpui::relative(i / 3.))
                        .left_0()
                        .right_0()
                        .h(px(1.))
                        .bg(Color::Text.color().opacity(0.1882)),
                ]
            }));
        div()
            .id("image-crop-surface")
            .debug_selector(|| "image-crop-surface".into())
            .absolute()
            .inset_0()
            .occlude()
            .cursor(gpui::CursorStyle::OpenHand)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::begin_image_crop_drag))
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(cx.listener(Self::scroll_canvas))
            .on_pinch(cx.listener(Self::pinch_canvas))
            .child(crate::scene::rotation::surface(
                frame,
                crop.rotation,
                width,
                height,
                0.,
            ))
            .child(
                div()
                    .absolute()
                    .bottom(px(28.))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .id("image-crop-toolbar")
                            .debug_selector(|| "image-crop-toolbar".into())
                            .occlude()
                            .p(px(6.))
                            .rounded(px(14.))
                            .border_1()
                            .border_color(BORDER.color())
                            .bg(PANEL.color())
                            .shadow_lg()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .line_height(px(16.))
                            .text_color(TEXT.color())
                            .cursor_default()
                            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                            .child(
                                div()
                                    .id("crop-help")
                                    .size(px(30.))
                                    .rounded(px(8.))
                                    .bg(Color::Accent.color().opacity(0.0784))
                                    .text_color(ACCENT.color())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon(LucideIcons::Crop, 16.))
                                    .tooltip(|_, cx| {
                                        cx.new(|_| toolbar::ToolTip(t("image-crop-hint").into()))
                                            .into()
                                    }),
                            )
                            .child(
                                div()
                                    .h(px(30.))
                                    .px(px(1.))
                                    .rounded(px(8.))
                                    .bg(WORKSPACE.color())
                                    .flex()
                                    .items_center()
                                    .child(self.crop_zoom_button(false, cx))
                                    .child(
                                        div().w(px(46.)).text_center().child(format!(
                                            "{:.0}%",
                                            crop.image.placement.zoom * 100.
                                        )),
                                    )
                                    .child(self.crop_zoom_button(true, cx)),
                            )
                            .child(
                                div()
                                    .id("image-crop-reset")
                                    .debug_selector(|| "image-crop-reset".into())
                                    .size(px(30.))
                                    .rounded(px(8.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(MUTED.color())
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(Color::Text.color().opacity(0.0471))
                                            .text_color(TEXT.color())
                                    })
                                    .child(icon(LucideIcons::RotateCcw, 15.))
                                    .tooltip(|_, cx| {
                                        cx.new(|_| toolbar::ToolTip(t("image-crop-reset").into()))
                                            .into()
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(crop) = &mut this.image_crop {
                                            crop.image.placement = Placement::default();
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(div().w(px(1.)).h(px(16.)).bg(BORDER.color()))
                            .children(
                                [("image-crop-cancel", "cancel"), ("image-crop-done", "done")]
                                    .into_iter()
                                    .map(|(id, label)| {
                                        div()
                                            .id(id)
                                            .debug_selector(move || id.into())
                                            .h(px(30.))
                                            .px(px(10.))
                                            .rounded(px(8.))
                                            .flex()
                                            .items_center()
                                            .gap(px(5.))
                                            .cursor_pointer()
                                            .when(id == "image-crop-done", |el| {
                                                el.bg(ACCENT.color())
                                                    .text_color(Color::OnAccent.color())
                                                    .hover(|s| s.bg(Color::Accent.color()))
                                                    .child(
                                                        icon(LucideIcons::Check, 14.)
                                                            .text_color(Color::OnAccent.color()),
                                                    )
                                            })
                                            .when(id != "image-crop-done", |el| {
                                                el.text_color(MUTED.color()).hover(|s| {
                                                    s.bg(Color::Text.color().opacity(0.0471))
                                                        .text_color(TEXT.color())
                                                })
                                            })
                                            .child(t(label))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.finish_image_crop(
                                                    id == "image-crop-done",
                                                    window,
                                                    cx,
                                                );
                                            }))
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }
}
