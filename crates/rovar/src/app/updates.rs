use super::*;
mod release;
use release::Release;

#[derive(Default)]
struct State {
    startup_attempted: bool,
    checking: bool,
    checked: bool,
    manual: bool,
    window: u64,
    available: Option<Release>,
    error: Option<String>,
}
impl gpui::Global for State {}

impl State {
    fn begin(&mut self, window: u64, manual: bool) -> bool {
        if !manual && self.startup_attempted {
            return false;
        }
        if !manual {
            self.startup_attempted = true;
        }
        if self.checking {
            if manual {
                self.manual = true;
                self.window = window;
            }
            return false;
        }
        self.checking = true;
        self.manual = manual;
        self.window = window;
        self.error = None;
        true
    }
    fn finish(
        &mut self,
        result: Result<Option<Release>, String>,
        ignored: Option<&str>,
    ) -> Option<Release> {
        self.checking = false;
        self.checked = true;
        match result {
            Ok(available) => {
                self.available = available;
                self.error = None;
            }
            Err(error) => {
                self.available = None;
                self.error = Some(error);
            }
        }
        self.available
            .as_ref()
            .filter(|r| self.manual || ignored != Some(r.version.as_str()))
            .cloned()
    }
}

pub(super) struct Prompt {
    release: Release,
    focus: FocusHandle,
    previous: Option<FocusHandle>,
    error: Option<String>,
}

impl Studio {
    pub(super) fn check_updates(&mut self, manual: bool, cx: &mut Context<Self>) {
        if !cx.has_global::<State>() {
            cx.set_global(State::default());
        }
        if !cx.global_mut::<State>().begin(self.window_id, manual) {
            return;
        }
        cx.refresh_windows();
        let task = cx
            .background_executor()
            .spawn(async { release::check().map_err(|error| format!("{error:#}")) });
        cx.spawn(async move |_, cx| {
            let result = task.await;
            if let Err(error) = &result {
                eprintln!("Could not check for Rovar updates: {error}");
            }
            cx.update(|cx| {
                let config = crate::settings::read(crate::settings::path().as_deref()).ok();
                let ignored = config
                    .as_ref()
                    .and_then(|c| c["ignored_update_version"].as_str());
                let state = cx.global_mut::<State>();
                let release = state.finish(result, ignored);
                let target = state.window;
                if let Some(release) = release {
                    let windows: Vec<_> = cx
                        .windows()
                        .into_iter()
                        .filter_map(|w| w.downcast::<Studio>())
                        .collect();
                    if let Some(handle) = windows
                        .iter()
                        .find(|w| w.window_id().as_u64() == target)
                        .or(windows.first())
                    {
                        let _ = handle.update(cx, |studio, window, cx| {
                            if studio.closing {
                                return;
                            }
                            let previous = window.focused(cx);
                            if let Some(editor) = studio.active_editor() {
                                editor.update(cx, |editor, cx| editor.suspend(window, cx));
                            }
                            let focus = cx.focus_handle();
                            focus.focus(window, cx);
                            studio.update_prompt = Some(Prompt {
                                release,
                                focus,
                                previous,
                                error: None,
                            });
                            cx.notify();
                        });
                    }
                }
                cx.refresh_windows();
            });
        })
        .detach();
    }

    pub(super) fn dismiss_update(
        &mut self,
        ignore: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(prompt) = &mut self.update_prompt else {
            return;
        };
        if ignore
            && let Err(error) = crate::settings::set(
                "ignored_update_version",
                serde_json::json!(prompt.release.version),
            )
        {
            prompt.error = Some(error.to_string());
            cx.notify();
            return;
        }
        let prompt = self.update_prompt.take().unwrap();
        prompt
            .previous
            .unwrap_or_else(|| self.focus.clone())
            .focus(window, cx);
        cx.notify();
    }

