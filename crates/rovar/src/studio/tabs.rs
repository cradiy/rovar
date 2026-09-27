use super::*;
mod drag;
mod hover;
mod state;
#[cfg(test)]
mod tests;
pub(super) use state::*;

impl Studio {
    pub(super) fn drag_payload(
        &self,
        token: usize,
        window: &Window,
        cx: &Context<Self>,
    ) -> DragTab {
        let index = self.tabs.iter().position(|tab| tab.token == token).unwrap();
        let tab = &self.tabs[index];
        DragTab {
            token,
            title: tab.file.title.clone(),
            source: cx.entity().downgrade(),
            window: window.window_handle().downcast::<Studio>().unwrap(),
            width: self.tab_width(f32::from(window.viewport_size().width), self.tabs.len()),
            thumbnail: tab
                .file
                .preview
                .as_ref()
                .map(|name| self.directory.join("previews").join(name)),
            preview: Rc::new(RefCell::new(None)),
            transaction: Rc::new(RefCell::new(DragTransaction {
                origin_index: index,
                origin_selected: self.active,
                cursor_offset_x: 0.,
                native: false,
                owner: Some((self.window_id, token)),
                detached: None,
            })),
        }
    }

    pub(super) fn tab_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        self.animate_tabs(window);
        let count = self.tabs.len() + usize::from(self.strip.snap_index.is_some());
        let viewport = f32::from(window.viewport_size().width);
        let width = self.tab_width(viewport, count);
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                self.render_tab(tab, index, width, window, cx)
                    .into_any_element()
            })
            .collect();
        let snap = self
            .strip
            .drag
            .as_ref()
            .zip(self.strip.snap_left)
            .map(|(drag, left)| {
                tab_face(&drag.title, cx)
                    .absolute()
                    .left(px(left))
                    .top(px(TAB_TOP))
                    .w(px(width))
                    .h(px(TAB_HEIGHT))
                    .into_any_element()
            });
        div()
            .h(px(BAR_HEIGHT))
            .flex_shrink_0()
            .relative()
            .bg(rgb(0x111216))
            .border_b_1()
            .border_color(gpui::rgba(0xffffff0d))
            .child(crate::titlebar::drag_region().absolute().inset_0())
            .on_drop(cx.listener(|this, drag: &DragTab, window, cx| {
                this.dropped_on_strip(drag, window, cx);
                cx.stop_propagation();
            }))
            .child(
                div()
                    .absolute()
                    .left(px(10. + self.chrome.left_inset()))
                    .occlude()
                    .top_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(self.app_menu(cx))
                    .child(
                        div()
                            .id("home-tab")
                            .debug_selector(|| "home-tab".into())
                            .w(px(36.))
                            .h(px(32.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .when(self.active.is_none(), |s| s.bg(rgb(0x282332)))
                            .when(self.active.is_some(), |el| {
                                el.hover(|s| s.bg(gpui::rgba(0xffffff08)))
                            })
                            .child(icon(LucideIcons::House, 17.).text_color(rgb(
                                if self.active.is_none() {
                                    0xcab8ec
                                } else {
                                    0x9b97a6
                                },
                            )))
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.select_tab(None, window, cx)
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .id("document-tabs")
                    .debug_selector(|| "document-tabs".into())
                    .absolute()
                    .left(px(self.tab_left()))
                    .right(px(self.tab_right_reserved() - 32.))
                    .top_0()
                    .h_full()
                    .overflow_x_scroll()
                    .track_scroll(&self.tab_scroll)
                    .child(
                        div()
                            .relative()
                            .h_full()
                            .w(px((slot_left(count, width) + 32.).max(
                                viewport - self.tab_left() - self.tab_right_reserved() + 32.,
                            )))
                            .children(tabs)
                            .children(snap)
                            .child(
                                div()
                                    .id("new-tab")
                                    .occlude()
                                    .debug_selector(|| "new-tab".into())
                                    .absolute()
                                    .left(px(slot_left(count, width)))
                                    .top(px(9.))
                                    .size(px(28.))
                                    .rounded(px(6.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x353044)))
                                    .child(icon(LucideIcons::Plus, 17.))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.new_document(window, cx)
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .right(px(10. + self.chrome.controls_width()))
                    .occlude()
                    .top_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .children(
                        self.active
                            .map(|_| {
                                [false, true].map(|redo| {
                                    let enabled = self
                                        .active_editor()
                                        .is_some_and(|editor| editor.read(cx).can_undo_redo(redo));
                                    div()
                                        .id(if redo {
                                            "document-redo"
                                        } else {
                                            "document-undo"
                                        })
                                        .debug_selector(move || {
                                            format!(
                                                "document-{}-{}",
                                                if redo { "redo" } else { "undo" },
                                                if enabled { "enabled" } else { "disabled" }
                                            )
                                        })
                                        .size(px(30.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(6.))
                                        .cursor(gpui::CursorStyle::Arrow)
                                        .child(
                                            icon(
                                                if redo {
                                                    LucideIcons::Redo2
                                                } else {
                                                    LucideIcons::Undo2
                                                },
                                                16.,
                                            )
                                            .text_color(rgb(if enabled { TEXT } else { 0x52515c })),
                                        )
                                        .when(enabled, |el| {
                                            el.cursor_pointer()
                                                .hover(|style| style.bg(rgb(0x292531)))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        if let Some(editor) = this.active_editor() {
                                                            editor.update(cx, |editor, cx| {
                                                                if editor.can_undo_redo(redo) {
                                                                    editor.undo_redo(
                                                                        redo, window, cx,
                                                                    );
                                                                }
                                                            });
                                                        }
                                                    },
                                                ))
                                        })
                                })
                            })
                            .into_iter()
                            .flatten(),
                    )
                    .child(self.language_control(cx)),
            )
            .when(self.chrome.controls_width() > 0., |el| {
                el.child(self.window_controls(window, cx))
            })
    }

    fn render_tab(
        &self,
        tab: &Tab,
        index: usize,
        width: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let token = tab.token;
        let rename_path = tab.file.path.clone();
        let menu_path = rename_path.clone();
        let active = self.strip.snap_index.is_none() && self.active == Some(token);
        let dragging = self.dragging == Some(token)
            && self
                .strip
                .drag
                .as_ref()
                .is_some_and(|drag| !drag.transaction.borrow().native);
        let left = self
            .strip
            .slots
            .get(&token)
            .map_or(slot_left(index, width), |slot| slot.current);
        let payload = self.drag_payload(token, window, cx);
        div()
            .id(("document-tab", token))
            .debug_selector(move || format!("document-tab-{token}"))
            .absolute()
            .occlude()
            .left(px(left))
            .top(px(TAB_TOP))
            .w(px(width))
            .h(px(TAB_HEIGHT))
            .px(px(12.))
            .rounded_tl(px(8.))
            .rounded_tr(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .opacity(if dragging { 0. } else { 1. })
            .cursor(if dragging {
                gpui::CursorStyle::ClosedHand
            } else {
                gpui::CursorStyle::OpenHand
            })
            .bg(rgb(if active { 0x292531 } else { 0x111216 }))
            .text_color(rgb(if active { TEXT } else { MUTED }))
            .hover(|s| s.bg(rgb(if active { 0x302a3a } else { 0x202027 })))
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                this.hover_tab(token, *hovered, cx);
            }))
            .child(icon(LucideIcons::PenTool, 13.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .child(tab.file.title.clone()),
            )
            .when(tab.saving || tab.error.is_some(), |el| {
                el.child(
                    div()
                        .text_color(rgb(if tab.error.is_some() {
                            0xf08e83
                        } else {
                            ACCENT
                        }))
                        .child(if tab.error.is_some() { "!" } else { "·" }),
                )
            })
            .child(
                div()
                    .id(("close-tab", token))
                    .debug_selector(move || format!("close-tab-{token}"))
                    .size(px(20.))
                    .flex_shrink_0()
                    .rounded(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(rgb(0x424550)))
                    .child(icon(LucideIcons::X, 12.))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_tab(token, window, cx);
                        cx.stop_propagation();
                    })),
            )
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    this.select_tab(Some(token), window, cx);
                    if event.click_count() == 2 {
                        this.begin_document_rename(rename_path.clone(), window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.document_menu(menu_path.clone(), None, event.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                gpui::MouseButton::Middle,
                cx.listener(move |this, _, window, cx| {
                    this.close_tab(token, window, cx);
                    cx.stop_propagation();
                }),
            )
            .when(
                !tab.saving
                    && !tab.loading
                    && !tab.exporting
                    && tab
                        .editor
                        .as_ref()
                        .is_none_or(|editor| editor.read(cx).can_transfer()),
                |el| {
                    el.on_drag(payload, |drag, offset, window, cx| {
                        drag.transaction.borrow_mut().cursor_offset_x = f32::from(offset.x);
                        let _ = drag.source.update(cx, |source, cx| {
                            source.begin_tab_drag(drag.clone(), window, cx)
                        });
                        let preview = cx.new(|_| DragPreview {
                            title: drag.title.clone(),
                            thumbnail: drag.thumbnail.clone(),
                            width: drag.width,
                            detached: false,
                            hidden: false,
                            cursor_offset_y: f32::from(offset.y),
                            y_offset: 0.,
                        });
                        *drag.preview.borrow_mut() = Some(preview.clone());
                        preview
                    })
                },
            )
            .on_drag_end::<DragTab>(|event, drag, _, cx| {
                let event = *event;
                let drag = drag.clone();
                cx.defer(move |cx| Self::finish_tab_drag(drag, event, cx));
            })
    }
}
