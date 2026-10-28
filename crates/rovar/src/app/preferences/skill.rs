use super::*;
use crate::agent::skill;

pub(super) struct State {
    token: uuid::Uuid,
    directory: Option<PathBuf>,
    status: Option<skill::Status>,
    busy: bool,
    installed_now: bool,
    verified: bool,
    feedback_task: Option<gpui::Task<()>>,
    error: Option<String>,
}

impl State {
    pub fn new() -> Self {
        let configured = crate::settings::read(crate::settings::path().as_deref())
            .ok()
            .and_then(|config| config["agent_skill_directory"].as_str().map(PathBuf::from));
        Self {
            token: uuid::Uuid::new_v4(),
            directory: configured.or_else(skill::default_directory),
            status: None,
            busy: false,
            installed_now: false,
            verified: false,
            feedback_task: None,
            error: None,
        }
    }
}

impl Studio {
    pub(super) fn refresh_skill(&mut self, install: bool, cx: &mut Context<Self>) {
        self.run_skill(install, false, cx);
    }

    fn run_skill(&mut self, install: bool, feedback: bool, cx: &mut Context<Self>) {
        let Some(panel) = self.preferences.as_mut() else {
            return;
        };
        let state = &mut panel.skill;
        if state.busy {
            return;
        }
        let Some(directory) = state.directory.clone() else {
            return;
        };
        state.busy = true;
        state.feedback_task = None;
        state.verified = false;
        state.error = None;
        let token = state.token;
        let task = cx.background_executor().spawn(async move {
            if install {
                skill::install(&directory)
            } else {
                skill::status(&directory)
            }
            .map_err(|error| format!("{error:#}"))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(panel) = &mut this.preferences
                    && panel.skill.token == token
                {
                    panel.skill.busy = false;
                    match result {
                        Ok(status) => {
                            panel.skill.verified = feedback && status.current;
                            panel.skill.installed_now = install;
                            panel.skill.status = Some(status);
                        }
                        Err(error) => {
                            panel.skill.status = None;
                            panel.skill.error = Some(error);
                        }
                    }
                    if panel.skill.verified {
                        panel.skill.feedback_task = Some(cx.spawn(async move |this, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_secs(2))
                                .await;
                            let _ = this.update(cx, |this, cx| {
                                if let Some(panel) = &mut this.preferences
                                    && panel.skill.token == token
                                {
                                    panel.skill.verified = false;
                                    cx.notify();
                                }
                            });
                        }));
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn choose_skill_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.preferences else {
            return;
        };
        if panel.skill.busy {
            return;
        }
        panel.skill.busy = true;
        let token = panel.skill.token;
        let dialog = crate::platform::prompt_for_paths(
            cx,
            gpui::PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some(t("settings-skill-folder").into()),
            },
        );
        cx.spawn_in(window, async move |this, cx| {
            let result = dialog.await;
            let _ = this.update(cx, |this, cx| {
                let Some(panel) = &mut this.preferences else {
                    return;
                };
                if panel.skill.token != token {
                    return;
                }
                panel.skill.busy = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            if let Err(error) =
                                crate::settings::set("agent_skill_directory", json!(path))
                            {
                                panel.skill.error = Some(error.to_string());
                            } else {
                                panel.skill.directory = Some(path);
                                panel.skill.status = None;
                                this.refresh_skill(false, cx);
                            }
                        }
                    }
                    Ok(Ok(None)) => {}
                    error => panel.skill.error = Some(format!("{error:?}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn skill_panel(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = &self.preferences.as_ref().unwrap().skill;
        let current = state.status.as_ref().is_some_and(|s| s.current);
        let installed = state.status.as_ref().is_some_and(|s| s.installed);
        let status = if state.busy {
            "settings-skill-working"
        } else if state.error.is_some() {
            "settings-skill-error"
        } else if current {
            "settings-skill-current"
        } else if installed {
            "settings-skill-outdated"
        } else {
            "settings-skill-missing"
        };
        let disabled = state.busy || state.directory.is_none();
        div()
            .id("settings-skill")
            .debug_selector(|| "settings-skill".into())
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(
                view::card()
                    .flex()
                    .flex_col()
                    .gap(px(22.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(14.))
                            .child(
                                div()
                                    .size(px(42.))
                                    .flex_shrink_0()
                                    .rounded(px(12.))
                                    .bg(Color::Selected.color())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(ACCENT.color())
                                    .child(icon(GpuiIcons::LucideSparkles, 22.)),
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
                                            .text_size(px(16.))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child("rovar-design"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(MUTED.color())
                                            .child(t("settings-skill-description")),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(if current {
                                ACCENT.color()
                            } else {
                                MUTED.color()
                            })
                            .when(current, |el| el.child(icon(GpuiIcons::LucideCheck, 14.)))
                            .child(t(status)),
                    )
                    .child(div().h(px(1.)).bg(Color::Text.color().opacity(0.06)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(16.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(5.))
                                    .text_size(px(12.))
                                    .text_color(MUTED.color())
                                    .child(format!("Rovar {}", env!("CARGO_PKG_VERSION")))
                                    .when_some(
                                        state.status.as_ref().filter(|s| s.installed && !s.current),
                                        |el, status| {
                                            el.child(i18n::message(
                                                "settings-skill-version",
                                                &[(
                                                    "version",
                                                    status.version.clone().unwrap_or_else(|| {
                                                        t("settings-skill-unknown").into()
                                                    }),
                                                )],
                                            ))
                                        },
                                    ),
                            )
                            .child(
                                div()
                                    .id("install-skill")
                                    .debug_selector(|| "install-skill".into())
                                    .flex_shrink_0()
                                    .px(px(14.))
                                    .h(px(34.))
                                    .rounded(px(8.))
                                    .flex()
                                    .items_center()
                                    .gap(px(7.))
                                    .bg(if current {
                                        Color::Input.color()
                                    } else {
                                        Color::Selected.color()
                                    })
                                    .text_color(if current && !state.verified {
                                        TEXT.color()
                                    } else {
                                        ACCENT.color()
                                    })
                                    .when(current, |el| {
                                        el.child(icon(
                                            if state.verified {
                                                GpuiIcons::LucideCheck
                                            } else {
                                                GpuiIcons::LucideRefreshCw
                                            },
                                            14.,
                                        ))
                                    })
                                    .when(disabled, |el| el.opacity(0.5))
                                    .when(!disabled, |el| {
                                        el.cursor_pointer()
                                            .hover(|s| s.bg(Color::Hover.color()))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.run_skill(!current, current, cx)
                                            }))
                                    })
                                    .child(t(if state.busy {
                                        "settings-skill-working"
                                    } else if state.verified {
                                        "settings-skill-verified"
                                    } else if current {
                                        "settings-skill-check"
                                    } else if installed {
                                        "settings-skill-update"
                                    } else {
                                        "settings-skill-install"
                                    })),
                            ),
                    ),
            )
            .when(state.directory.is_none(), |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(MUTED.color())
                        .child(t("settings-skill-no-directory")),
                )
            })
            .child(
                view::card()
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
                            .child(t("settings-skill-location"))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(MUTED.color())
                                    .child(t("settings-skill-location-description")),
                            ),
                    )
                    .child(
                        div()
                            .id("choose-skill-directory")
                            .flex_shrink_0()
                            .px(px(12.))
                            .h(px(32.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .text_color(MUTED.color())
                            .cursor_pointer()
                            .hover(|s| s.bg(Color::Hover.color()))
                            .child(t("settings-skill-folder"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_skill_directory(window, cx)
                            })),
                    ),
            )
            .when(current && state.installed_now, |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(MUTED.color())
                        .child(t("settings-skill-reload")),
                )
            })
            .when_some(state.error.clone(), |el, error| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(Color::Danger.color())
                        .child(error),
                )
            })
    }
}

