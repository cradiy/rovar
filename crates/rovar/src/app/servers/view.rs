use super::*;
use gpui::{FontWeight, MouseButton};
use uic::components::input::{Input, InputAppearance};

const CARD: u32 = 0x24232e;

pub(super) fn primary(
    id: &'static str,
    label: &'static str,
    busy: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .rounded(px(7.))
        .cursor_pointer()
        .child(label)
        .h(px(38.))
        .px(px(16.))
        .justify_center()
        .font_weight(FontWeight::SEMIBOLD)
        .bg(rgb(ACCENT))
        .text_color(rgb(0x21182f))
        .hover(|s| s.bg(rgb(0xc4b3f5)))
        .opacity(if busy { 0.5 } else { 1. })
}

fn tile(symbol: LucideIcons) -> gpui::Div {
    div()
        .size(px(38.))
        .flex_shrink_0()
        .rounded(px(10.))
        .bg(rgb(0x352e45))
        .flex()
        .items_center()
        .justify_center()
        .child(icon(symbol, 19.).text_color(rgb(ACCENT)))
}

pub(super) fn field(
    id: &'static str,
    label: &'static str,
    input: &Entity<TextInput>,
    cx: &gpui::App,
) -> gpui::Div {
    field_with_suffix(id, label, input, None, cx)
}

fn field_with_suffix(
    id: &'static str,
    label: &'static str,
    input: &Entity<TextInput>,
    suffix: Option<gpui::AnyElement>,
    cx: &gpui::App,
) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(0xbab5c8))
                .child(label),
        )
        .child(
            div().debug_selector(move || id.into()).child(
                Input::new(input)
                    .when_some(suffix, |input, suffix| input.suffix(suffix))
                    .w_full()
                    .h(px(44.))
                    .font_family(crate::ui::font::family(cx))
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .bg(rgb(0x22232c))
                    .text_color(rgb(TEXT))
                    .border_color(rgb(BORDER))
                    .rounded(px(9.))
                    .appearance(InputAppearance {
                        focus_border: rgb(ACCENT).into(),
                        caret: rgb(ACCENT).into(),
                        selection: rgba(0xb4a2ee44).into(),
                        ..Default::default()
                    }),
            ),
        )
}

