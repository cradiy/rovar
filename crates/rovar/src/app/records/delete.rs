use super::*;
use crate::ui::theme::Color;

impl Studio {
    pub(super) fn begin_document_delete(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !files::is_internal(&self.directory, &path)
            || !self
                .session
                .borrow()
                .recent
                .iter()
                .any(|file| file.path == path)
        {
            return;
        }
        self.finish_document_rename(false, window, cx);
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.suspend(window, cx));
        }
        self.deleting_document = Some(path);
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(in crate::app) fn finish_document_delete(
        &mut self,
        confirmed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.deleting_document.take() else {
            return;
        };
        if confirmed {
            // Recheck catalog membership and all windows' pending writes at confirmation.
            self.delete_document(&path, window, cx);
        }
        self.focus.focus(window, cx);
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.focus_canvas(window, cx));
        }
        cx.notify();
    }

    pub(in crate::app) fn document_delete_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let title = self
            .session
            .borrow()
            .recent
            .iter()
            .find(|file| Some(&file.path) == self.deleting_document.as_ref())
            .map(|file| file.title.clone())
            .unwrap_or_default();
        let body = div()
            .id("delete-document-dialog")
            .debug_selector(|| "delete-document-dialog".into())
            .w(px(360.))
            .max_w_full()
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
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icon(LucideIcons::Trash2, 18.).text_color(Color::Danger.color()))
                    .child(div().text_size(px(15.)).child(t("delete-document-title"))),
            )
            .child(div().text_size(px(13.)).truncate().child(title))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .child(t("delete-document-hint")),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .children([false, true].map(|confirm| {
                        let id = if confirm {
                            "confirm-delete-document"
                        } else {
                            "cancel-delete-document"
                        };
                        div()
                            .id(id)
                            .debug_selector(move || id.into())
                            .px(px(14.))
                            .h(px(32.))
                            .rounded(px(7.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.))
                            .cursor_pointer()
                            .bg(if confirm {
                                Color::DangerSurface.color()
                            } else {
                                Color::Input.color()
                            })
                            .hover(move |style| {
                                style.bg(if confirm {
                                    Color::DangerSurface.color()
                                } else {
                                    Color::Hover.color()
                                })
                            })
                            .child(t(if confirm { "delete-document" } else { "cancel" }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.finish_document_delete(confirm, window, cx);
                                cx.stop_propagation();
                            }))
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
                .p(px(20.))
                .on_any_mouse_down(
                    cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                        if event.button == gpui::MouseButton::Left {
                            this.finish_document_delete(false, window, cx);
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(body),
        )
        .with_priority(30)
    }
}
