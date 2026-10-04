use super::*;
use crate::app::servers::button;
use crate::ui::theme::Color;
use gpui::{ClipboardItem, Focusable, FontWeight, SharedString};
use uic::components::input::Input;

const CARD: Color = Color::Surface;
const LINE: Color = Color::Border;

fn primary(id: &'static str, label: &'static str, busy: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .rounded(px(7.))
        .cursor_pointer()
        .child(label)
        .h(px(36.))
        .px(px(16.))
        .justify_center()
        .font_weight(FontWeight::SEMIBOLD)
        .bg(ACCENT.color())
        .text_color(Color::OnAccent.color())
        .hover(|s| s.bg(Color::Accent.color()))
        .opacity(if busy { 0.5 } else { 1. })
}
fn avatar(label: &str, size: f32) -> gpui::Div {
    div()
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(10.))
        .bg(Color::Selected.color())
        .flex()
        .items_center()
        .justify_center()
        .text_color(Color::Accent.color())
        .font_weight(FontWeight::SEMIBOLD)
        .child(
            label
                .chars()
                .next()
                .unwrap_or('•')
                .to_uppercase()
                .to_string(),
        )
}

impl Studio {
    fn space_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = &mut self.spaces {
            if panel.busy {
                return;
            }
            panel.view = view;
            panel.error = None;
            if view == View::Create {
                panel.name.focus_handle(cx).focus(window, cx);
            }
            if view == View::Join {
                panel.code.focus_handle(cx).focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(in crate::app) fn spaces_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let panel = self.spaces.as_ref().unwrap();
        let team = panel.connection.space.kind == "team";
        let field = |input: &Entity<TextInput>| {
            Input::new(input)
                .w_full()
                .h(px(44.))
                .font_family(crate::ui::font::family(cx))
                .text_size(px(14.))
                .line_height(px(20.))
                .bg(Color::Workspace.color())
                .text_color(TEXT.color())
                .border_color(LINE.color())
                .rounded(px(9.))
                .appearance(crate::ui::theme::input_appearance())
        };
        let navigation =
            div()
                .flex()
                .gap(px(4.))
                .p(px(4.))
                .bg(Color::Workspace.color())
                .rounded(px(10.))
                .children(
                    [
                        (
                            View::Overview,
                            if team {
                                "space-members"
                            } else {
                                "space-your-teams"
                            },
                        ),
                        (View::Create, "space-create-team"),
                        (View::Join, "space-join-team"),
                    ]
                    .into_iter()
                    .filter(|(view, _)| {
                        *view != View::Create || panel.connection.identity.registration.teams
                    })
                    .enumerate()
                    .map(|(index, (view, label))| {
                        button(("space-view", index), t(label))
                            .flex_1()
                            .justify_center()
                            .h(px(34.))
                            .text_color(if panel.view == view {
                                TEXT.color()
                            } else {
                                MUTED.color()
                            })
                            .when(panel.view == view, |el| el.bg(Color::Selected.color()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.space_view(view, window, cx)
                            }))
                    }),
                );
        let mut content = div().flex().flex_col().gap(px(20.)).min_h(px(240.));
        match panel.view {
            View::Overview if team => {
                content = content
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(avatar(&panel.connection.space.name, 44.))
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
                                            .text_size(px(17.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(panel.connection.space.name.clone()),
                                    )
                                    .child(
                                        div().text_size(px(12.)).text_color(MUTED.color()).child(
                                            crate::i18n::count(
                                                "space-member-count",
                                                panel.members.len(),
                                            ),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("space-members")
                            .max_h(px(200.))
                            .overflow_y_scroll()
                            .border_1()
                            .border_color(LINE.color())
                            .rounded(px(12.))
                            .bg(CARD.color())
                            .children(panel.members.iter().enumerate().map(|(index, member)| {
                                let user = member.user_id.clone();
                                let is_self = user == panel.connection.identity.user_id;
                                let remove = member.role != "owner"
                                    && (panel.connection.space.role == "owner" || is_self);
                                div()
                                    .h(px(60.))
                                    .px(px(14.))
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .when(index > 0, |el| {
                                        el.border_t_1().border_color(LINE.color())
                                    })
                                    .child(avatar(&member.username, 32.))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .truncate()
                                            .child(member.username.clone()),
                                    )
                                    .child(
                                        div()
                                            .px(px(8.))
                                            .py(px(3.))
                                            .rounded(px(5.))
                                            .text_size(px(11.))
                                            .bg(Color::Selected.color())
                                            .text_color(Color::Muted.color())
                                            .child(t(if member.role == "owner" {
                                                "space-owner"
                                            } else {
                                                "space-member"
                                            })),
                                    )
                                    .when(remove, |el| {
                                        el.child(
                                            button(
                                                SharedString::from(format!("remove-{user}")),
                                                t(if is_self {
                                                    "space-leave"
                                                } else {
                                                    "space-remove"
                                                }),
                                            )
                                            .text_size(px(12.))
                                            .text_color(Color::Danger.color())
                                            .on_click(
                                                cx.listener(move |this, _, window, cx| {
                                                    this.space_action(
                                                        Action::Remove(user.clone()),
                                                        window,
                                                        cx,
                                                    )
                                                }),
                                            ),
                                        )
                                    })
                            })),
                    );
                if panel.connection.space.role == "owner" {
                    let generated = panel.invitation.is_some();
                    content = content.child(
                        div()
                            .p(px(16.))
                            .rounded(px(12.))
                            .border_1()
                            .border_color(Color::Border.color())
                            .bg(Color::Surface.color())
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .child(icon(LucideIcons::Link, 20.).text_color(ACCENT.color()))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .gap(px(5.))
                                            .child(
                                                div()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(t("space-invite-members")),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .text_color(MUTED.color())
                                                    .child(t("space-invite-description")),
                                            ),
                                    )
                                    .when(!generated, |el| {
                                        el.child(
                                            primary(
                                                "space-invite",
                                                t("space-invite-action"),
                                                panel.busy,
                                            )
                                            .on_click(
                                                cx.listener(|this, _, window, cx| {
                                                    this.space_action(Action::Invite, window, cx)
                                                }),
                                            ),
                                        )
                                    }),
                            )
                            .when_some(panel.invitation.clone(), |el, code| {
                                let copy = code.clone();
                                el.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .p(px(8.))
                                        .rounded(px(8.))
                                        .bg(Color::Workspace.color())
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .text_size(px(12.))
                                                .text_color(Color::Text.color())
                                                .child(code),
                                        )
                                        .child(
                                            button("copy-invite", t("space-copy"))
                                                .bg(Color::Selected.color())
                                                .text_color(ACCENT.color())
                                                .child(icon(LucideIcons::Copy, 14.))
                                                .on_click(move |_, _, cx| {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(copy.clone()),
                                                    )
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(MUTED.color())
                                                .child(t("space-invite-expiry")),
                                        )
                                        .child(
                                            button("replace-invite", t("space-invite-replace"))
                                                .text_size(px(11.))
                                                .text_color(ACCENT.color())
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.space_action(Action::Invite, window, cx)
                                                })),
                                        ),
                                )
                            }),
                    );
                }
            }
            View::Overview => {
                let teams = self
                    .remote
                    .read(cx)
                    .connections()
                    .iter()
                    .filter(|c| {
                        c.authenticated
                            && c.url == panel.connection.url
                            && c.identity.user_id == panel.connection.identity.user_id
                            && c.space.kind == "team"
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                content = content.child(
                    div()
                        .text_size(px(13.))
                        .text_color(MUTED.color())
                        .child(t("space-personal-note")),
                );
                if teams.is_empty() {
                    content = content.child(
                        div()
                            .py(px(32.))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.))
                            .child(avatar("+", 44.))
                            .child(div().text_color(MUTED.color()).child(t("space-no-teams"))),
                    );
                } else {
                    content = content.child(
                        div()
                            .id("space-team-list")
                            .max_h(px(240.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .children(teams.into_iter().map(|connection| {
                                let id = connection.id.clone();
                                div()
                                    .id(SharedString::from(format!("team-{id}")))
                                    .p(px(12.))
                                    .rounded(px(10.))
                                    .bg(CARD.color())
                                    .border_1()
                                    .border_color(LINE.color())
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(Color::Hover.color()))
                                    .child(avatar(&connection.space.name, 36.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .child(connection.space.name),
                                    )
                                    .child(
                                        icon(LucideIcons::ArrowRight, 16.)
                                            .text_color(MUTED.color()),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_source(Some(id.clone()), window, cx);
                                        this.open_spaces(window, cx);
                                    }))
                            })),
                    );
                }
            }
            View::Create | View::Join => {
                let create = panel.view == View::Create;
                content = content
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(19.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(t(if create {
                                        "space-create-title"
                                    } else {
                                        "space-join-title"
                                    })),
                            )
                            .child(
                                div()
                                    .text_color(MUTED.color())
                                    .line_height(px(20.))
                                    .child(t(if create {
                                        "space-create-description"
                                    } else {
                                        "space-join-description"
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(div().text_size(px(12.)).text_color(MUTED.color()).child(t(
                                if create {
                                    "space-team-name"
                                } else {
                                    "space-invitation"
                                },
                            )))
                            .child(field(if create { &panel.name } else { &panel.code })),
                    )
                    .child(
                        div().flex().justify_end().child(
                            primary(
                                "space-submit",
                                t(if create {
                                    "space-create-team"
                                } else {
                                    "space-join-team"
                                }),
                                panel.busy,
                            )
                            .child(icon(LucideIcons::ArrowRight, 15.))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    if let Some(panel) = &this.spaces {
                                        let action = if create {
                                            Action::Create(panel.name.read(cx).value().to_string())
                                        } else {
                                            Action::Join(panel.code.read(cx).value().to_string())
                                        };
                                        this.space_action(action, window, cx);
                                    }
                                },
                            )),
                        ),
                    );
            }
        }
        let body = div()
            .id("spaces-dialog")
            .w(px(560.))
            .max_w_full()
            .max_h_full()
            .overflow_y_scroll()
            .rounded(px(18.))
            .border_1()
            .border_color(Color::Border.color())
            .bg(Color::Panel.color())
            .shadow_xl()
            .font_family(crate::ui::font::family(cx))
            .text_color(TEXT.color())
            .text_size(px(13.))
            .flex()
            .flex_col()
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .p(px(24.))
                    .flex()
                    .flex_col()
                    .gap(px(22.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(5.))
                                    .child(
                                        div()
                                            .text_size(px(22.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(t("space-manage")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(MUTED.color())
                                            .child(panel.connection.identity.username.clone()),
                                    ),
                            )
                            .child(
                                button("space-close-icon", "")
                                    .child(icon(LucideIcons::X, 18.))
                                    .text_color(MUTED.color())
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.spaces = None;
                                        this.focus.focus(window, cx);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(navigation)
                    .child(content)
                    .when_some(panel.error.clone(), |el, error| {
                        el.child(
                            div()
                                .p(px(12.))
                                .rounded(px(8.))
                                .bg(Color::DangerSurface.color())
                                .text_color(Color::Danger.color())
                                .child(error),
                        )
                    }),
            )
            .child(
                div()
                    .px(px(24.))
                    .py(px(14.))
                    .border_t_1()
                    .border_color(LINE.color())
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_size(px(12.)).text_color(MUTED.color()).child(t(
                        if panel.busy {
                            "server-connecting"
                        } else {
                            "space-private-footer"
                        },
                    )))
                    .child(
                        button("spaces-close", t("close"))
                            .bg(Color::Surface.color())
                            .px(px(16.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.spaces = None;
                                this.focus.focus(window, cx);
                                cx.notify();
                            })),
                    ),
            );
        gpui::deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(Color::Overlay.color())
                .p(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .child(body),
        )
        .with_priority(30)
    }
}