impl Studio {
    pub(super) fn password_field(
        &self,
        id: &'static str,
        label: &'static str,
        slot: usize,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let panel = self.servers.as_ref().unwrap();
        let input = [
            &panel.password,
            &panel.new_password,
            &panel.confirm_password,
        ][slot];
        let visible = panel.password_visible[slot];
        let toggle = div()
            .id(("password-visibility", slot))
            .debug_selector(move || format!("{id}-visibility"))
            .size(px(28.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(rgba(0xffffff0d)))
            .child(
                icon(
                    if visible {
                        LucideIcons::EyeOff
                    } else {
                        LucideIcons::Eye
                    },
                    17.,
                )
                .text_color(rgb(if visible { ACCENT } else { MUTED })),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                let Some(panel) = &mut this.servers else {
                    return;
                };
                panel.password_visible[slot] = !panel.password_visible[slot];
                let mode = if panel.password_visible[slot] {
                    uic::components::input::InputMode::Text
                } else {
                    uic::components::input::InputMode::Password
                };
                [
                    &panel.password,
                    &panel.new_password,
                    &panel.confirm_password,
                ][slot]
                    .update(cx, |input, cx| {
                        input.set_mode(mode);
                        cx.notify();
                    });
                cx.notify();
            }));
        field_with_suffix(id, label, input, Some(toggle.into_any_element()), cx)
    }

    pub(in crate::app) fn server_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let panel = self.servers.as_ref().unwrap();
        let field = |id, label, input: &Entity<TextInput>| field(id, label, input, cx);
        let selected = panel.selected.as_deref().unwrap_or_default();
        let header = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .when(
                panel.view != View::Servers && !cfg!(target_family = "wasm"),
                |el| {
                    el.child(
                        button("servers-back", "")
                            .size(px(32.))
                            .p_0()
                            .justify_center()
                            .child(icon(LucideIcons::ArrowLeft, 18.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                let view =
                                    if this.servers.as_ref().is_some_and(|p| {
                                        matches!(p.view, View::Login | View::Settings)
                                    }) {
                                        View::Accounts
                                    } else {
                                        View::Servers
                                    };
                                this.server_view(view, window, cx);
                            })),
                    )
                },
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t(match panel.view {
                                View::Servers => "server-manage",
                                View::Accounts => "server-accounts",
                                View::Rename => "server-rename",
                                View::Settings => "account-settings",
                                View::Login if panel.mode == "login" => "server-sign-in",
                                View::Login => "server-register",
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .truncate()
                            .child(if panel.view == View::Servers {
                                SharedString::from(t("server-address-first"))
                            } else {
                                self.remote.read(cx).server_name(selected).to_owned().into()
                            }),
                    ),
            )
            .child(
                button("server-close", "")
                    .size(px(30.))
                    .p_0()
                    .justify_center()
                    .child(icon(LucideIcons::X, 17.))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.servers = None;
                        this.focus.focus(window, cx);
                        cx.notify();
                    })),
            );

        let content = match panel.view {
            View::Servers => {
                let remote = self.remote.read(cx);
                div()
                    .flex()
                    .flex_col()
                    .gap(px(20.))
                    .when(!remote.servers().is_empty(), |el| {
                        el.child(
                            div()
                                .id("saved-servers")
                                .max_h(px(240.))
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .children(remote.servers().iter().enumerate().map(
                                    |(index, (url, name))| {
                                        let address = url.clone();
                                        let rename = url.clone();
                                        let remove = url.clone();
                                        let active = self
                                            .source
                                            .as_ref()
                                            .and_then(|id| remote.connection(id))
                                            .is_some_and(|c| c.url == *url);
                                        div()
                                            .id(("saved-server", index))
                                            .debug_selector(move || format!("saved-server-{index}"))
                                            .flex()
                                            .items_center()
                                            .gap(px(12.))
                                            .p(px(12.))
                                            .rounded(px(11.))
                                            .border_1()
                                            .border_color(rgb(if active {
                                                0x655580
                                            } else {
                                                BORDER
                                            }))
                                            .bg(rgb(CARD))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(0x2e2a3c)))
                                            .child(tile(LucideIcons::Server))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(4.))
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .child(name.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(12.))
                                                            .text_color(rgb(MUTED))
                                                            .child(t("server-manage-accounts")),
                                                    ),
                                            )
                                            .when(active, |el| {
                                                el.child(
                                                    icon(LucideIcons::Check, 15.)
                                                        .text_color(rgb(ACCENT)),
                                                )
                                            })
                                            .child(
                                                button(("rename-server", index), "")
                                                    .size(px(30.))
                                                    .p_0()
                                                    .justify_center()
                                                    .child(
                                                        icon(LucideIcons::Pencil, 15.)
                                                            .text_color(rgb(MUTED)),
                                                    )
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            cx.stop_propagation();
                                                            this.rename_server(
                                                                rename.clone(),
                                                                window,
                                                                cx,
                                                            );
                                                        },
                                                    )),
                                            )
                                            .child(
                                                button(("remove-server", index), "")
                                                    .child(
                                                        icon(LucideIcons::X, 15.)
                                                            .text_color(rgb(MUTED)),
                                                    )
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            this.remove_server(remove.clone(), cx);
                                                        },
                                                    )),
                                            )
                                            .child(
                                                icon(LucideIcons::ChevronRight, 16.)
                                                    .text_color(rgb(MUTED)),
                                            )
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.select_server(address.clone(), window, cx)
                                            }))
                                    },
                                )),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .when(!remote.servers().is_empty(), |el| {
                                el.pt(px(18.)).border_t_1().border_color(rgb(BORDER))
                            })
                            .child(field("server-name", t("server-name"), &panel.name))
                            .child(field("server-url", t("server-address"), &panel.url))
                            .child(
                                primary(
                                    "server-continue",
                                    t(if panel.busy {
                                        "server-connecting"
                                    } else {
                                        "server-continue"
                                    }),
                                    panel.busy,
                                )
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.server_address_submit(window, cx),
                                )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(icon(LucideIcons::Laptop, 15.))
                            .child(t("server-local-available")),
                    )
            }
            View::Rename => div()
                .flex()
                .flex_col()
                .gap(px(16.))
                .child(field("server-name", t("server-name"), &panel.name))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .truncate()
                        .child(selected.to_owned()),
                )
                .child(
                    primary("server-save-name", t("server-save-name"), false).on_click(
                        cx.listener(|this, _, window, cx| this.save_server_name(window, cx)),
                    ),
                ),
            View::Accounts => {
                let accounts = self.remote.read(cx).accounts(selected);
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .when(accounts.is_empty(), |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(12.))
                                .py(px(24.))
                                .child(tile(LucideIcons::User))
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(t("server-no-accounts")),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(rgb(MUTED))
                                        .child(t("server-account-hint")),
                                ),
                        )
                    })
                    .child(
                        div()
                            .id("server-accounts")
                            .max_h(px(280.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .children(accounts.into_iter().enumerate().map(|(index, account)| {
                                let id = account.id.clone();
                                let logout = id.clone();
                                let settings = id.clone();
                                let username = account.identity.username.clone();
                                let authenticated = account.authenticated;
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .p(px(8.))
                                    .rounded(px(11.))
                                    .bg(rgb(CARD))
                                    .child(
                                        div()
                                            .id(("server-account", index))
                                            .debug_selector(move || {
                                                format!("server-account-{index}")
                                            })
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .items_center()
                                            .gap(px(12.))
                                            .p(px(6.))
                                            .rounded(px(8.))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(0x302b3d)))
                                            .child(tile(LucideIcons::User))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(4.))
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .child(account.identity.username),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(12.))
                                                            .text_color(rgb(if authenticated {
                                                                ACCENT
                                                            } else {
                                                                MUTED
                                                            }))
                                                            .child(t(if authenticated {
                                                                "server-open-workspace"
                                                            } else {
                                                                "server-sign-in"
                                                            })),
                                                    ),
                                            )
                                            .child(
                                                icon(LucideIcons::ChevronRight, 16.)
                                                    .text_color(rgb(MUTED)),
                                            )
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                if this.signing_out {
                                                    return;
                                                }
                                                if authenticated {
                                                    this.servers = None;
                                                    this.select_source(
                                                        Some(id.clone()),
                                                        window,
                                                        cx,
                                                    );
                                                } else {
                                                    this.server_view(View::Login, window, cx);
                                                    if let Some(panel) = &this.servers {
                                                        panel.username.update(cx, |input, cx| {
                                                            input.set_value(username.clone(), cx)
                                                        });
                                                        panel
                                                            .password
                                                            .focus_handle(cx)
                                                            .focus(window, cx);
                                                    }
                                                }
                                            })),
                                    )
                                    .when(authenticated, |el| {
                                        el.child(
                                            button(("account-settings", index), "")
                                                .child(icon(LucideIcons::Settings, 16.))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.open_account(
                                                            settings.clone(),
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                )),
                                        )
                                        .child(
                                            button(("account-logout", index), t("server-sign-out"))
                                                .text_size(px(12.))
                                                .text_color(rgb(MUTED))
                                                .opacity(if self.signing_out { 0.5 } else { 1. })
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.server_logout(
                                                            logout.clone(),
                                                            window,
                                                            cx,
                                                        )
                                                    },
                                                )),
                                        )
                                    })
                            })),
                    )
                    .child(
                        primary("server-add-account", t("server-add-account"), false).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.server_view(View::Login, window, cx)
                            }),
                        ),
                    )
            }
            View::Login => self.server_login_form(cx),
            View::Settings => self.account_settings(cx),
        };
        gpui::deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(rgba(0x00000077))
                .p(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .id("server-dialog")
                        .debug_selector(|| "server-dialog".into())
                        .w(px(480.))
                        .max_w_full()
                        .max_h_full()
                        .overflow_y_scroll()
                        .p(px(24.))
                        .rounded(px(16.))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(PANEL))
                        .shadow_xl()
                        .flex()
                        .flex_col()
                        .gap(px(24.))
                        .text_size(px(13.))
                        .text_color(rgb(TEXT))
                        .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                        .child(header)
                        .child(content)
                        .when_some(panel.error.clone(), |el, error| {
                            el.child(
                                div()
                                    .p(px(12.))
                                    .rounded(px(8.))
                                    .bg(rgba(0xf08e8311))
                                    .text_color(rgb(0xf08e83))
                                    .child(error),
                            )
                        }),
                ),
        )
        .with_priority(30)
    }
}

