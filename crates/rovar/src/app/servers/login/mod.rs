use super::*;
use crate::ui::theme::Color;
pub(super) mod silk;

impl Studio {
    #[cfg(target_family = "wasm")]
    pub(super) fn initialize_web_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_servers(None, window, cx);
        let panel = self.servers.as_mut().unwrap();
        let url = crate::remote::browser_url();
        panel.selected = Some(url.clone());
        panel.view = View::Login;
        panel.busy = true;
        let request = panel.id;
        panel.username.focus_handle(cx).focus(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let client = Client::new(&url)?;
                let info: rovar_api::ServerInfo = client.json("GET", "info", None).await?;
                anyhow::ensure!(
                    info.api_version == rovar_api::VERSION,
                    "Unsupported server API version"
                );
                let identity = match client
                    .json::<rovar_api::Identity>("GET", "session", None)
                    .await
                {
                    Ok(identity) => Some(identity),
                    Err(error)
                        if error
                            .downcast_ref::<crate::remote::HttpError>()
                            .is_some_and(|e| e.status == 401) =>
                    {
                        None
                    }
                    Err(error) => return Err(error),
                };
                Ok::<_, anyhow::Error>((info, identity))
            }
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(panel) = this.servers.as_mut().filter(|p| p.id == request) else {
                    return;
                };
                panel.busy = false;
                match result {
                    Ok((info, identity)) => {
                        panel.registration = Some(info.registration);
                        if let Some(identity) = identity {
                            match this.remote.update(cx, |remote, cx| {
                                remote.connect(url, identity, String::new(), cx)
                            }) {
                                Ok(id) => {
                                    this.servers = None;
                                    this.select_source(Some(id), window, cx);
                                }
                                Err(error) => {
                                    this.servers.as_mut().unwrap().error = Some(error.to_string())
                                }
                            }
                        }
                    }
                    Err(error) => panel.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::app) fn web_login_page(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let panel = self.servers.as_ref().unwrap();
        let height = panel.motion.borrow().height();
        let sheen = panel.motion.borrow().sheen();
        let opacity = panel.motion.borrow_mut().content_opacity(panel.mode);
        div()
            .id("web-login-page")
            .debug_selector(|| "web-login-page".into())
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(Color::Workspace.color())
            .text_color(TEXT.color())
            .font_family(crate::ui::font::family(cx))
            .text_size(px(14.))
            .child(silk::background(panel.motion.clone()))
            .child(
                div()
                    .id("login-scroll")
                    .relative()
                    .size_full()
                    .overflow_y_scroll()
                    .p(px(20.))
                    .flex()
                    .child(
                        div()
                            .debug_selector(|| "login-card".into())
                            .w(px(424.))
                            .max_w_full()
                            .flex_shrink_0()
                            .my_auto()
                            .mx_auto()
                            .when_some(height, |el, height| el.h(px(height + 2.)))
                            .overflow_hidden()
                            .rounded(px(20.))
                            .border_1()
                            .border_color(Color::Accent.color().opacity(0.2000))
                            .bg(gpui::linear_gradient(
                                160. + sheen,
                                gpui::linear_color_stop(
                                    Color::Selected.color().opacity(0.9608),
                                    0.,
                                ),
                                gpui::linear_color_stop(
                                    Color::Workspace.color().opacity(0.9804),
                                    0.6,
                                ),
                            ))
                            .shadow_xl()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .relative()
                                    .w_full()
                                    .flex_shrink_0()
                                    .p(px(32.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(24.))
                                    .child(silk::measure(panel.motion.clone()))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap(px(16.))
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(10.))
                                                    .child(
                                                        gpui::color_svg()
                                                            .path("rovar-mark.svg")
                                                            .size(px(30.)),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(18.))
                                                            .font_weight(gpui::FontWeight::MEDIUM)
                                                            .child("Rovar"),
                                                    ),
                                            )
                                            .child(self.language_control(cx)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(8.))
                                            .child(
                                                div()
                                                    .text_size(px(24.))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .child(t(if panel.mode == "login" {
                                                        "server-welcome"
                                                    } else {
                                                        "server-register"
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .text_color(MUTED.color())
                                                    .child(t(if panel.mode == "login" {
                                                        "server-welcome-subtitle"
                                                    } else {
                                                        "server-register-subtitle"
                                                    })),
                                            ),
                                    )
                                    .child(self.server_login_form(cx).opacity(opacity))
                                    .when_some(panel.error.clone(), |el, error| {
                                        el.child(
                                            div()
                                                .text_size(px(13.))
                                                .text_color(Color::Danger.color())
                                                .child(error),
                                        )
                                    })
                                    .when(!panel.busy && panel.registration.is_none(), |el| {
                                        el.child(
                                            button("login-retry", t("server-login-retry"))
                                                .on_click(cx.listener(|_this, _, _window, _cx| {
                                                    #[cfg(target_family = "wasm")]
                                                    _this.initialize_web_login(_window, _cx);
                                                })),
                                        )
                                    }),
                            ),
                    ),
            )
    }
}
