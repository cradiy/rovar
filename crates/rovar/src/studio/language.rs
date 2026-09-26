use super::*;
use crate::i18n::{self, Language, t};
use uic::components::dropdown::{DropdownPlacement, dropdown};

impl Studio {
    pub(super) fn change_language(
        &mut self,
        language: Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_mixed = t("font-mixed");
        match i18n::set_language(language) {
            Ok(()) => {
                if let Some(panel) = &mut self.preferences {
                    panel.error = None;
                    panel.refresh_language(old_mixed, cx);
                }
                for editor in self.tabs.iter().filter_map(|tab| tab.editor.clone()) {
                    editor.update(cx, |editor, cx| editor.refresh_language(old_mixed, cx));
                }
                self.search.update(cx, |input, cx| {
                    input.set_placeholder(t("home-search"));
                    cx.notify();
                });
                for other in cx
                    .windows()
                    .into_iter()
                    .filter(|w| w.window_id().as_u64() != self.window_id)
                    .filter_map(|w| w.downcast::<Studio>())
                {
                    let _ = other.update(cx, |studio, _, cx| {
                        if let Some(panel) = &studio.preferences {
                            panel.refresh_language(old_mixed, cx);
                        }
                        for editor in studio.tabs.iter().filter_map(|tab| tab.editor.clone()) {
                            editor.update(cx, |editor, cx| editor.refresh_language(old_mixed, cx));
                        }
                        studio.search.update(cx, |input, cx| {
                            input.set_placeholder(t("home-search"));
                            cx.notify();
                        });
                        cx.notify();
                    });
                }
            }
            Err(error) => {
                if let Some(panel) = &mut self.preferences {
                    panel.error = Some(error.to_string());
                } else {
                    self.error = Some(i18n::message(
                        "language-save-error",
                        &[("error", error.to_string())],
                    ));
                }
            }
        }
        self.language_menu
            .update(cx, |menu, cx| menu.close(window, cx));
        cx.notify();
    }

    pub(super) fn language_control(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        dropdown(&self.language_menu)
            .placement(DropdownPlacement::BottomEnd)
            .w(px(192.))
            .min_w(px(192.))
            .p(px(4.))
            .rounded(px(8.))
            .bg(rgb(PANEL))
            .border_color(rgb(BORDER))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .trigger(
                div()
                    .id("language-menu")
                    .debug_selector(|| "language-menu".into())
                    .h(px(30.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0x282b33)))
                    .child(icon(LucideIcons::Languages, 16.))
                    .child(icon(LucideIcons::ChevronDown, 12.)),
            )
            .menu(
                div()
                    .debug_selector(|| "language-options".into())
                    .flex()
                    .flex_col()
                    .children(Language::available().into_iter().map(|language| {
                        div()
                            .id(language.id())
                            .debug_selector(move || format!("language-{}", language.id()))
                            .h(px(32.))
                            .flex_shrink_0()
                            .px(px(8.))
                            .rounded(px(4.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x353044)))
                            .child(div().w(px(16.)).when(i18n::preference() == language, |el| {
                                el.child(icon(LucideIcons::Check, 14.))
                            }))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .child(language.label()),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.change_language(language, window, cx)
                            }))
                    })),
            )
    }
}
