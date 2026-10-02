use super::*;
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
        .text_color(rgb(ACCENT))
        .bg(rgb(0x322b42))
        .hover(|s| s.bg(rgb(0x403550)))
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
        let (status, mark, color) = if !connection.authenticated {
            ("server-session-expired", LucideIcons::LogOut, 0xd5b777)
        } else if link.conflict {
            ("server-conflict-title", LucideIcons::CircleAlert, 0xd5b777)
        } else if link.error.is_some() {
            ("server-sync-paused", LucideIcons::CircleAlert, 0xd5b777)
        } else if tab.saving || (link.dirty && remote.busy) {
            ("server-syncing", LucideIcons::RefreshCw, ACCENT)
        } else if link.dirty || tab.saved_revision != Some(editor.read(cx).document_revision()) {
            ("server-pending", LucideIcons::Clock, 0xd5b777)
        } else {
            ("server-synced", LucideIcons::Check, 0x98c6ad)
        };
        let trigger = div()
            .id("server-badge")
            .debug_selector(|| "server-badge".into())
            .size(px(32.))
            .relative()
            .rounded(px(10.))
            .bg(rgb(0x292533))
            .border_1()
            .border_color(rgba(0xb4a2ee40))
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(3.), rgba(0x00000030).into()).blur_radius(px(10.)),
            ])
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x363044)).border_color(rgba(0xb4a2ee88)))
            .child(icon(LucideIcons::Cloud, 19.).text_color(rgb(0xc5b5e8)))
            .child(
                div()
                    .absolute()
                    .right(px(3.))
                    .bottom(px(3.))
                    .size(px(12.))
                    .rounded_full()
                    .bg(rgb(0x292533))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(mark, 9.).text_color(rgb(color))),
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
                            .bg(rgb(0x322b42))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon(LucideIcons::Server, 17.).text_color(rgb(ACCENT))),
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
                                    .text_color(rgb(MUTED))
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
                            .text_color(rgb(color)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .text_size(px(12.))
                            .text_color(rgb(color))
                            .child(t(status))
                            .when(link.conflict || link.error.is_some(), |el| {
                                el.child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(MUTED))
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
                    && !remote.busy
                    && !tab.saving
                    && (link.dirty || link.error.is_some()),
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
                        .bg(rgb(0x1c1c25))
                        .border_color(rgb(0x45404f))
                        .shadow_xl()
                        .text_color(rgb(TEXT))
                        .font_family(crate::ui::font::family(cx))
                        .trigger(trigger)
                        .menu(panel),
                )
                .into_any_element(),
        )
    }
}
