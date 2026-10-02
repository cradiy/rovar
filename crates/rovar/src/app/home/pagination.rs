use super::*;

impl Studio {
    pub(in crate::app) fn set_home_page(&mut self, page: usize, cx: &mut Context<Self>) {
        self.home_page = page;
        self.home_scroll.set_offset(gpui::point(px(0.), px(0.)));
        cx.notify();
    }

    pub(super) fn home_pagination(
        &self,
        total: usize,
        width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let pages = total.div_ceil(PAGE_SIZE);
        let current = self.home_page;
        let visible: Vec<_> = (0..pages)
            .filter(|&page| page == 0 || page + 1 == pages || page.abs_diff(current) <= 1)
            .collect();
        let mut buttons = Vec::new();
        for (index, &page) in visible.iter().enumerate() {
            if index > 0 && page > visible[index - 1] + 1 {
                buttons.push(
                    div()
                        .w(px(20.))
                        .text_center()
                        .text_color(rgb(MUTED))
                        .child("…")
                        .into_any_element(),
                );
            }
            buttons.push(
                div()
                    .id(("home-page", page))
                    .debug_selector(move || format!("home-page-{}", page + 1))
                    .min_w(px(28.))
                    .h(px(28.))
                    .px(px(6.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(if current == page { ACCENT } else { MUTED }))
                    .when(current == page, |el| el.bg(rgba(0xb4a2ee22)))
                    .when(current != page, |el| {
                        el.cursor_pointer()
                            .hover(|style| style.bg(rgba(0xffffff08)).text_color(rgb(TEXT)))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.set_home_page(page, cx)),
                            )
                    })
                    .child((page + 1).to_string())
                    .into_any_element(),
            );
        }
        div()
            .id("home-pagination")
            .debug_selector(|| "home-pagination".into())
            .w_full()
            .flex_shrink_0()
            .border_t_1()
            .border_color(rgba(0xffffff0c))
            .flex()
            .justify_center()
            .child(
                div()
                    .w(px(width))
                    .py(px(12.))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .child(div().text_color(rgb(MUTED)).child(crate::i18n::message(
                        "home-page-range",
                        &[
                            ("start", (current * PAGE_SIZE + 1).to_string()),
                            ("end", ((current + 1) * PAGE_SIZE).min(total).to_string()),
                            ("total", total.to_string()),
                        ],
                    )))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .child(self.home_page_arrow(false, pages, cx))
                            .children(buttons)
                            .child(self.home_page_arrow(true, pages, cx)),
                    ),
            )
    }

    fn home_page_arrow(
        &self,
        next: bool,
        pages: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let enabled = if next {
            self.home_page + 1 < pages
        } else {
            self.home_page > 0
        };
        let page = if next {
            self.home_page + 1
        } else {
            self.home_page.saturating_sub(1)
        };
        let id = if next {
            "home-page-next"
        } else {
            "home-page-prev"
        };
        div()
            .id(id)
            .debug_selector(move || id.into())
            .size(px(28.))
            .rounded(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(if enabled { MUTED } else { 0x45434e }))
            .cursor(gpui::CursorStyle::Arrow)
            .child(icon(
                if next {
                    LucideIcons::ChevronRight
                } else {
                    LucideIcons::ChevronLeft
                },
                15.,
            ))
            .when(enabled, |el| {
                el.cursor_pointer()
                    .hover(|style| style.bg(rgba(0xffffff08)).text_color(rgb(TEXT)))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_home_page(page, cx)))
            })
    }
}
