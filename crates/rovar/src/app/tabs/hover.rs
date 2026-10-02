use super::*;

impl Studio {
    pub(in crate::app) fn dismiss_tab_preview(&mut self, cx: &mut Context<Self>) {
        self.strip.hovered = None;
        self.strip.hover_task = Task::ready(());
        if self.strip.hover_card.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn hover_tab(&mut self, token: usize, hovered: bool, cx: &mut Context<Self>) {
        if !hovered {
            if self.strip.hovered == Some(token) {
                self.dismiss_tab_preview(cx);
            }
            return;
        }
        if self.strip.drag.is_some() || self.dragging.is_some() {
            self.dismiss_tab_preview(cx);
            return;
        }
        let delay = Duration::from_millis(if self.strip.hover_card.is_some() {
            90
        } else {
            420
        });
        self.strip.hovered = Some(token);
        self.strip.hover_task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                if this.strip.hovered == Some(token)
                    && this.strip.drag.is_none()
                    && this.tabs.iter().any(|tab| tab.token == token)
                {
                    this.strip.hover_card = Some(token);
                    cx.notify();
                }
            });
        });
    }

    pub(in crate::app) fn tab_preview(
        &self,
        window: &Window,
        cx: &gpui::App,
    ) -> Option<gpui::AnyElement> {
        let token = self.strip.hover_card?;
        if self.strip.drag.is_some() || self.dragging.is_some() {
            return None;
        }
        let index = self.tabs.iter().position(|tab| tab.token == token)?;
        let tab = &self.tabs[index];
        let remote = self.remote.read(cx);
        let account = remote
            .link(&tab.file.path)
            .and_then(|link| remote.connection(&link.connection))
            .map(|connection| {
                (
                    connection.identity.username.clone(),
                    connection.space_label(),
                    connection.space.kind == "team",
                )
            });
        let viewport = f32::from(window.viewport_size().width);
        let width = self.tab_width(viewport, self.tabs.len());
        let tab_left = self.tab_left()
            + self
                .strip
                .slots
                .get(&token)
                .map_or(slot_left(index, width), |slot| slot.current)
            + f32::from(self.tab_scroll.offset().x);
        let preview = tab
            .file
            .preview
            .as_ref()
            .map(|name| self.directory.join("previews").join(name));
        Some(
            div()
                .id("tab-preview")
                .debug_selector(|| "tab-preview".into())
                .absolute()
                .left(px(tab_left.clamp(8., (viewport - 310.).max(8.))))
                .top(px(BAR_HEIGHT + 6.))
                .w(px(302.))
                .p(px(10.))
                .rounded(px(12.))
                .border_1()
                .border_color(rgb(0x393441))
                .bg(rgb(0x222029))
                .shadow_xl()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .px(px(2.))
                        .max_h(px(56.))
                        .overflow_hidden()
                        .text_size(px(13.))
                        .text_color(rgb(TEXT))
                        .child(tab.file.title.clone()),
                )
                .when_some(account, |el, (username, space, team)| {
                    el.child(
                        div()
                            .debug_selector(|| "tab-preview-account".into())
                            .px(px(2.))
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(icon(LucideIcons::User, 12.).flex_shrink_0())
                            .child(div().max_w(px(90.)).min_w_0().truncate().child(username))
                            .child(div().flex_shrink_0().text_color(rgb(0x625b70)).child("·"))
                            .child(
                                icon(
                                    if team {
                                        LucideIcons::Users
                                    } else {
                                        LucideIcons::House
                                    },
                                    12.,
                                )
                                .flex_shrink_0(),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(space)),
                    )
                })
                .child(
                    div()
                        .w_full()
                        .h(px(168.))
                        .rounded(px(7.))
                        .overflow_hidden()
                        .bg(rgb(0x121317))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(preview.is_none(), |el| {
                            el.child(icon(LucideIcons::Frame, 30.).text_color(rgb(0x797184)))
                        })
                        .when_some(preview, |el, path| {
                            el.child(
                                gpui::img(crate::platform::preview_image(path))
                                    .debug_selector(|| "tab-preview-image".into())
                                    .size_full()
                                    .object_fit(gpui::ObjectFit::Contain),
                            )
                        }),
                )
                .into_any_element(),
        )
    }
}