impl Studio {
    pub(super) fn server_login_form(&self, cx: &mut Context<Self>) -> gpui::Div {
        let panel = self.servers.as_ref().unwrap();
        let field = |id, label, input: &Entity<TextInput>| field(id, label, input, cx);
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .when(
                panel.mode != "login"
                    && panel
                        .registration
                        .as_ref()
                        .is_some_and(|p| p.personal && p.teams),
                |el| {
                    el.child(
                        div()
                            .flex()
                            .gap(px(4.))
                            .p(px(4.))
                            .rounded(px(9.))
                            .bg(rgb(0x16161e))
                            .children(
                                [
                                    ("personal", "server-register-personal"),
                                    ("team", "server-register-team"),
                                ]
                                .into_iter()
                                .map(|(mode, label)| {
                                    button(mode, t(label))
                                        .flex_1()
                                        .justify_center()
                                        .when(panel.mode == mode, |el| {
                                            el.bg(rgb(0x352e45)).text_color(rgb(ACCENT))
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(panel) = &mut this.servers
                                                && !panel.busy
                                            {
                                                panel.mode = mode;
                                                panel.error = None;
                                                cx.notify();
                                            }
                                        }))
                                }),
                            ),
                    )
                },
            )
            .child(field("server-user", t("server-username"), &panel.username))
            .child(self.password_field("server-password", t("server-password"), 0, cx))
            .when(panel.mode == "team", |el| {
                el.child(field("server-team", t("space-team-name"), &panel.team_name))
            })
            .child(
                primary(
                    "server-login",
                    t(if panel.busy {
                        "server-connecting"
                    } else if panel.mode == "login" {
                        "server-sign-in"
                    } else {
                        "server-register"
                    }),
                    panel.busy,
                )
                .h(px(44.))
                .rounded(px(9.))
                .mt(px(4.))
                .on_click(cx.listener(|this, _, window, cx| this.server_login(window, cx))),
            )
            .when(
                panel
                    .registration
                    .as_ref()
                    .is_some_and(|p| p.personal || p.teams),
                |el| {
                    el.child(
                        div()
                            .pt(px(18.))
                            .border_t_1()
                            .border_color(rgb(BORDER))
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_center()
                            .gap(px(4.))
                            .text_size(px(12.))
                            .child(div().text_color(rgb(MUTED)).child(t(
                                if panel.mode == "login" {
                                    "server-new-account"
                                } else {
                                    "server-existing-account"
                                },
                            )))
                            .child(
                                button(
                                    "account-mode",
                                    t(if panel.mode == "login" {
                                        "server-register"
                                    } else {
                                        "server-sign-in"
                                    }),
                                )
                                .justify_center()
                                .px(px(4.))
                                .py(px(2.))
                                .rounded(px(5.))
                                .text_color(rgb(ACCENT))
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.registration_options(window, cx),
                                )),
                            ),
                    )
                },
            )
    }
}
