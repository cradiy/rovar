use super::*;
use crate::ui::theme::Color;
use gpui::{ObjectFit, img};
use uic::components::{
    context_menu::{self, ContextMenuItem},
    input::Input,
};

pub(super) fn thumbnail(entry: &Entry, height: f32) -> impl IntoElement {
    div()
        .h(px(height))
        .w_full()
        .overflow_hidden()
        .rounded(px(6.))
        .bg(Color::Workspace.color())
        .flex()
        .items_center()
        .justify_center()
        .map(|el| {
            if entry.error.is_some() {
                el.flex_col()
                    .gap(px(8.))
                    .child(icon(LucideIcons::CircleAlert, 22.).text_color(Color::Danger.color()))
                    .child(
                        div()
                            .px(px(4.))
                            .text_center()
                            .text_size(px(11.))
                            .text_color(MUTED.color())
                            .child(t(if entry.name.is_empty() {
                                "assets-unreadable-component"
                            } else {
                                "assets-load-failed"
                            })),
                    )
            } else {
                match &entry.preview {
                    Some(path) => el.child(
                        img(crate::platform::preview_image(path.clone()))
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    ),
                    None => el.child(icon(LucideIcons::Component, 25.).text_color(ACCENT.color())),
                }
            }
        })
}

impl Workspace {
    pub(super) fn local_assets_panel(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let library = self.assets.library.as_ref().map(|lib| lib.read(cx));
        let busy = library.is_some_and(|lib| lib.busy) || self.assets.inserting;
        let error = self
            .assets
            .error
            .clone()
            .or_else(|| library.and_then(|lib| lib.error.clone()));
        let query = self.assets.search.read(cx).value().trim().to_lowercase();
        let entries: Vec<_> = library
            .into_iter()
            .flat_map(|lib| lib.entries.iter())
            .filter(|entry| {
                if entry.name.is_empty() {
                    t("assets-unreadable-component")
                        .to_lowercase()
                        .contains(&query)
                } else {
                    entry.name.to_lowercase().contains(&query)
                }
            })
            .cloned()
            .collect();
        let enabled = self.can_save_asset(cx);
        let mut panel = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(12.))
            .child(
                Input::new(&self.assets.search)
                    .w_full()
                    .h(px(32.))
                    .text_size(px(12.))
                    .bg(Color::Input.color())
                    .border_color(Color::Text.color().opacity(0.0627)),
            )
            .child(self.color_assets(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(MUTED.color())
                            .child(t("assets-components")),
                    )
                    .child(
                        div()
                            .id("save-selection-to-assets")
                            .debug_selector(|| "save-selection-to-assets".into())
                            .h(px(28.))
                            .px(px(8.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .text_size(px(11.))
                            .text_color(ACCENT.color())
                            .opacity(if enabled { 1. } else { 0.35 })
                            .child(icon(LucideIcons::Plus, 13.).text_color(ACCENT.color()))
                            .child(t("assets-save-selection"))
                            .when(enabled, |el| {
                                el.cursor_pointer()
                                    .hover(|s| s.bg(Color::Accent.color().opacity(0.0941)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.begin_save_asset(window, cx)
                                    }))
                            }),
                    ),
            )
            .when(busy, |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(MUTED.color())
                        .child(t("assets-working")),
                )
            })
            .when_some(error, |el, error| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(Color::Danger.color())
                        .child(error)
                        .child(
                            div()
                                .id("retry-assets")
                                .mt(px(6.))
                                .cursor_pointer()
                                .text_color(ACCENT.color())
                                .child(t("assets-retry"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.assets.error = None;
                                    if let Some(library) = this.assets.library.clone() {
                                        library.update(cx, |lib, cx| lib.refresh(cx));
                                    }
                                    cx.notify();
                                })),
                        ),
                )
            });
        if entries.is_empty() && !busy {
            panel = panel.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(10.))
                    .px(px(8.))
                    .pb(px(48.))
                    .child(icon(LucideIcons::Component, 30.).text_color(Color::Muted.color()))
                    .child(div().text_size(px(13.)).child(t(if query.is_empty() {
                        "assets-empty"
                    } else {
                        "assets-no-results"
                    })))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(MUTED.color())
                            .text_center()
                            .child(t(if query.is_empty() {
                                "assets-empty-hint"
                            } else {
                                "assets-search-hint"
                            })),
                    ),
            );
        } else {
            let columns = if self.panels.width(panels::Side::Left) >= 220. {
                2
            } else {
                1
            };
            panel = panel.child(
                gpui::uniform_list(
                    "component-library-list",
                    entries.len().div_ceil(columns),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|row| {
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .flex()
                                    .gap(px(8.))
                                    .pb(px(10.))
                                    .children((0..columns).map(|col| {
                                        match entries.get(row * columns + col) {
                                            Some(entry) => this
                                                .asset_card(entry.clone(), cx)
                                                .into_any_element(),
                                            None => div().flex_1().min_w_0().into_any_element(),
                                        }
                                    }))
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .min_h_0(),
            );
        }
        panel.on_any_mouse_down(|_, _, cx| cx.stop_propagation())
    }

    fn asset_card(&self, entry: Entry, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let available = self.library_available(cx) && !self.assets.inserting;
        let failed = entry.error.is_some();
        let enabled = available && !failed;
        let remove = entry.clone();
        let insert = entry.clone();
        let context = entry.clone();
        let drag = AssetDrag {
            entry: entry.clone(),
            library: self.assets.library.clone().unwrap(),
        };
        div()
            .id(gpui::SharedString::from(format!("asset-card-{}", entry.id)))
            .debug_selector({
                let id = entry.id.clone();
                move || format!("asset-card-{id}")
            })
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .p(px(5.))
            .rounded(px(9.))
            .border_1()
            .border_color(Color::Text.color().opacity(0.0431))
            .bg(Color::Surface.color())
            .hover(|s| {
                s.border_color(Color::Accent.color().opacity(0.4667))
                    .bg(Color::Hover.color())
            })
            .child(thumbnail(&entry, 84.))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(28.))
                    .px(px(3.))
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(entry.name.clone()),
                    )
                    .when(failed, |el| {
                        el.child(
                            div()
                                .id(gpui::SharedString::from(format!(
                                    "remove-failed-asset-{}",
                                    entry.id
                                )))
                                .debug_selector({
                                    let id = entry.id.clone();
                                    move || format!("remove-failed-asset-{id}")
                                })
                                .flex_shrink_0()
                                .ml(px(4.))
                                .px(px(4.))
                                .text_color(ACCENT.color())
                                .opacity(if available { 1. } else { 0.35 })
                                .child(t("assets-remove"))
                                .when(available, |el| {
                                    el.cursor_pointer()
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation()
                                        })
                                        .hover(|s| s.bg(Color::Accent.color().opacity(0.0941)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(library) = this.assets.library.clone() {
                                                library.update(cx, |library, cx| {
                                                    library.remove_failed(remove.clone(), cx)
                                                });
                                            }
                                            cx.stop_propagation();
                                        }))
                                }),
                        )
                    }),
            )
            .when(enabled, |el| {
                el.cursor_pointer()
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            if event.click_count() == 2 {
                                this.insert_asset(insert.clone(), None, window, cx);
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            this.asset_menu(context.clone(), event.position, window, cx);
                            cx.stop_propagation();
                        }),
                    )
            })
    }

    fn asset_menu(
        &self,
        entry: Entry,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut menu = crate::editor::context_menu::menu(194., "asset-context-glass");
        for (action, label, glyph) in [
            (0, t("assets-insert"), LucideIcons::Plus),
            (1, t("rename"), LucideIcons::Pencil),
            (2, t("delete"), LucideIcons::Trash2),
        ] {
            let weak = cx.entity().downgrade();
            let entry = entry.clone();
            menu = menu.item(ContextMenuItem::action_with(
                move |_, _| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(9.))
                        .child(icon(glyph, 14.))
                        .child(label)
                },
                move |window, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if action == 0 {
                            this.insert_asset(entry.clone(), None, window, cx)
                        } else {
                            this.edit_asset(entry.clone(), action == 2, window, cx)
                        }
                    });
                },
            ));
        }
        let _ = context_menu::show(menu, position, window, cx);
    }

    pub(in crate::editor) fn asset_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let delete = matches!(
            self.assets.dialog,
            Some(Dialog::Delete(_) | Dialog::DocumentDelete(_))
        );
        let title = match self.assets.dialog {
            Some(Dialog::Save(_)) => t("assets-save-title"),
            Some(Dialog::Delete(_) | Dialog::DocumentDelete(_)) => t("assets-delete-title"),
            _ => t("rename"),
        };
        let value = self.assets.name.read(cx).value();
        let valid = (self.library_available(cx)
            || matches!(
                self.assets.dialog,
                Some(Dialog::DocumentRename(_) | Dialog::DocumentDelete(_))
            ))
            && (delete || (!value.trim().is_empty() && value.trim().chars().count() <= 200));
        let mut body = div()
            .id("asset-dialog")
            .debug_selector(|| "asset-dialog".into())
            .w(px(340.))
            .p(px(20.))
            .rounded(px(14.))
            .bg(Color::Panel.color())
            .border_1()
            .border_color(Color::Text.color().opacity(0.1255))
            .shadow_xl()
            .flex()
            .flex_col()
            .gap(px(16.))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(div().text_size(px(15.)).child(title));
        if delete {
            let name = match &self.assets.dialog {
                Some(Dialog::Delete(entry)) => entry.name.clone(),
                Some(Dialog::DocumentDelete(id)) => self
                    .components
                    .definitions
                    .get(id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default(),
                _ => String::new(),
            };
            body = body.child(div().text_size(px(13.)).child(name)).child(
                div().text_size(px(12.)).text_color(MUTED.color()).child(t(
                    if matches!(self.assets.dialog, Some(Dialog::DocumentDelete(_))) {
                        "component-delete-hint"
                    } else {
                        "assets-delete-hint"
                    },
                )),
            );
        } else {
            body = body.child(
                Input::new(&self.assets.name)
                    .w_full()
                    .h(px(36.))
                    .text_size(px(13.))
                    .bg(Color::Workspace.color())
                    .text_color(TEXT.color())
                    .border_color(BORDER.color()),
            );
        }
        body = body.child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .children([false, true].map(|commit| {
                    div()
                        .id(if commit {
                            "confirm-asset-dialog"
                        } else {
                            "cancel-asset-dialog"
                        })
                        .debug_selector(move || {
                            if commit {
                                "confirm-asset-dialog".into()
                            } else {
                                "cancel-asset-dialog".into()
                            }
                        })
                        .px(px(14.))
                        .h(px(32.))
                        .rounded(px(7.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.))
                        .bg(if commit {
                            if delete {
                                Color::DangerSurface.color()
                            } else {
                                Color::Accent.color()
                            }
                        } else {
                            Color::Input.color()
                        })
                        .opacity(if commit && !valid { 0.4 } else { 1. })
                        .child(t(if commit {
                            if delete { "delete" } else { "save-document" }
                        } else {
                            "cancel"
                        }))
                        .when(!commit || valid, |el| {
                            el.cursor_pointer()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if commit {
                                        this.confirm_asset_dialog(window, cx)
                                    } else {
                                        this.cancel_asset_dialog(window, cx)
                                    }
                                }))
                        })
                })),
        );
        gpui::deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(Color::Overlay.color())
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.cancel_asset_dialog(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(body),
        )
        .with_priority(30)
    }
}