use serde_json::json;

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext, size};

    #[gpui::test]
    fn skill_settings_install_detect_changes_and_surface_failures(cx: &mut TestAppContext) {
        cx.update(uic::init);
        let root = tempfile::tempdir().unwrap();
        let window = cx.open_window(size(px(1100.), px(1100.)), |window, cx| {
            Studio::new(root.path().join("documents"), window, cx)
        });
        window
            .update(cx, |this, window, cx| this.open_settings(window, cx))
            .unwrap();
        cx.run_until_parked();
        let directory = root.path().join("skills");
        window
            .update(cx, |this, _, cx| {
                this.preferences.as_mut().unwrap().section = Section::Ai;
                this.preferences.as_mut().unwrap().skill.directory = Some(directory.clone());
                this.refresh_skill(false, cx);
            })
            .unwrap();
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear());
        let button = visual.debug_bounds("install-skill").unwrap();
        visual.simulate_mouse_down(button.center(), gpui::MouseButton::Left, Default::default());
        visual.simulate_mouse_up(button.center(), gpui::MouseButton::Left, Default::default());
        visual.cx.run_until_parked();
        window
            .update(&mut visual.cx, |this, _, _| {
                let state = &this.preferences.as_ref().unwrap().skill;
                assert!(state.status.as_ref().unwrap().current);
                assert!(state.error.is_none());
            })
            .unwrap();
        let path = directory.join("rovar-design/SKILL.md");
        std::fs::write(&path, format!("{}\nLocal changes", skill::bundled())).unwrap();
        window
            .update(&mut visual.cx, |this, _, cx| this.refresh_skill(false, cx))
            .unwrap();
        visual.cx.run_until_parked();
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert!(
                    !this
                        .preferences
                        .as_ref()
                        .unwrap()
                        .skill
                        .status
                        .as_ref()
                        .unwrap()
                        .current
                );
                this.refresh_skill(true, cx);
            })
            .unwrap();
        visual.cx.run_until_parked();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), skill::bundled());
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        window
            .update(&mut visual.cx, |this, _, cx| this.refresh_skill(true, cx))
            .unwrap();
        visual.cx.run_until_parked();
        window
            .update(&mut visual.cx, |this, _, _| {
                let state = &this.preferences.as_ref().unwrap().skill;
                assert!(!state.busy);
                assert!(state.error.is_some());
                assert!(state.status.is_none());
            })
            .unwrap();
    }
}
