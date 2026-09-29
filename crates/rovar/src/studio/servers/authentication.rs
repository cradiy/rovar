use super::*;

impl Studio {
    pub(super) fn registration_options(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.servers else {
            return;
        };
        if panel.busy {
            return;
        }
        if panel.mode != "login" {
            panel.mode = "login";
        } else if let Some(policy) = &panel.registration {
            panel.mode = if policy.personal {
                "personal"
            } else if policy.teams {
                "team"
            } else {
                "login"
            };
        }
        panel.error = None;
        panel.username.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(super) fn server_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.servers else {
            return;
        };
        if panel.busy || panel.view != View::Login {
            return;
        }
        if panel.username.read(cx).value().trim().is_empty() {
            panel.username.focus_handle(cx).focus(window, cx);
            return;
        }
        if panel.password.read(cx).value().is_empty() {
            panel.password.focus_handle(cx).focus(window, cx);
            return;
        }
        if panel.mode != "login" && panel.password.read(cx).value().chars().count() < 6 {
            panel.error = Some(t("server-password-short").into());
            panel.password.focus_handle(cx).focus(window, cx);
            cx.notify();
            return;
        }
        if panel.mode == "team" && panel.team_name.read(cx).value().trim().is_empty() {
            panel.error = Some(t("server-team-required").into());
            panel.team_name.focus_handle(cx).focus(window, cx);
            cx.notify();
            return;
        }
        let client = match Client::new(panel.selected.as_deref().unwrap_or_default()) {
            Ok(client) => client,
            Err(error) => {
                panel.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let request = panel.id;
        let endpoint = if panel.mode == "login" {
            "login"
        } else {
            "register"
        };
        let credentials = serde_json::json!({
            "username": panel.username.read(cx).value().trim(),
            "password": panel.password.read(cx).value().to_string(),
            "team_name": if panel.mode == "team" { Some(panel.team_name.read(cx).value().trim().to_owned()) } else { None }
        });
        panel.busy = true;
        panel.error = None;
        panel
            .password
            .update(cx, |input, cx| input.set_value("", cx));
        cx.spawn_in(window, async move |this, cx| {
            let result = client
                .json::<rovar_api::Login>("POST", endpoint, Some(credentials))
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.servers.as_ref().is_some_and(|p| p.id == request) {
                    return;
                }
                match result {
                    Ok(login) => {
                        let token = login.token.clone();
                        let username = login.identity.username.clone();
                        match this.remote.update(cx, |remote, cx| {
                            remote.connect(
                                client.url,
                                login.identity,
                                if cfg!(target_family = "wasm") {
                                    String::new()
                                } else {
                                    login.token
                                },
                                cx,
                            )
                        }) {
                            Ok(id) => {
                                #[cfg(not(target_family = "wasm"))]
                                {
                                    cx.write_credentials(
                                        &format!("rovar-server/{id}"),
                                        &username,
                                        token.as_bytes(),
                                    )
                                    .detach();
                                }
                                let _ = (token, username);
                                this.servers = None;
                                this.select_source(Some(id), window, cx);
                            }
                            Err(error) => {
                                if let Some(panel) = &mut this.servers {
                                    panel.busy = false;
                                    panel.error = Some(error.to_string());
                                }
                            }
                        }
                    }
                    Err(error) => {
                        if let Some(panel) = &mut this.servers {
                            panel.busy = false;
                            panel.error = Some(
                                match error
                                    .downcast_ref::<crate::remote::HttpError>()
                                    .map(|error| error.status)
                                {
                                    Some(401) => t("server-login-invalid").into(),
                                    Some(429) => t("server-login-rate-limited").into(),
                                    _ => error.to_string(),
                                },
                            );
                            panel.password.focus_handle(cx).focus(window, cx);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::studio) fn server_logout(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.signing_out {
            return;
        }
        let Some(connection) = self.remote.read(cx).connection(&id) else {
            return;
        };
        let client = connection.client();
        #[cfg(not(target_family = "wasm"))]
        let credential_ids = self
            .remote
            .read(cx)
            .connections()
            .iter()
            .filter(|c| {
                c.url == connection.url && c.identity.user_id == connection.identity.user_id
            })
            .map(|c| c.id.clone())
            .collect::<Vec<_>>();
        #[cfg(target_family = "wasm")]
        let libraries = {
            let ids = self
                .tabs
                .iter()
                .filter_map(|tab| {
                    self.remote
                        .read(cx)
                        .link(&tab.file.path)
                        .map(|link| link.connection.clone())
                })
                .chain(self.source.clone())
                .collect::<std::collections::BTreeSet<_>>();
            ids.into_iter()
                .map(|id| self.source_library(Some(id), cx))
                .collect::<Vec<_>>()
        };
        self.signing_out = true;
        #[cfg(target_family = "wasm")]
        {
            self.servers = None;
            for token in self.tabs.iter().map(|tab| tab.token).collect::<Vec<_>>() {
                self.close_tab(token, window, cx);
            }
        }
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            #[cfg(target_family = "wasm")]
            {
                let deadline = web_time::Instant::now() + Duration::from_secs(30);
                loop {
                    let ready = this.update(cx, |this, cx| {
                        if this.tabs.is_empty()
                            && libraries.iter().all(|library| !library.read(cx).busy)
                        {
                            return Ok(true);
                        }
                        if this.tabs.iter().any(|tab| tab.error.is_some())
                            || web_time::Instant::now() >= deadline
                        {
                            this.signing_out = false;
                            this.error = Some(t("server-sign-out-save-error").into());
                            cx.notify();
                            return Err(());
                        }
                        Ok(false)
                    });
                    match ready {
                        Ok(Ok(true)) => break,
                        Ok(Ok(false)) => {
                            cx.background_executor()
                                .timer(Duration::from_millis(100))
                                .await
                        }
                        _ => return,
                    }
                }
                if !crate::web::flush_for_navigation().await {
                    let _ = this.update(cx, |this, cx| {
                        this.signing_out = false;
                        this.error = Some(t("server-sign-out-save-error").into());
                        cx.notify();
                    });
                    return;
                }
            }
            let result = client.logout().await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.signing_out = false;
                match result {
                    Ok(_) => {
                        let active_account = this
                            .source
                            .as_ref()
                            .and_then(|source| this.remote.read(cx).connection(source))
                            .zip(this.remote.read(cx).connection(&id))
                            .is_some_and(|(active, account)| {
                                active.url == account.url
                                    && active.identity.user_id == account.identity.user_id
                            });
                        this.remote
                            .update(cx, |remote, cx| remote.sign_out(&id, cx));
                        #[cfg(not(target_family = "wasm"))]
                        for id in credential_ids {
                            cx.delete_credentials(&format!("rovar-server/{id}"))
                                .detach();
                        }
                        if active_account {
                            this.select_source(None, window, cx);
                        }
                        #[cfg(target_family = "wasm")]
                        this.initialize_web_login(window, cx);
                        let _ = window;
                    }
                    Err(error) => {
                        if let Some(panel) = &mut this.servers {
                            panel.error = Some(error.to_string());
                        } else {
                            this.error = Some(error.to_string());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