    pub(super) fn update_settings(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = cx.try_global::<State>();
        let checking = state.is_some_and(|s| s.checking);
        let available = state.and_then(|s| s.available.as_ref());
        let message = if checking {
            t("updates-checking").to_owned()
        } else if let Some(release) = available {
            crate::i18n::message("updates-available", &[("version", release.version.clone())])
        } else if state.is_some_and(|s| s.error.is_some()) {
            t("updates-failed").to_owned()
        } else if state.is_some_and(|s| s.checked) {
            t("updates-current").to_owned()
        } else {
            t("updates-hint").to_owned()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(20.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(t("updates-title"))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(MUTED.color())
                                    .child(message),
                            ),
                    )
                    .child(
                        button("check-updates", t("updates-check"), false)
                            .when(checking, |el| el.opacity(0.5))
                            .when(!checking, |el| {
                                el.on_click(
                                    cx.listener(|this, _, _, cx| this.check_updates(true, cx)),
                                )
                            }),
                    ),
            )
            .when_some(available.cloned(), |el, release| {
                el.child(
                    div()
                        .id("release-download")
                        .text_size(px(12.))
                        .text_color(ACCENT.color())
                        .cursor_pointer()
                        .child(t("updates-download"))
                        .on_click(move |_, _, cx| cx.open_url(&release.url)),
                )
            })
    }

    pub(super) fn update_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let prompt = self.update_prompt.as_ref().unwrap();
        let url = prompt.release.url.clone();
        gpui::deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(Color::Overlay.color())
                .flex()
                .items_center()
                .justify_center()
                .p(px(24.))
                .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("update-dialog")
                        .track_focus(&prompt.focus)
                        .w(px(460.))
                        .max_w_full()
                        .p(px(28.))
                        .rounded(px(18.))
                        .bg(Color::Panel.color())
                        .border_1()
                        .border_color(BORDER.color())
                        .shadow_xl()
                        .flex()
                        .flex_col()
                        .gap(px(20.))
                        .child(
                            div()
                                .size(px(44.))
                                .rounded(px(12.))
                                .bg(Color::Selected.color())
                                .text_color(ACCENT.color())
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(icon(GpuiIcons::LucideDownload, 22.)),
                        )
                        .child(
                            div()
                                .text_size(px(20.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(crate::i18n::message(
                                    "updates-available",
                                    &[("version", prompt.release.version.clone())],
                                )),
                        )
                        .child(div().text_size(px(13.)).text_color(MUTED.color()).child(
                            crate::i18n::message(
                                "updates-dialog-hint",
                                &[("version", env!("CARGO_PKG_VERSION").into())],
                            ),
                        ))
                        .when_some(prompt.error.clone(), |el, error| {
                            el.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(Color::Danger.color())
                                    .child(error),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(8.))
                                .child(button("update-later", t("updates-later"), false).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.dismiss_update(false, window, cx)
                                    }),
                                ))
                                .child(
                                    button("update-ignore", t("updates-ignore"), false).on_click(
                                        cx.listener(|this, _, window, cx| {
                                            this.dismiss_update(true, window, cx)
                                        }),
                                    ),
                                )
                                .child(
                                    button("update-download", t("updates-download"), true)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            cx.open_url(&url);
                                            this.dismiss_update(false, window, cx);
                                        })),
                                ),
                        ),
                ),
        )
        .with_priority(60)
    }
}

fn button(id: &'static str, label: &'static str, primary: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_shrink_0()
        .h(px(34.))
        .px(px(14.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.))
        .cursor_pointer()
        .bg(if primary {
            Color::Selected.color()
        } else {
            Color::Input.color()
        })
        .text_color(if primary {
            ACCENT.color()
        } else {
            TEXT.color()
        })
        .hover(|s| s.bg(Color::Hover.color()))
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_checks_once_and_manual_requests_override_ignored_versions_without_duplicate_requests()
     {
        let release = Release {
            version: "0.4.0".into(),
            url: release::RELEASES.into(),
        };
        let mut state = State::default();
        assert!(state.begin(1, false));
        assert!(!state.begin(2, false));
        assert!(
            state
                .finish(Ok(Some(release.clone())), Some("0.4.0"))
                .is_none()
        );
        assert!(!state.begin(2, false));
        assert!(state.begin(2, true));
        assert_eq!(
            state.finish(Ok(Some(release.clone())), Some("0.4.0")),
            Some(release.clone())
        );
        let mut state = State::default();
        assert!(state.begin(1, false));
        assert!(!state.begin(2, true));
        assert_eq!(state.window, 2);
        assert_eq!(
            state.finish(Ok(Some(release.clone())), Some("0.4.0")),
            Some(release.clone())
        );
        let mut state = State::default();
        assert!(state.begin(1, false));
        assert_eq!(
            state
                .finish(Ok(Some(release)), Some("0.3.1"))
                .unwrap()
                .version,
            "0.4.0"
        );
        assert!(!state.begin(1, false));
        assert!(state.begin(1, true));
        assert!(state.finish(Err("offline".into()), None).is_none());
        assert!(!state.checking);
        assert!(state.error.is_some());
    }
}
