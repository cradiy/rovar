use super::*;

fn button(
    id: &'static str,
    label: &'static str,
    glyph: LucideIcons,
    enabled: bool,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .size(px(30.))
        .flex_shrink_0()
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .when(enabled, |el| {
            el.cursor_pointer().hover(|s| s.bg(Color::Hover.color()))
        })
        .opacity(if enabled { 1. } else { 0.35 })
        .tooltip(move |_, cx| cx.new(|_| toolbar::ToolTip(label.into())).into())
        .child(icon(glyph, 16.).text_color(TEXT.color()))
}

impl Workspace {
    pub(super) fn playback_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let playback = self.presentation.playback.as_ref().unwrap();
        let name = self
            .boards
            .iter()
            .find(|b| b.id == playback.current)
            .unwrap()
            .name
            .clone();
        let can_back = !playback.history.is_empty();
        let owner = playback.owner.clone();
        div()
            .debug_selector(|| "playback-toolbar".into())
            .h(px(52.))
            .flex_shrink_0()
            .w_full()
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(12.))
            .bg(PANEL.color())
            .border_b_1()
            .border_color(BORDER.color())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .p(px(3.))
                    .flex_shrink_0()
                    .rounded(px(9.))
                    .bg(Color::Input.color())
                    .child(
                        button(
                            "playback-back",
                            t("prototype-back"),
                            LucideIcons::ArrowLeft,
                            can_back,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if can_back {
                                this.commit_playback_hover();
                                this.playback_action(Action::Back, cx);
                            }
                        })),
                    )
                    .child(div().w(px(1.)).h(px(14.)).bg(BORDER.color()))
                    .child(
                        button(
                            "playback-restart",
                            t("prototype-restart"),
                            LucideIcons::RotateCcw,
                            true,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            let page = this
                                .presentation
                                .playback
                                .as_ref()
                                .unwrap()
                                .initial_page
                                .clone();
                            this.load_page(page, window, cx);
                            let p = this.presentation.playback.as_mut().unwrap();
                            p.current = p.start;
                            p.history.clear();
                            p.hotspot_hovered = false;
                            p.entered = None;
                            p.hover = None;
                            this.pause_videos(cx);
                            cx.notify();
                        })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .size(px(24.))
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .bg(Color::Selected.color())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon(LucideIcons::Play, 12.).text_color(ACCENT.color())),
                    )
                    .child(
                        div()
                            .debug_selector(|| "playback-frame-name".into())
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(TEXT.color())
                            .child(name),
                    ),
            )
            .child(
                button("presentation-close", t("close"), LucideIcons::X, true).on_click(
                    move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| this.stop_presentation(window, cx));
                    },
                ),
            )
    }
}
