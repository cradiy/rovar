use super::*;
use gpui::{FontWeight, SharedString};
use uic::components::dropdown::{DropdownPlacement, dropdown};

fn row(
    id: impl Into<gpui::ElementId>,
    symbol: LucideIcons,
    label: impl Into<SharedString>,
    selected: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .min_h(px(38.))
        .px(px(10.))
        .py(px(7.))
        .rounded(px(7.))
        .flex()
        .items_center()
        .gap(px(10.))
        .cursor_pointer()
        .bg(rgb(if selected { 0x322a43 } else { 0x1c1c25 }))
        .hover(|s| s.bg(rgb(0x302b3d)))
        .child(icon(symbol, 16.).text_color(rgb(if selected { ACCENT } else { MUTED })))
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .when(selected, |el| {
            el.child(icon(LucideIcons::Check, 14.).text_color(rgb(ACCENT)))
        })
}

impl Studio {
    pub(super) fn source_control(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let remote = self.remote.read(cx);
        let active = self
            .source
            .as_ref()
            .and_then(|id| remote.connection(id))
            .cloned();
        let connections = remote
            .connections()
            .iter()
            .filter(|c| {
                c.authenticated
                    && (!cfg!(target_family = "wasm")
                        || active.as_ref().is_some_and(|a| {
                            a.url == c.url && a.identity.user_id == c.identity.user_id
                        }))
            })
            .cloned()
            .collect::<Vec<_>>();
        let label = active
            .as_ref()
            .map(|c| c.space_label())
            .unwrap_or_else(|| t("server-local").into());
        let symbol = if active.as_ref().is_some_and(|c| c.space.kind == "team") {
            LucideIcons::Users
        } else {
            LucideIcons::House
        };
        let account = active
            .as_ref()
            .map(|c| c.identity.username.clone())
            .unwrap_or_else(|| t("server-local").into());
        let header = div()
            .px(px(12.))
            .pt(px(12.))
            .pb(px(14.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .size(px(32.))
                    .rounded(px(9.))
                    .bg(rgb(0x393047))
                    .text_color(rgb(0xd1bdf7))
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(
                        account
                            .chars()
                            .next()
                            .unwrap_or('•')
                            .to_uppercase()
                            .to_string(),
                    ),
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
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(account),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(t("space-switch")),
                    ),
            );
        let spaces = div()
            .id("source-space-list")
            .max_h(px(240.))
            .overflow_y_scroll()
            .px(px(6.))
            .flex()
            .flex_col()
            .gap(px(3.))
            .when(!cfg!(target_family = "wasm"), |el| {
                el.child(
                    row(
                        "source-local",
                        LucideIcons::Laptop,
                        t("server-local"),
                        self.source.is_none(),
                    )
                    .on_click(
                        cx.listener(|this, _, window, cx| this.select_source(None, window, cx)),
                    ),
                )
            })
            .children(connections.into_iter().map(|connection| {
                let selected = self.source.as_ref() == Some(&connection.id);
                let id = connection.id;
                let symbol = if connection.space.kind == "team" {
                    LucideIcons::Users
                } else {
                    LucideIcons::House
                };
                let label = if connection.space.kind == "personal" {
                    t("space-personal").into()
                } else {
                    connection.space.name
                };
                let secondary = format!(
                    "{} · {}",
                    remote.server_name(&connection.url),
                    connection.identity.username
                );
                row(
                    SharedString::from(format!("source-{id}")),
                    symbol,
                    label,
                    selected,
                )
                .when(!cfg!(target_family = "wasm"), |el| {
                    el.child(
                        div()
                            .max_w(px(120.))
                            .truncate()
                            .text_size(px(10.))
                            .text_color(rgb(MUTED))
                            .child(secondary),
                    )
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_source(Some(id.clone()), window, cx)
                }))
            }));
        let actions = div()
            .mx(px(6.))
            .mt(px(8.))
            .pt(px(6.))
            .pb(px(6.))
            .border_t_1()
            .border_color(rgb(0x34303e))
            .flex()
            .flex_col()
            .gap(px(2.))
            .when(active.is_some(), |el| {
                el.child(
                    row(
                        "account-settings",
                        LucideIcons::User,
                        t("account-settings"),
                        false,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.source_menu
                            .update(cx, |menu, cx| menu.close(window, cx));
                        if let Some(id) = this.source.clone() {
                            this.open_account(id, window, cx);
                        }
                    })),
                )
                .child(
                    row(
                        "space-manage",
                        LucideIcons::Settings,
                        t("space-manage"),
                        false,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.source_menu
                            .update(cx, |menu, cx| menu.close(window, cx));
                        this.open_spaces(window, cx);
                    })),
                )
            })
            .when(!cfg!(target_family = "wasm"), |el| {
                el.child(
                    row(
                        "source-manage",
                        LucideIcons::Server,
                        t("server-manage"),
                        false,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.source_menu
                            .update(cx, |menu, cx| menu.close(window, cx));
                        this.open_servers(None, window, cx);
                    })),
                )
            })
            .when(cfg!(target_family = "wasm"), |el| {
                el.child(
                    row(
                        "source-sign-out",
                        LucideIcons::LogOut,
                        t("server-sign-out"),
                        false,
                    )
                    .text_color(rgb(MUTED))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.source_menu
                            .update(cx, |menu, cx| menu.close(window, cx));
                        if let Some(id) = this.source.clone() {
                            this.server_logout(id, window, cx);
                        }
                    })),
                )
            });
        let trigger = div()
            .id("source-menu")
            .h(px(36.))
            .max_w(px(260.))
            .px(px(12.))
            .rounded(px(9.))
            .border_1()
            .border_color(rgb(0x3d374b))
            .bg(rgb(0x262230))
            .flex()
            .items_center()
            .gap(px(9.))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x322b40)))
            .child(icon(symbol, 16.).text_color(rgb(ACCENT)))
            .child(div().min_w_0().truncate().child(label))
            .child(icon(LucideIcons::ChevronDown, 13.).text_color(rgb(MUTED)));
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(13.))
            .child(
                dropdown(&self.source_menu)
                    .placement(DropdownPlacement::BottomEnd)
                    .w(px(284.))
                    .max_h(px(450.))
                    .p_0()
                    .rounded(px(12.))
                    .bg(rgb(0x1c1c25))
                    .border_color(rgb(0x45404f))
                    .shadow_xl()
                    .text_color(rgb(TEXT))
                    .font_family(crate::ui::font::family(cx))
                    .text_size(px(13.))
                    .trigger(trigger)
                    .menu(
                        div()
                            .flex()
                            .flex_col()
                            .child(header)
                            .child(spaces)
                            .child(actions),
                    ),
            )
            .when(active.is_some(), |el| {
                el.child(
                    div()
                        .id("server-refresh")
                        .size(px(36.))
                        .rounded(px(9.))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(MUTED))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(0x26232f)).text_color(rgb(TEXT)))
                        .child(icon(LucideIcons::RefreshCw, 16.))
                        .on_click(cx.listener(|this, _, _, cx| this.refresh_server(cx))),
                )
            })
    }
}
