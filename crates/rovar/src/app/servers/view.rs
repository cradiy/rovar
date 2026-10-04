use super::*;
use crate::ui::theme::Color;
use gpui::{FontWeight, MouseButton};
use uic::components::input::Input;

const CARD: Color = Color::Surface;

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
        .bg(ACCENT.color())
        .text_color(Color::OnAccent.color())
        .hover(|s| s.bg(Color::Accent.color()))
        .opacity(if busy { 0.5 } else { 1. })
}

fn tile(symbol: LucideIcons) -> gpui::Div {
    div()
        .size(px(38.))
        .flex_shrink_0()
        .rounded(px(10.))
        .bg(Color::Selected.color())
        .flex()
        .items_center()
        .justify_center()
        .child(icon(symbol, 19.).text_color(ACCENT.color()))
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
                .text_color(Color::Text.color())
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
                    .bg(Color::Surface.color())
                    .text_color(TEXT.color())
                    .border_color(BORDER.color())
                    .rounded(px(9.))
                    .appearance(crate::ui::theme::input_appearance()),
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
            .hover(|s| s.bg(Color::Text.color().opacity(0.0510)))
            .child(
                icon(
                    if visible {
                        LucideIcons::EyeOff
                    } else {
                        LucideIcons::Eye
                    },
                    17.,
                )
                .text_color(if visible {
                    ACCENT.color()
                } else {
                    MUTED.color()
                }),
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
                            .text_color(MUTED.color())
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
                                            .border_color(if active {
                                                Color::Muted.color()
                                            } else {
                                                BORDER.color()
                                            })
                                            .bg(CARD.color())
                                            .cursor_pointer()
                                            .hover(|s| s.bg(Color::Hover.color()))
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
                                                            .text_color(MUTED.color())
                                                            .truncate()
                                                            .child(url.clone()),
                                                    ),
                                            )
                                            .when(active, |el| {
                                                el.child(
                                                    icon(LucideIcons::Check, 15.)
                                                        .text_color(ACCENT.color()),
                                                )
                                            })
                                            .child(
                                                div().flex().items_center().gap(px(2.))
                                                .pl(px(8.)).border_l_1().border_color(BORDER.color())
                                                .child(
                                                button(("rename-server", index), "")
                                                    .debug_selector(move || format!("rename-server-{index}"))
                                                    .size(px(30.))
                                                    .p_0()
                                                    .justify_center()
                                                    .child(
                                                        icon(LucideIcons::Pencil, 15.)
                                                            .text_color(MUTED.color()),
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
                                                    .debug_selector(move || format!("remove-server-{index}"))
                                                    .child(
                                                        icon(LucideIcons::Trash2, 15.)
                                                            .text_color(MUTED.color()),
                                                    )
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            cx.stop_propagation();
                                                            this.server_action = Some(super::PendingAction::Remove(remove.clone()));
                                                            this.focus.focus(window, cx);
                                                            cx.notify();
                                                        },
                                                    )),
                                                ),
                                            )
                                            .child(
                                                icon(LucideIcons::ChevronRight, 16.)
                                                    .text_color(MUTED.color()),
                                            )
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.select_server(address.clone(), window, cx)
                                            }))
                                    },
                                )),
                        )
                    })
                    .when(!remote.servers().is_empty() && !panel.adding_server, |el| {
                        el.child(button("server-add", "")
                            .debug_selector(|| "server-add".into())
                            .w_full().h(px(38.)).gap(px(8.)).justify_center().border_1().border_color(BORDER.color())
                            .child(icon(LucideIcons::Plus, 16.).text_color(ACCENT.color()))
                            .child(t("server-add"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(panel) = &mut this.servers {
                                    panel.adding_server = true;
                                    panel.error = None;
                                    panel.name.focus_handle(cx).focus(window, cx);
                                    cx.notify();
                                }
                            })))
                    })
                    .when(remote.servers().is_empty() || panel.adding_server, |el| { el.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .when(!remote.servers().is_empty(), |el| {
                                el.pt(px(18.)).border_t_1().border_color(BORDER.color())
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
                    ) })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .text_color(MUTED.color())
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
                        .text_color(MUTED.color())
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
                                        .text_color(MUTED.color())
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
                                let expanded = authenticated
                                    && panel.expanded_account.as_ref() == Some(&id);
                                let spaces = self.remote.read(cx).connections().iter()
                                    .filter(|connection| {
                                        connection.authenticated
                                            && connection.url == account.url
                                            && connection.identity.server_id == account.identity.server_id
                                            && connection.identity.user_id == account.identity.user_id
                                    })
                                    .cloned()
                                    .collect::<Vec<_>>();
                                let header = div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .p(px(8.))
                                    .rounded(px(11.))
                                    .bg(CARD.color())
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
                                            .hover(|s| s.bg(Color::Hover.color()))
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
                                                            .text_color(if authenticated {
                                                                ACCENT.color()
                                                            } else {
                                                                MUTED.color()
                                                            })
                                                            .child(t(if authenticated {
                                                                "server-workspaces"
                                                            } else {
                                                                "server-sign-in"
                                                            })),
                                                    ),
                                            )
                                            .child(
                                                icon(if expanded { LucideIcons::ChevronDown } else { LucideIcons::ChevronRight }, 16.)
                                                    .text_color(MUTED.color()),
                                            )
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                if this.signing_out {
                                                    return;
                                                }
                                                if authenticated {
                                                    if let Some(panel) = &mut this.servers {
                                                        panel.expanded_account = if expanded { None } else { Some(id.clone()) };
                                                    }
                                                    cx.notify();
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
                                                .text_color(MUTED.color())
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
                                    });
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_shrink_0()
                                    .rounded(px(11.))
                                    .bg(CARD.color())
                                    .child(header)
                                    .when(expanded, |el| {
                                        el.child(
                                            div()
                                                .px(px(8.))
                                                .pb(px(8.))
                                                .flex()
                                                .flex_col()
                                                .gap(px(4.))
                                                .children(spaces.into_iter().enumerate().map(|(space_index, connection)| {
                                                    let selected = self.source.as_ref() == Some(&connection.id);
                                                    div()
                                                        .id(SharedString::from(format!("account-workspace-{}", connection.id)))
                                                        .debug_selector(move || format!("account-workspace-{index}-{space_index}"))
                                                        .min_h(px(36.))
                                                        .px(px(12.))
                                                        .rounded(px(7.))
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(10.))
                                                        .cursor_pointer()
                                                        .hover(|s| s.bg(Color::Hover.color()))
                                                        .child(icon(if connection.space.kind == "team" { LucideIcons::Users } else { LucideIcons::House }, 16.).text_color(MUTED.color()))
                                                        .child(div().flex_1().min_w_0().truncate().child(connection.space_label()))
                                                        .when(selected, |el| el.child(icon(LucideIcons::Check, 14.).text_color(ACCENT.color())))
                                                        .on_click(cx.listener(move |this, _, window, cx| {
                                                            if this.signing_out { return; }
                                                            this.servers = None;
                                                            this.select_source(Some(connection.id.clone()), window, cx);
                                                        }))
                                                })),
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
                .bg(Color::Overlay.color())
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
                        .border_color(BORDER.color())
                        .bg(PANEL.color())
                        .shadow_xl()
                        .flex()
                        .flex_col()
                        .gap(px(24.))
                        .text_size(px(13.))
                        .text_color(TEXT.color())
                        .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                        .child(header)
                        .child(content)
                        .when_some(panel.error.clone(), |el, error| {
                            el.child(
                                div()
                                    .p(px(12.))
                                    .rounded(px(8.))
                                    .bg(Color::Danger.color().opacity(0.0667))
                                    .text_color(Color::Danger.color())
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
                            .bg(Color::Workspace.color())
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
                                            el.bg(Color::Selected.color())
                                                .text_color(ACCENT.color())
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
                            .border_color(BORDER.color())
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_center()
                            .gap(px(4.))
                            .text_size(px(12.))
                            .child(div().text_color(MUTED.color()).child(t(
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
                                .text_color(ACCENT.color())
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.registration_options(window, cx),
                                )),
                            ),
                    )
                },
            )
    }
}
