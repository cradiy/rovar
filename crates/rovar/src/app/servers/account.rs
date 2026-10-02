use super::*;
const CARD: u32 = 0x24232e;

enum Action {
    Load,
    Password,
    Revoke(Option<String>),
}

impl Studio {
    pub(super) fn save_account_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.account_action(Action::Password, window, cx);
    }
    pub(in crate::app) fn open_account(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(connection) = self.remote.read(cx).connection(&id).cloned() else {
            return;
        };
        if !connection.authenticated {
            self.reauthenticate(id, window, cx);
            return;
        }
        self.open_servers(None, window, cx);
        let panel = self.servers.as_mut().unwrap();
        panel.view = View::Settings;
        panel.mode = "password";
        panel.selected = Some(connection.url);
        panel.account = Some(id);
        self.account_action(Action::Load, window, cx);
    }

    pub(in crate::app) fn reauthenticate(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(connection) = self.remote.read(cx).connection(&id).cloned() else {
            return;
        };
        self.open_servers(None, window, cx);
        let panel = self.servers.as_mut().unwrap();
        panel.selected = Some(connection.url);
        panel.view = View::Login;
        panel.resume = Some(id);
        panel.username.update(cx, |input, cx| {
            input.set_value(connection.identity.username, cx)
        });
        panel.password.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn account_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.servers.as_mut() else {
            return;
        };
        if panel.busy || panel.view != View::Settings {
            return;
        }
        let Some(connection) = panel
            .account
            .as_deref()
            .and_then(|id| self.remote.read(cx).connection(id))
            .cloned()
        else {
            return;
        };
        panel.error = None;
        panel.success = None;
        let (method, path, body, message) = match action {
            Action::Load => ("GET", "account/sessions".to_owned(), None, None),
            Action::Password => {
                let next = panel.new_password.read(cx).value().to_string();
                let current = panel.password.read(cx).value().to_string();
                if next.chars().count() < 6 {
                    panel.error = Some(t("server-password-short").into());
                    cx.notify();
                    return;
                }
                if next != panel.confirm_password.read(cx).value().as_ref() {
                    panel.error = Some(t("account-password-mismatch").into());
                    cx.notify();
                    return;
                }
                if current.is_empty() {
                    panel.password.focus_handle(cx).focus(window, cx);
                    return;
                }
                (
                    "PUT",
                    "account/password".into(),
                    Some(serde_json::json!({"current_password":current,"new_password":next})),
                    Some("account-password-updated"),
                )
            }
            Action::Revoke(id) => (
                "DELETE",
                id.map(|id| format!("account/sessions/{id}"))
                    .unwrap_or_else(|| "account/sessions".into()),
                None,
                Some("account-sessions-revoked"),
            ),
        };
        let request = panel.id;
        panel.busy = true;
        let client = connection.client();
        cx.spawn_in(window, async move |this, cx| {
            // Preserve successful mutation feedback even if the following refresh fails.
            let result = client.json::<serde_json::Value>(method, &path, body).await;
            let changed = result.is_ok() && method != "GET";
            let sessions = match result {
                Ok(value) if method == "GET" => {
                    serde_json::from_value::<Vec<rovar_api::AccountSession>>(value)
                        .map_err(Into::into)
                }
                Ok(_) => client.json("GET", "account/sessions", None).await,
                Err(error) => Err(error),
            };
            let _ = this.update_in(cx, |this, _, cx| {
                let Some(panel) = this.servers.as_mut().filter(|p| p.id == request) else {
                    return;
                };
                panel.busy = false;
                if changed {
                    panel.success = message.map(|key| t(key).into());
                    for field in [
                        &panel.password,
                        &panel.new_password,
                        &panel.confirm_password,
                    ] {
                        field.update(cx, |input, cx| input.set_value("", cx));
                    }
                }
                match sessions {
                    Ok(items) => panel.sessions = items,
                    Err(error) => {
                        if this
                            .remote
                            .read(cx)
                            .connection(&connection.id)
                            .is_some_and(|c| c.generation == connection.generation)
                            && error
                                .downcast_ref::<crate::remote::HttpError>()
                                .is_some_and(|e| e.status == 401)
                        {
                            this.remote
                                .update(cx, |remote, cx| remote.sign_out(&connection.id, cx));
                        }
                        panel.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn account_settings(&self, cx: &mut Context<Self>) -> gpui::Div {
        let panel = self.servers.as_ref().unwrap();
        let username = panel
            .account
            .as_deref()
            .and_then(|id| self.remote.read(cx).connection(id))
            .map(|c| c.identity.username.clone())
            .unwrap_or_default();
        let password = div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(self.password_field(
                "account-current-password",
                t("account-current-password"),
                0,
                cx,
            ))
            .child(self.password_field("account-new-password", t("account-new-password"), 1, cx))
            .child(self.password_field(
                "account-confirm-password",
                t("account-confirm-password"),
                2,
                cx,
            ))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(t("account-password-hint")),
            )
            .child(
                view::primary(
                    "account-save-password",
                    t("account-change-password"),
                    panel.busy,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.account_action(Action::Password, window, cx)
                })),
            );
        let sessions = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(t("account-sessions"))
                    .child(
                        button("account-refresh", t("server-refresh"))
                            .text_size(px(12.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.account_action(Action::Load, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .id("account-session-list")
                    .max_h(px(280.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .children(panel.sessions.iter().enumerate().map(|(index, session)| {
                        let id = session.id.clone();
                        let created = time::OffsetDateTime::from_unix_timestamp(session.created_at)
                            .map(|date| {
                                format!(
                                    "{}-{:02}-{:02} {:02}:{:02} UTC",
                                    date.year(),
                                    u8::from(date.month()),
                                    date.day(),
                                    date.hour(),
                                    date.minute()
                                )
                            })
                            .unwrap_or_default();
                        div()
                            .p(px(12.))
                            .rounded(px(9.))
                            .bg(rgb(CARD))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(icon(LucideIcons::Monitor, 18.).text_color(rgb(MUTED)))
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.))
                                    .child(t(if session.current {
                                        "account-current-session"
                                    } else {
                                        "account-other-session"
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(MUTED))
                                            .child(created),
                                    ),
                            )
                            .when(!session.current, |el| {
                                el.child(
                                    button(("revoke-session", index), t("server-sign-out"))
                                        .text_size(px(12.))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.account_action(
                                                Action::Revoke(Some(id.clone())),
                                                window,
                                                cx,
                                            )
                                        })),
                                )
                            })
                    })),
            )
            .when(panel.sessions.iter().any(|s| !s.current), |el| {
                el.child(
                    button("account-revoke-others", t("account-revoke-others"))
                        .text_color(rgb(ACCENT))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.account_action(Action::Revoke(None), window, cx)
                        })),
                )
            });
        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icon(LucideIcons::User, 18.).text_color(rgb(ACCENT)))
                    .child(div().flex_1().min_w_0().truncate().child(username)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .p(px(4.))
                    .rounded(px(9.))
                    .bg(rgb(0x17171f))
                    .children(
                        [
                            ("password", "account-change-password"),
                            ("sessions", "account-sessions"),
                        ]
                        .into_iter()
                        .map(|(tab, label)| {
                            button(tab, t(label))
                                .flex_1()
                                .justify_center()
                                .bg(rgb(if panel.mode == tab {
                                    0x352e45
                                } else {
                                    0x17171f
                                }))
                                .text_color(rgb(if panel.mode == tab { ACCENT } else { MUTED }))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(panel) = this.servers.as_mut().filter(|p| !p.busy) {
                                        panel.mode = tab;
                                        panel.error = None;
                                        panel.success = None;
                                        cx.notify();
                                    }
                                }))
                        }),
                    ),
            )
            .when_some(panel.success.clone(), |el, message| {
                el.child(
                    div()
                        .p(px(12.))
                        .rounded(px(8.))
                        .bg(rgba(0x98c6ad15))
                        .text_color(rgb(0x98c6ad))
                        .child(message),
                )
            })
            .child(if panel.mode == "sessions" {
                sessions
            } else {
                password
            })
    }
}
