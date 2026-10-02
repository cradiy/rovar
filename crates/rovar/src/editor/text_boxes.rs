use super::inspector::{icon_button, inspector_section};
use super::*;
use crate::i18n::t;
use crate::scene::text::VerticalAlign;
use gpui::{CursorStyle, TextAlign};

impl Workspace {
    pub(super) fn change_text_color(
        &mut self,
        color: gpui::Rgba,
        alpha_only: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(text) = self.selected_text() {
            let stop = self.inspector.active_stop;
            text.editor.update(cx, |editor, cx| {
                editor.apply_color(color, stop, alpha_only, cx)
            });
        }
    }
    pub(super) fn change_text_style(&mut self, change: StyleChange, cx: &mut Context<Self>) {
        self.history.borrow_mut().break_group();
        if let Some(text) = self.selected_text() {
            text.editor
                .update(cx, |editor, cx| editor.apply_style(change, cx));
        }
        cx.notify();
    }
    pub(super) fn selected_text(&self) -> Option<&TextBox> {
        self.texts.iter().find(|t| Some(t.id) == self.selected_text)
    }
    pub(super) fn selected_text_mut(&mut self) -> Option<&mut TextBox> {
        self.texts
            .iter_mut()
            .find(|t| Some(t.id) == self.selected_text)
    }
    pub(super) fn select_text(&mut self, id: usize, cx: &mut Context<Self>) {
        if !self.layer_editable(id) {
            return;
        }
        if self.selection_ids() != std::collections::BTreeSet::from([id]) {
            self.duplicate = None;
        }
        self.multi_selection.clear();
        self.seal_text_edits(cx);
        self.selected_shape = None;
        self.selected_node = None;
        self.vector_edit = None;
        for text in &self.texts {
            text.editor.update(cx, |editor, _| editor.editing = false);
        }
        if let Some(text) = self.texts.iter().find(|t| t.id == id) {
            self.selected = text.board;
            self.selected_text = Some(id);
            self.sync_fields(cx);
            cx.notify();
        }
    }
    pub(super) fn add_text(
        &mut self,
        board: Option<usize>,
        rect: Rect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_bezier(cx);
        self.draw_tool = None;
        self.seal_text_edits(cx);
        let id = self.next_id;
        self.next_id += 1;
        let text = self.make_text(id, board, rect, window, cx);
        text.editor.read(cx).focus.clone().focus(window, cx);
        self.texts.push(text);
        self.history.borrow_mut().record(
            vec![Change::TextBox {
                id,
                index: self.texts.len() - 1,
                value: None,
            }],
            None,
        );
        self.select_text(id, cx);
        self.texts
            .last()
            .unwrap()
            .editor
            .update(cx, |editor, _| editor.editing = true);
    }
    pub(super) fn make_text(
        &self,
        id: usize,
        board: Option<usize>,
        rect: Rect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TextBox {
        let history = self.history.clone();
        let editor = cx.new(|cx| TextEditor::new(id, history, window, cx));
        // Repaint the owning surface on input without copying text back into it.
        let subscription = cx.observe_in(&editor, window, |this, editor, window, cx| {
            this.auto_layout.revision = None;
            // An IME commit may arrive during a chrome-only resize.
            this.scene.update(cx, |_, cx| cx.notify());
            if this
                .selected_text()
                .is_some_and(|text| text.editor == editor)
            {
                this.sync_text_inspector(window, cx);
            }
            cx.notify();
        });
        TextBox {
            id,
            layer: Default::default(),
            board,
            rect,
            editor,
            _subscription: subscription,
        }
    }
    pub(super) fn text_element(
        &self,
        text: &TextBox,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = text.id;
        let origin = self.parent_origin(text.board);
        let position = self.view.screen(origin + point(text.rect.x, text.rect.y));
        let width = text.rect.width * self.view.zoom;
        let height = text.rect.height * self.view.zoom;
        let selected = self.selected_text == Some(id);
        let element = div()
            .id(("text-box", id))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &gpui::MouseDownEvent, window, cx| {
                    this.open_context_menu(
                        Some(id),
                        false,
                        window.raw_mouse_position(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .debug_selector(move || format!("text-box-{id}"))
            .absolute()
            .left(px(position.x))
            .top(px(position.y))
            .w(px(width))
            .h(px(height))
            .cursor(CursorStyle::OpenHand)
            .when(!self.layer_editable(id), |el| el.cursor(CursorStyle::Arrow))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let event = &gpui::MouseDownEvent {
                        position: window.raw_mouse_position(),
                        ..event.clone()
                    };
                    if this.space_down {
                        this.begin(
                            GestureKind::Pan {
                                original: this.view.pan,
                            },
                            event.position,
                            event.button,
                            window,
                            cx,
                        );
                        return;
                    }
                    if !this.layer_editable(id) {
                        this.focus.focus(window, cx);
                        this.select(None, cx);
                        cx.stop_propagation();
                        return;
                    }
                    if this.selection_pointer(id, event, window, cx) {
                        return;
                    }
                    this.select_text(id, cx);
                    if event.click_count >= 2 {
                        if let Some(text) = this.selected_text() {
                            text.editor.update(cx, |editor, _| editor.editing = true);
                            text.editor.read(cx).focus.clone().focus(window, cx);
                        }
                        cx.stop_propagation();
                        cx.notify();
                    } else if let Some(text) = this.selected_text() {
                        this.begin(
                            GestureKind::Text {
                                id,
                                original: text.rect,
                                handle: None,
                            },
                            event.position,
                            event.button,
                            window,
                            cx,
                        );
                    }
                }),
            )
            .child(crate::scene::text::element(
                &text.editor,
                self.view.zoom,
                self.focus.clone(),
                cx,
            ))
            .when(selected, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .border_1()
                        .border_color(rgb(ACCENT)),
                )
                .children(Handle::ALL.into_iter().enumerate().map(|(index, handle)| {
                    let x = (handle.0 as f32 + 1.) * 0.5 * width;
                    let y = (handle.1 as f32 + 1.) * 0.5 * height;
                    let cursor = rotation::handle_cursor(handle, text.layer.rotation);
                    div()
                        .id(("text-handle", index))
                        .debug_selector(move || format!("text-handle-{index}"))
                        .absolute()
                        .left(px(x - 6.))
                        .top(px(y - 6.))
                        .size(px(12.))
                        .cursor(cursor)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .size(px(7.))
                                .bg(rgb(0xffffff))
                                .border_1()
                                .border_color(rgb(ACCENT)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                let event = &gpui::MouseDownEvent {
                                    position: window.raw_mouse_position(),
                                    ..event.clone()
                                };
                                if let Some(text) = this.selected_text() {
                                    this.begin(
                                        GestureKind::Text {
                                            id,
                                            original: text.rect,
                                            handle: Some(handle),
                                        },
                                        event.position,
                                        event.button,
                                        window,
                                        cx,
                                    );
                                }
                            }),
                        )
                }))
                .children(self.rotation_handles(id, width, height, 0., cx))
            });
        crate::scene::rotation::surface(element, text.layer.rotation, width, height, 0.)
    }
    pub(super) fn text_properties(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let align = self.selected_text().and_then(|t| {
            let editor = t.editor.read(cx);
            (!editor.mixed(TextProperty::Align)).then_some(editor.effective_style().align)
        });
        let vertical_align = self.selected_text().and_then(|t| {
            let editor = t.editor.read(cx);
            (!editor.mixed(TextProperty::VerticalAlign))
                .then_some(editor.effective_style().vertical_align)
        });
        let scope = self
            .selected_text()
            .is_some_and(|t| t.editor.read(cx).has_style_selection());
        div()
            .id("text-properties-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.inspector.inspector_scroll)
            .flex()
            .flex_col()
            .child(self.geometry_controls(cx))
            .child(
                inspector_section(t("typography"))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(if scope {
                                t("apply-selection")
                            } else {
                                t("apply-text-box")
                            }),
                    )
                    .child(self.inspector.font_picker.clone())
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .gap(px(10.))
                            .child(self.property_field(7, t("font-size"), cx))
                            .child(self.property_field(10, t("font-weight"), cx)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .gap(px(10.))
                            .child(self.property_field(8, t("line-height"), cx))
                            .child(self.property_field(9, t("letter-spacing"), cx)),
                    )
                    .child(
                        div().flex_shrink_0().flex().gap(px(4.)).children(
                            [
                                (
                                    TextAlign::Left,
                                    t("text-align-left"),
                                    "align-left",
                                    LucideIcons::TextAlignStart,
                                ),
                                (
                                    TextAlign::Center,
                                    t("text-align-center"),
                                    "align-center",
                                    LucideIcons::TextAlignCenter,
                                ),
                                (
                                    TextAlign::Right,
                                    t("text-align-right"),
                                    "align-right",
                                    LucideIcons::TextAlignEnd,
                                ),
                            ]
                            .into_iter()
                            .map(|(value, label, id, glyph)| {
                                icon_button(id, label, glyph, align == Some(value))
                                    .flex_1()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.change_text_style(StyleChange::Align(value), cx);
                                    }))
                            }),
                        ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap(px(7.))
                            .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(
                                if vertical_align.is_some() {
                                    t("inline-alignment")
                                } else {
                                    t("inline-alignment-mixed")
                                },
                            ))
                            .child(
                                div().flex().gap(px(4.)).children(
                                    [
                                        (
                                            VerticalAlign::Baseline,
                                            t("align-baseline"),
                                            "vertical-baseline",
                                            LucideIcons::Baseline,
                                        ),
                                        (
                                            VerticalAlign::Top,
                                            t("inline-top"),
                                            "vertical-top",
                                            LucideIcons::AlignStartHorizontal,
                                        ),
                                        (
                                            VerticalAlign::Center,
                                            t("inline-center"),
                                            "vertical-center",
                                            LucideIcons::AlignCenterHorizontal,
                                        ),
                                        (
                                            VerticalAlign::Bottom,
                                            t("inline-bottom"),
                                            "vertical-bottom",
                                            LucideIcons::AlignEndHorizontal,
                                        ),
                                    ]
                                    .into_iter()
                                    .map(
                                        |(value, label, id, glyph)| {
                                            let selected = vertical_align == Some(value);
                                            icon_button(id, label, glyph, selected)
                                                .flex_1()
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.change_text_style(
                                                        StyleChange::VerticalAlign(value),
                                                        cx,
                                                    );
                                                }))
                                        },
                                    ),
                                ),
                            ),
                    ),
            )
            .child(inspector_section(t("fill")).child(self.paint_value_row(cx)))
            .child(self.auto_layout_controls(cx))
            .child(self.export_properties(cx))
    }
}
