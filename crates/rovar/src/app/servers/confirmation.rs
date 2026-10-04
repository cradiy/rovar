use super::*;
use crate::ui::theme::Color;

pub(in crate::app) enum PendingAction {
    Remove(String),
    SignOut(String),
}

impl Studio {
    pub(in crate::app) fn finish_server_action(
        &mut self,
        confirm: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.server_action.take() else {
            return;
        };
        if confirm {
            match action {
                PendingAction::Remove(url) => self.remove_server(url, cx),
                PendingAction::SignOut(id) => self.perform_server_logout(id, window, cx),
            }
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(in crate::app) fn server_action_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let remote = self.remote.read(cx);
        let (label, hint, symbol, target) = match self.server_action.as_ref().unwrap() {
            PendingAction::Remove(url) => (
                "server-remove",
                "server-remove-confirm",
                LucideIcons::Trash2,
                remote.server_name(url).to_owned(),
            ),
            PendingAction::SignOut(id) => (
                "server-sign-out",
                "server-sign-out-confirm",
                LucideIcons::LogOut,
                remote
                    .connection(id)
                    .map(|c| format!("{} · {}", c.identity.username, remote.server_name(&c.url)))
                    .unwrap_or_default(),
            ),
        };
        let body = div()
            .id("server-action-dialog")
            .debug_selector(|| "server-action-dialog".into())
            .w(px(380.))
            .max_w_full()
            .p(px(20.))
            .rounded(px(14.))
            .bg(PANEL.color())
            .border_1()
            .border_color(BORDER.color())
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
                    .child(icon(symbol, 18.).text_color(Color::Danger.color()))
                    .child(div().text_size(px(16.)).child(t(label))),
            )
            .child(div().text_size(px(13.)).truncate().child(target))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .child(t(hint)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .children([false, true].map(|confirm| {
                        let id = if confirm {
                            "confirm-server-action"
                        } else {
                            "cancel-server-action"
                        };
                        div()
                            .id(id)
                            .rounded(px(7.))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .child(t(if confirm { label } else { "cancel" }))
                            .debug_selector(move || id.into())
                            .h(px(34.))
                            .px(px(14.))
                            .justify_center()
                            .bg(if confirm {
                                Color::DangerSurface.color()
                            } else {
                                Color::Input.color()
                            })
                            .hover(move |s| {
                                s.bg(if confirm {
                                    Color::DangerSurface.color()
                                } else {
                                    Color::Hover.color()
                                })
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.finish_server_action(confirm, window, cx);
                                cx.stop_propagation();
                            }))
                    })),
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
                .on_any_mouse_down(
                    cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                        if event.button == gpui::MouseButton::Left {
                            this.finish_server_action(false, window, cx);
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(body),
        )
        .with_priority(30)
    }
}
