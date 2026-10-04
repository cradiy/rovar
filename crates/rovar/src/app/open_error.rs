use super::*;
use crate::ui::theme::Color;

pub(super) struct OpenError {
    title: String,
    detail: String,
}

impl Studio {
    pub(super) fn show_open_error(
        &mut self,
        title: String,
        detail: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.suspend(window, cx));
        }
        self.open_errors.push_back(OpenError { title, detail });
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn dismiss_open_error(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_errors.pop_front();
        self.focus.focus(window, cx);
        if self.open_errors.is_empty()
            && self.preferences.is_none()
            && self.renaming.is_none()
            && self.deleting_document.is_none()
            && self.server_action.is_none()
            && let Some(editor) = self.active_editor()
        {
            editor.update(cx, |editor, cx| editor.focus_canvas(window, cx));
        }
        cx.notify();
    }

    pub(super) fn open_error_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let error = self.open_errors.front().unwrap();
        let body = div()
            .id("open-error-dialog")
            .debug_selector(|| "open-error-dialog".into())
            .w(px(440.))
            .max_w_full()
            .p(px(24.))
            .rounded(px(14.))
            .bg(Color::Panel.color())
            .border_1()
            .border_color(Color::Text.color().opacity(0.1255))
            .shadow_xl()
            .flex()
            .flex_col()
            .gap(px(16.))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icon(LucideIcons::CircleAlert, 20.).text_color(Color::Danger.color()))
                    .child(div().text_size(px(16.)).child(t("open-error-title"))),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .truncate()
                    .child(error.title.clone()),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .child(t("open-error-hint")),
            )
            .child(
                div()
                    .id("open-error-detail")
                    .max_h(px(160.))
                    .overflow_y_scroll()
                    .p(px(12.))
                    .rounded(px(8.))
                    .bg(Color::Input.color())
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .child(error.detail.clone()),
            )
            .child(
                div().flex().justify_end().child(
                    div()
                        .id("dismiss-open-error")
                        .debug_selector(|| "dismiss-open-error".into())
                        .px(px(18.))
                        .h(px(32.))
                        .rounded(px(7.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.))
                        .bg(Color::Selected.color())
                        .hover(|style| style.bg(Color::Hover.color()))
                        .cursor_pointer()
                        .child(t("close"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dismiss_open_error(window, cx);
                            cx.stop_propagation();
                        })),
                ),
            );
        gpui::deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(Color::Overlay.color())
                .flex()
                .items_center()
                .justify_center()
                .p(px(20.))
                .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(body),
        )
        .with_priority(40)
    }
}
