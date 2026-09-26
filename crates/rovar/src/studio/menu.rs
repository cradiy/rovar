use super::*;
use gpui::{color_svg, rgba};
use uic::components::dropdown::dropdown;

impl Studio {
    pub(super) fn app_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let active = self.active_editor().is_some();
        let exporting = self
            .tabs
            .iter()
            .any(|tab| Some(tab.token) == self.active && tab.exporting);
        dropdown(&self.menu)
            .w(px(264.))
            .p(px(5.))
            .rounded(px(12.))
            .bg(rgb(0x202027))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .border_color(rgba(0xffffff16))
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(8.), rgba(0x00000050).into()).blur_radius(px(24.)),
            ])
            .trigger(
                div()
                    .id("app-menu")
                    .debug_selector(|| "app-menu".into())
                    .size(px(32.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba(0xffffff08)))
                    .child(
                        color_svg()
                            .path("rovar-mark.svg")
                            .w(px(26.))
                            .h(px(26. * 710. / 740.)),
                    ),
            )
            .menu(
                div().flex().flex_col().children(
                    [
                        ("new-document", LucideIcons::FilePlus, "Mod+N"),
                        ("open-document", LucideIcons::FolderOpen, "Mod+O"),
                        ("save-document", LucideIcons::Save, "Mod+S"),
                        ("export-document", LucideIcons::Download, "Mod+Shift+E"),
                        ("settings", LucideIcons::Settings, "Mod+,"),
                    ]
                    .into_iter()
                    .map(|(id, glyph, shortcut)| {
                        let enabled = match id {
                            "save-document" => active,
                            "export-document" => active && !exporting,
                            _ => true,
                        };
                        div()
                            .when(matches!(id, "save-document" | "settings"), |el| {
                                el.child(div().h(px(1.)).mx(px(8.)).my(px(5.)).bg(rgba(0xffffff12)))
                            })
                            .child(
                                div()
                                    .id(id)
                                    .debug_selector(move || id.into())
                                    .h(px(34.))
                                    .px(px(10.))
                                    .rounded(px(7.))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .text_color(rgb(if enabled { 0xe0dbe9 } else { 0x686371 }))
                                    .child(icon(glyph, 15.))
                                    .child(div().flex_1().child(t(id)))
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(rgb(if enabled {
                                                0x8d859e
                                            } else {
                                                0x514d58
                                            }))
                                            .child(crate::shortcuts::label(shortcut)),
                                    )
                                    .when(enabled, |el| {
                                        el.cursor_pointer().hover(|s| s.bg(rgb(0x353042))).on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.menu.update(cx, |state, cx| {
                                                    state.close(window, cx)
                                                });
                                                match id {
                                                    "new-document" => this.new_document(window, cx),
                                                    "open-document" => this.open_dialog(window, cx),
                                                    "export-document" => {
                                                        this.export_dialog(window, cx)
                                                    }
                                                    "settings" => this.open_settings(window, cx),
                                                    _ => this.save_command(window, cx),
                                                }
                                            }),
                                        )
                                    }),
                            )
                    }),
                ),
            )
    }
}
