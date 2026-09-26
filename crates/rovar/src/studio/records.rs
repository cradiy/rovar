use super::*;
mod delete;
use gpui::Focusable;
use std::{collections::BTreeSet, path::Path};
use uic::components::{
    context_menu::{self, ContextMenu, ContextMenuAppearance, ContextMenuItem},
    input::Input,
};

// These markers affect the recent list only; the catalog still owns all documents.
pub(super) fn removed_recent(directory: &Path) -> BTreeSet<PathBuf> {
    std::fs::read_dir(directory.join("removed-recents"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            uuid::Uuid::parse_str(name.to_str()?).ok()?;
            Some(
                directory
                    .join("documents")
                    .join(name)
                    .with_extension("rovar"),
            )
        })
        .collect()
}

impl Studio {
    fn recent_marker(&self, path: &Path) -> PathBuf {
        self.directory
            .join("removed-recents")
            .join(path.file_stem().unwrap_or_default())
    }

    fn notify_catalog(&self, cx: &mut Context<Self>) {
        for other in cx
            .windows()
            .into_iter()
            .filter(|handle| handle.window_id().as_u64() != self.window_id)
            .filter_map(|handle| handle.downcast::<Studio>())
        {
            let _ = other.update(cx, |studio, _, cx| {
                if Rc::ptr_eq(&studio.session, &self.session) {
                    cx.notify();
                }
            });
        }
        cx.notify();
    }

    pub(super) fn restore_recent(&mut self, path: &Path, cx: &mut Context<Self>) {
        if !self.session.borrow().removed_recent.contains(path) {
            return;
        }
        match std::fs::remove_file(self.recent_marker(path)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        }
        self.session.borrow_mut().removed_recent.remove(path);
        self.notify_catalog(cx);
    }

    pub(super) fn remove_recent(&mut self, path: &Path, cx: &mut Context<Self>) {
        if !files::is_internal(&self.directory, path) {
            return;
        }
        let marker = self.recent_marker(path);
        let result = std::fs::create_dir_all(marker.parent().unwrap())
            .and_then(|_| std::fs::write(marker, []));
        match result {
            Ok(()) => {
                self.session
                    .borrow_mut()
                    .removed_recent
                    .insert(path.to_owned());
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        self.notify_catalog(cx);
    }

    fn document_busy(&self, path: &Path, cx: &mut Context<Self>) -> bool {
        let busy = |studio: &Studio, cx: &gpui::App| {
            studio.strip.drag.is_some()
                || studio.tabs.iter().any(|tab| {
                    tab.file.path == path
                        && (tab.saving
                            || tab.exporting
                            || tab.loading
                            || tab
                                .editor
                                .as_ref()
                                .is_some_and(|editor| editor.read(cx).is_exporting()))
                })
        };
        if busy(self, cx) {
            return true;
        }
        cx.windows()
            .into_iter()
            .filter(|handle| handle.window_id().as_u64() != self.window_id)
            .filter_map(|handle| handle.downcast::<Studio>())
            .any(|handle| {
                handle
                    .update(cx, |studio, _, cx| {
                        Rc::ptr_eq(&studio.session, &self.session) && busy(studio, cx)
                    })
                    .unwrap_or(false)
            })
    }

    fn delete_document(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if !files::is_internal(&self.directory, path)
            || !self
                .session
                .borrow()
                .recent
                .iter()
                .any(|file| file.path == path)
        {
            return;
        }
        if self.document_busy(path, cx) {
            self.error = Some(t("document-busy").into());
            cx.notify();
            return;
        }
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        }
        // No writer can be in flight. Remove all open owners before another autosave.
        self.drop_deleted_document(path, window, cx);
        for handle in cx
            .windows()
            .into_iter()
            .filter(|handle| handle.window_id().as_u64() != self.window_id)
            .filter_map(|handle| handle.downcast::<Studio>())
        {
            let _ = handle.update(cx, |studio, window, cx| {
                if Rc::ptr_eq(&studio.session, &self.session) {
                    studio.drop_deleted_document(path, window, cx);
                }
            });
        }
        self.session
            .borrow_mut()
            .recent
            .retain(|file| file.path != path);
        self.restore_recent(path, cx);
        self.persist_session();
        self.notify_catalog(cx);
    }

    fn drop_deleted_document(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if self.deleting_document.as_deref() == Some(path) {
            self.finish_document_delete(false, window, cx);
        }
        if self
            .tabs
            .iter()
            .any(|tab| tab.file.path == path && Some(tab.token) == self.active)
        {
            self.select_tab(None, window, cx);
        }
        self.tabs.retain(|tab| tab.file.path != path);
        if self.renaming.as_deref() == Some(path) {
            self.finish_document_rename(false, window, cx);
        }
        self.persist_session();
        cx.notify();
    }

    pub(super) fn document_menu(
        &mut self,
        path: PathBuf,
        delete: Option<bool>,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let weak = cx.entity().downgrade();
        let rename_path = path.clone();
        let rename = ContextMenuItem::action_with(
            |_, _| {
                div()
                    .debug_selector(|| "file-menu-rename".into())
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icon(LucideIcons::Pencil, 15.))
                    .child(t("rename"))
            },
            move |window, cx| {
                let _ = weak.update(cx, |studio, cx| {
                    studio.begin_document_rename(rename_path.clone(), window, cx)
                });
            },
        );
        let mut menu = ContextMenu::new()
            .w(px(206.))
            .text_size(px(12.))
            .text_color(rgb(TEXT))
            .bg(rgb(0x202027))
            .rounded(px(10.))
            .border_color(gpui::rgba(0xffffff20))
            .appearance(ContextMenuAppearance {
                selected_background: gpui::rgba(0xb4a2ee28).into(),
                selected_foreground: rgb(TEXT).into(),
                item_height: px(32.),
                ..Default::default()
            })
            .item(rename);
        if let Some(delete) = delete {
            let enabled = !delete || !self.document_busy(&path, cx);
            let weak = cx.entity().downgrade();
            menu = menu.item(
                ContextMenuItem::action_with(
                    move |_, _| {
                        div()
                            .debug_selector(|| "file-menu-remove".into())
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .opacity(if enabled { 1. } else { 0.4 })
                            .text_color(rgb(if delete { 0xf08e83 } else { TEXT }))
                            .child(icon(
                                if delete {
                                    LucideIcons::Trash2
                                } else {
                                    LucideIcons::X
                                },
                                15.,
                            ))
                            .child(t(if delete {
                                "delete-document"
                            } else {
                                "remove-recent"
                            }))
                    },
                    move |window, cx| {
                        let _ = weak.update(cx, |studio, cx| {
                            if delete {
                                studio.begin_document_delete(path.clone(), window, cx);
                            } else {
                                studio.remove_recent(&path, cx);
                            }
                        });
                    },
                )
                .disabled(!enabled),
            );
        }
        let _ = context_menu::show(menu, position, window, cx);
        cx.notify();
    }

