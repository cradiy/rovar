use super::*;
use crate::ui::theme::Color;
use gpui::FontWeight;
use uic::components::dropdown::{DropdownPlacement, dropdown};

fn status_action(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .rounded(px(7.))
        .cursor_pointer()
        .flex()
        .items_center()
        .child(label)
        .mx(px(16.))
        .mb(px(16.))
        .h(px(34.))
        .py_0()
        .justify_center()
        .text_size(px(12.))
        .text_color(ACCENT.color())
        .bg(Color::Selected.color())
        .hover(|s| s.bg(Color::Hover.color()))
}

impl Studio {
    pub(in crate::app) fn server_badge(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let editor = self.active_editor()?;
        let remote = self.remote.read(cx);
        let tab = self
            .tabs
            .iter()
            .find(|tab| Some(tab.token) == self.active)?;
        let link = remote.link(&tab.file.path)?;
        let connection = remote.connection(&link.connection)?;
        let name = remote.server_name(&connection.url).to_owned();
        let failed = link.error.is_some() || remote.download_failed(link);
        let pending = link.dirty || remote.download_pending(link);
        let (status, mark, color) = if !connection.authenticated {
            (
                "server-session-expired",
                LucideIcons::LogOut,
                Color::Warning,
            )
        } else if link.conflict {
            (
                "server-conflict-title",
                LucideIcons::CircleAlert,
                Color::Warning,
            )
        } else if failed {
            (
                "server-sync-paused",
                LucideIcons::CircleAlert,
                Color::Warning,
            )
        } else if tab.saving || (pending && remote.connection_busy(&connection.id)) {
            ("server-syncing", LucideIcons::RefreshCw, ACCENT)
        } else if pending || tab.saved_revision != Some(editor.read(cx).document_revision()) {
            ("server-pending", LucideIcons::Clock, Color::Warning)
        } else {
            ("server-synced", LucideIcons::Check, Color::Success)
        };
        let trigger = div()
            .id("server-badge")
            .debug_selector(|| "server-badge".into())
            .size(px(32.))
            .relative()
            .rounded(px(10.))
            .bg(Color::Surface.color())
            .border_1()
            .border_color(Color::Accent.color().opacity(0.2510))
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(3.), Color::Shadow.color().into())
                    .blur_radius(px(10.)),
            ])
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(Color::Hover.color())
                    .border_color(Color::Accent.color().opacity(0.5333))
            })
            .child(icon(LucideIcons::Cloud, 19.).text_color(Color::Accent.color()))
            .child(
                div()
                    .absolute()
                    .right(px(3.))
                    .bottom(px(3.))
                    .size(px(12.))
                    .rounded_full()
                    .bg(Color::Surface.color())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(mark, 9.).text_color(color.color())),
            );
        let connection_id = connection.id.clone();
        let authenticated = connection.authenticated;
        let conflict_path = tab.file.path.clone();
        let panel = div()
            .debug_selector(|| "server-info-panel".into())
            .flex()
            .flex_col()
            .child(
                div()
                    .p(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .size(px(32.))
                            .rounded(px(9.))
                            .bg(Color::Selected.color())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon(LucideIcons::Server, 17.).text_color(ACCENT.color())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(MUTED.color())
                                    .truncate()
                                    .child(format!(
                                        "{} · {}",
                                        connection.identity.username,
                                        connection.space_label()
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .mx(px(16.))
                    .mb(px(16.))
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .child(
                        icon(mark, 14.)
                            .mt(px(2.))
                            .flex_shrink_0()
                            .text_color(color.color()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .text_size(px(12.))
                            .text_color(color.color())
                            .child(t(status))
                            .when(link.conflict || failed, |el| {
                                el.child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(MUTED.color())
                                        .child(t("server-local-copy")),
                                )
                            }),
                    ),
            )
            .when(!authenticated, |el| {
                el.child(
                    status_action("sync-sign-in", t("server-reconnect")).on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.server_info
                                .update(cx, |menu, cx| menu.close(window, cx));
                            this.reauthenticate(connection_id.clone(), window, cx);
                        },
                    )),
                )
            })
            .when(
                authenticated
                    && !link.conflict
                    && !remote.connection_busy(&link.connection)
                    && !tab.saving
                    && (pending || failed),
                |el| {
                    el.child(status_action("sync-retry", t("server-retry-now")).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.remote.update(cx, |remote, cx| {
                                remote.retry(cx);
                                remote.sync(cx);
                            });
                        }),
                    ))
                },
            )
            .when(authenticated && link.conflict, |el| {
                el.child(
                    status_action("sync-save-copy", t("compare-versions")).on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.open_comparison(&conflict_path, window, cx);
                        },
                    )),
                )
            });
        Some(
            div()
                .size(px(32.))
                .mr(px(8.))
                .flex_shrink_0()
                .occlude()
                .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(
                    dropdown(&self.server_info)
                        .placement(DropdownPlacement::BottomEnd)
                        .w(px(284.))
                        .p_0()
                        .rounded(px(14.))
                        .bg(Color::Panel.color())
                        .border_color(Color::Border.color())
                        .shadow_xl()
                        .text_color(TEXT.color())
                        .font_family(crate::ui::font::family(cx))
                        .trigger(trigger)
                        .menu(panel),
                )
                .into_any_element(),
        )
    }
}
