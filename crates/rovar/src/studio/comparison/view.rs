use super::*;

impl Studio {
    fn navigate_comparison(
        &mut self,
        server: bool,
        page: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = &mut self.comparison {
            if panel.resolving {
                return;
            }
            panel.navigate(server, page, window, cx);
            if panel.editor().is_none() {
                self.focus.focus(window, cx);
            }
            cx.notify();
        }
    }

    pub(in crate::studio) fn comparison_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let panel = self.comparison.as_ref().unwrap();
        let narrow = f32::from(window.viewport_size().width) < 720.;
        let metadata = if panel.server_visible {
            panel
                .object
                .as_ref()
                .map(|o| format!("v{} · {}", o.revision, home::edited_time(o.modified)))
                .unwrap_or_default()
        } else {
            crate::i18n::message(
                "compare-local-base",
                &[("revision", panel.base_revision.to_string())],
            )
        };
        let busy = self.remote.read(cx).busy || panel.resolving;
        let can_resolve = !busy && !panel.loading && panel.object.is_some();
        let page = panel.page;
        let server = panel.server_visible;
        let path = panel.path.clone();
        let retry_path = path.clone();
        let tabs = || {
            div()
                .flex()
                .p(px(3.))
                .gap(px(2.))
                .rounded(px(9.))
                .bg(rgb(0x22212b))
        };
        let switch = |cx: &mut Context<Self>| {
            tabs().children([(false, "compare-local"), (true, "compare-server")].map(
                |(server, key)| {
                    action(key, t(key))
                        .px(px(16.))
                        .when(panel.server_visible == server, |el| {
                            el.bg(rgb(0x393047)).text_color(rgb(ACCENT))
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate_comparison(server, page, window, cx)
                        }))
                },
            ))
        };
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgb(0x15151c))
            .flex()
            .flex_col()
            .text_size(px(12.))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(16.))
                    .py(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .id("compare-close")
                                    .debug_selector(|| "compare-close".into())
                                    .size(px(32.))
                                    .rounded(px(7.))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x292631)))
                                    .child(icon(LucideIcons::ArrowLeft, 16.))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_comparison(window, cx)
                                    })),
                            )
                            .child(div().min_w_0().truncate().child(panel.title.clone())),
                    )
                    .when(!narrow, |el| el.child(switch(cx)))
                    .child(
                        div()
                            .when(!narrow, |el| el.flex_1())
                            .flex()
                            .justify_end()
                            .gap(px(10.))
                            .child(
                                button("compare-keep-both", t("server-save-copy"))
                                    .h(px(34.))
                                    .px(px(14.))
                                    .rounded(px(8.))
                                    .border_1()
                                    .border_color(rgb(0x302d3b))
                                    .bg(rgb(0x201e29))
                                    .text_color(rgb(0xc5bfd3))
                                    .when(!busy, |el| {
                                        el.hover(|s| {
                                            s.bg(rgb(0x2b2737)).border_color(rgb(0x494158))
                                        })
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.keep_conflict_versions(&path, cx);
                                                this.close_comparison(window, cx);
                                            }),
                                        )
                                    })
                                    .opacity(if busy { 0.5 } else { 1. }),
                            )
                            .child(
                                button(
                                    "compare-resolve",
                                    t(if panel.resolving {
                                        "compare-resolving"
                                    } else if server {
                                        "compare-use-server"
                                    } else {
                                        "compare-submit-local"
                                    }),
                                )
                                .h(px(34.))
                                .px(px(16.))
                                .rounded(px(8.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .bg(rgb(if can_resolve { ACCENT } else { 0x2b2638 }))
                                .text_color(rgb(if can_resolve { 0x21182f } else { 0xaaa0bb }))
                                .when(can_resolve, |el| {
                                    el.hover(|s| s.bg(rgb(0xc9b6f2))).on_click(cx.listener(
                                        |this, _, window, cx| {
                                            this.resolve_comparison(window, cx);
                                        },
                                    ))
                                }),
                            ),
                    ),
            )
            .when(narrow, |el| {
                el.child(div().flex().justify_center().pb(px(8.)).child(switch(cx)))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(20.))
                    .pb(px(9.))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .flex_shrink_0()
                            .text_color(rgb(MUTED))
                            .text_size(px(11.))
                            .child(icon(
                                if server {
                                    LucideIcons::Lock
                                } else {
                                    LucideIcons::Pencil
                                },
                                12.,
                            ))
                            .child(t(if server {
                                "compare-copy-only"
                            } else {
                                "compare-merge-local"
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(metadata),
                    )
                    .when(panel.pages.len() > 1, |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .max_w(px(if narrow { 90. } else { 240. }))
                                .child(action("compare-prev-page", "‹").on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.navigate_comparison(
                                            server,
                                            page.saturating_sub(1),
                                            window,
                                            cx,
                                        )
                                    },
                                )))
                                .child(div().min_w_0().truncate().child(if narrow {
                                    format!("{}/{}", page + 1, panel.pages.len())
                                } else {
                                    panel.pages[page].1.clone()
                                }))
                                .child(action("compare-next-page", "›").on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        let last = this
                                            .comparison
                                            .as_ref()
                                            .map_or(0, |p| p.pages.len() - 1);
                                        this.navigate_comparison(
                                            server,
                                            (page + 1).min(last),
                                            window,
                                            cx,
                                        );
                                    },
                                ))),
                        )
                    }),
            )
            .when_some(panel.error.clone(), |el, error| {
                el.child(
                    div()
                        .px(px(20.))
                        .py(px(8.))
                        .text_color(rgb(0xee998f))
                        .child(error),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .flex()
                    .when_some(panel.editor(), |el, editor| el.child(editor))
                    .when(panel.editor().is_none(), |el| {
                        el.child(
                            div()
                                .size_full()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap(px(16.))
                                .text_color(rgb(MUTED))
                                .child(t(if panel.loading {
                                    "compare-loading"
                                } else if panel.failed {
                                    "compare-failed"
                                } else {
                                    "compare-page-missing"
                                }))
                                .when(panel.failed, |el| {
                                    el.child(
                                        action("compare-retry", t("server-retry-now")).on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.open_comparison(&retry_path, window, cx)
                                            }),
                                        ),
                                    )
                                }),
                        )
                    }),
            )
    }
}

fn action(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    button(id, label).hover(|style| style.bg(rgb(0x34313f)))
}

fn button(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(30.))
        .px(px(10.))
        .rounded(px(6.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .child(label)
}