    pub(super) fn begin_document_rename(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self
            .tabs
            .iter()
            .find(|tab| tab.file.path == path)
            .map(|tab| tab.file.title.clone())
            .or_else(|| {
                self.session
                    .borrow()
                    .recent
                    .iter()
                    .find(|file| file.path == path)
                    .map(|file| file.title.clone())
            });
        let Some(name) = name else {
            return;
        };
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.suspend(window, cx));
        }
        self.renaming = Some(path);
        self.rename_input
            .update(cx, |input, cx| input.set_value(name, cx));
        self.rename_input.focus_handle(cx).focus(window, cx);
        let input = self.rename_input.clone();
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| {
                if input.focus_handle(cx).is_focused(window)
                    && let Ok(action) = cx.build_action("text_input::SelectAll", None)
                {
                    window.dispatch_action(action, cx);
                }
            })
        });
        cx.notify();
    }

    pub(super) fn finish_document_rename(
        &mut self,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.renaming.take() else {
            return;
        };
        let title = self.rename_input.read(cx).value().trim().to_owned();
        if commit && !title.is_empty() {
            for file in &mut self.session.borrow_mut().recent {
                if file.path == path {
                    file.title = title.clone();
                }
            }
            for tab in &mut self.tabs {
                if tab.file.path == path {
                    tab.file.title = title.clone();
                }
            }
            for handle in cx
                .windows()
                .into_iter()
                .filter(|handle| handle.window_id().as_u64() != self.window_id)
                .filter_map(|handle| handle.downcast::<Studio>())
            {
                let _ = handle.update(cx, |studio, _, cx| {
                    if Rc::ptr_eq(&studio.session, &self.session) {
                        for tab in &mut studio.tabs {
                            if tab.file.path == path {
                                tab.file.title = title.clone();
                            }
                        }
                        cx.notify();
                    }
                });
            }
            self.persist_session();
        }
        self.focus.focus(window, cx);
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.focus_canvas(window, cx));
        }
        cx.notify();
    }

    pub(super) fn rename_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        gpui::deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(gpui::rgba(0x00000066))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.finish_document_rename(false, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(
                    div()
                        .id("rename-document-dialog")
                        .w(px(340.))
                        .p(px(20.))
                        .rounded(px(14.))
                        .bg(rgb(0x202027))
                        .border_1()
                        .border_color(gpui::rgba(0xffffff20))
                        .shadow_xl()
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                        .child(div().text_size(px(15.)).child(t("rename")))
                        .child(
                            div().debug_selector(|| "document-name-input".into()).child(
                                Input::new(&self.rename_input)
                                    .w_full()
                                    .h(px(36.))
                                    .text_size(px(13.))
                                    .bg(rgb(0x17181f))
                                    .text_color(rgb(TEXT))
                                    .border_color(rgb(BORDER))
                                    .appearance(uic::components::input::InputAppearance {
                                        focus_border: rgb(ACCENT).into(),
                                        caret: rgb(ACCENT).into(),
                                        selection: gpui::rgba(0xb4a2ee44).into(),
                                        ..Default::default()
                                    }),
                            ),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.))
                                .children([false, true].map(|commit| {
                                    div()
                                        .id(if commit {
                                            "confirm-document-name"
                                        } else {
                                            "cancel-document-name"
                                        })
                                        .debug_selector(move || {
                                            if commit {
                                                "confirm-document-name".into()
                                            } else {
                                                "cancel-document-name".into()
                                            }
                                        })
                                        .px(px(14.))
                                        .h(px(32.))
                                        .rounded(px(7.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_size(px(12.))
                                        .cursor_pointer()
                                        .bg(rgb(if commit { 0x785fb4 } else { 0x2c2c36 }))
                                        .child(t(if commit { "rename" } else { "cancel" }))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.finish_document_rename(commit, window, cx)
                                        }))
                                })),
                        ),
                ),
        )
        .with_priority(10)
    }
}
