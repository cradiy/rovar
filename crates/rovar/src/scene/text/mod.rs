mod layout;
pub(crate) use layout::TextFragment;
pub(crate) use layout::export_fragments;
pub(crate) use layout::measure_content;
pub(crate) mod styles;
pub(crate) use styles::StyleChange;
use styles::StyledText;
#[cfg(test)]
mod tests;

use crate::scene::artboard::{FillMode, LinearGradient, Rect};
use crate::scene::history::{Change, Group, SharedHistory};
use gpui::{prelude::*, *};
use layout::TextLayout;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub struct TextBox {
    pub uid: uuid::Uuid,
    pub id: usize,
    pub layer: crate::scene::layer::LayerState,
    pub board: Option<usize>,
    pub rect: Rect,
    pub editor: Entity<TextEditor>,
    pub _subscription: Subscription,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum VerticalAlign {
    #[default]
    Baseline,
    Top,
    Center,
    Bottom,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_style: Option<String>,
    pub family: SharedString,
    pub weight: f32,
    pub size: f32,
    pub line_height: f32,
    pub spacing: f32,
    #[serde(with = "crate::document::rgba")]
    pub color: Rgba,
    pub align: TextAlign,
    pub vertical_align: VerticalAlign,
    pub fill_mode: FillMode,
    pub gradient: LinearGradient,
}

impl Default for TextStyle {
    fn default() -> Self {
        let mut gradient = LinearGradient::default();
        gradient.stop_mut(0).unwrap().color = rgb(0x20232b);
        Self {
            family: crate::ui::font::SYSTEM.into(),
            color_style: None,
            weight: 400.,
            size: 24.,
            line_height: 1.5,
            spacing: 0.,
            color: rgb(0x20232b),
            align: TextAlign::Left,
            vertical_align: VerticalAlign::Baseline,
            fill_mode: FillMode::Solid,
            gradient,
        }
    }
}
impl TextStyle {
    pub fn editable_color(&self, stop: usize) -> Rgba {
        if self.fill_mode == FillMode::Linear {
            self.gradient
                .stop(stop)
                .unwrap_or(&self.gradient.stops()[0])
                .color
        } else {
            self.color
        }
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct Snapshot {
    content: String,
    editing: bool,
    anchor: usize,
    cursor: usize,
    styles: StyledText,
}

impl Snapshot {
    pub(crate) fn visit_color_styles(
        &mut self,
        mut f: impl FnMut(&mut Option<String>, &mut Rgba, &mut FillMode, &mut LinearGradient),
    ) {
        for style in std::iter::once(&mut self.styles.default)
            .chain(self.styles.runs.iter_mut().map(|r| &mut r.style))
        {
            f(
                &mut style.color_style,
                &mut style.color,
                &mut style.fill_mode,
                &mut style.gradient,
            );
        }
    }
    pub(crate) fn from_document(text: crate::document::Text) -> Self {
        Self {
            content: text.content,
            styles: text.styles,
            editing: false,
            anchor: 0,
            cursor: 0,
        }
    }
}

pub struct TextEditor {
    pub focus: FocusHandle,
    pub(crate) content: String,
    styles: StyledText,
    pub editing: bool,
    id: usize,
    history: SharedHistory,
    anchor: usize,
    cursor: usize,
    marked: Option<Range<usize>>,
    composition: Option<Snapshot>,
    selecting: bool,
    layout: Option<TextLayout>,
    layout_dirty: bool,
    content_revision: u64,
    layout_zoom: f32,
    _blur: Subscription,
}

impl TextEditor {
    pub(crate) fn document_text(
        &self,
        id: usize,
        uid: uuid::Uuid,
        board: Option<usize>,
        rect: Rect,
        layer: crate::scene::layer::LayerState,
    ) -> crate::document::Text {
        crate::document::Text {
            uid,
            id,
            board,
            rect,
            layer,
            content: self.content.clone(),
            styles: self.styles.clone(),
        }
    }
    pub(crate) fn load_document_text(&mut self, text: crate::document::Text) {
        self.content = text.content;
        self.styles = text.styles;
        self.anchor = 0;
        self.cursor = 0;
        self.editing = false;
        self.invalidate_layout();
    }
    pub fn new(
        id: usize,
        history: SharedHistory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let blur = cx.on_blur(&focus, window, |this, _, cx| {
            this.finish_composition();
            this.selecting = false;
            this.history.borrow_mut().break_group();
            cx.notify();
        });
        Self {
            focus,
            content: String::new(),
            styles: StyledText::default(),
            editing: true,
            id,
            history,
            anchor: 0,
            cursor: 0,
            marked: None,
            composition: None,
            selecting: false,
            layout: None,
            layout_dirty: true,
            content_revision: 0,
            layout_zoom: 0.,
            _blur: blur,
        }
    }

    pub(crate) fn content_revision(&self) -> u64 {
        self.content_revision
    }

    fn invalidate_layout(&mut self) {
        self.layout_dirty = true;
        self.content_revision = self.content_revision.wrapping_add(1);
    }

    fn layout_for_bounds(
        &mut self,
        zoom: f32,
        bounds: Bounds<Pixels>,
        window: &Window,
    ) -> TextLayout {
        if self.layout_dirty
            || self.layout_zoom != zoom
            || self
                .layout
                .as_ref()
                .is_none_or(|layout| layout.bounds.size.width != bounds.size.width)
        {
            self.layout = Some(TextLayout::new(
                &self.content,
                &self.styles,
                zoom,
                bounds,
                window,
            ));
            self.layout_zoom = zoom;
            self.layout_dirty = false;
        }
        // Moving or resizing the height changes clipping and hit coordinates only.
        let layout = self.layout.as_mut().unwrap();
        layout.bounds = bounds;
        layout.clone()
    }

    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }
    pub fn style_range(&self) -> Range<usize> {
        if self.editing && !self.selection().is_empty() {
            self.selection()
        } else {
            0..self.content.len()
        }
    }
    pub fn has_style_selection(&self) -> bool {
        self.editing && !self.selection().is_empty()
    }
    pub fn effective_style(&self) -> &TextStyle {
        self.styles.at(self.style_range().start)
    }
    pub fn mixed(&self, property: crate::scene::property::TextProperty) -> bool {
        let effective = self.effective_style();
        self.styles
            .in_range(self.style_range())
            .any(|style| !style.same_property(effective, property))
    }
    pub fn apply_color(
        &mut self,
        color: Rgba,
        stop: usize,
        alpha_only: bool,
        cx: &mut Context<Self>,
    ) {
        if self.effective_style().fill_mode == FillMode::Linear {
            let mut gradient = self.effective_style().gradient.clone();
            let stop = if gradient.stop(stop).is_some() {
                stop
            } else {
                gradient.stops()[0].id
            };
            let target = &mut gradient.stop_mut(stop).unwrap().color;
            if alpha_only {
                target.a = color.a;
            } else {
                target.r = color.r;
                target.g = color.g;
                target.b = color.b;
            }
            self.apply_style(StyleChange::Gradient(gradient), cx);
        } else {
            self.apply_style(
                if alpha_only {
                    StyleChange::Opacity(color.a)
                } else {
                    StyleChange::Color(color)
                },
                cx,
            );
        }
    }
    pub fn apply_style(&mut self, change: StyleChange, cx: &mut Context<Self>) {
        self.finish_composition();
        let whole = !self.has_style_selection();
        let mut range = self.style_range();
        if change.is_paragraph() && !whole {
            range.start = self.content[..range.start].rfind('\n').map_or(0, |i| i + 1);
            let end = self.content[..range.end]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
            range.end = self.content[end..]
                .find('\n')
                .map_or(self.content.len(), |i| end + i + 1);
        }
        let group = Group::Style(self.id, change.key(), range.clone());
        let before = (!self.history.borrow().can_merge(Some(&group))).then(|| self.snapshot());
        let old_styles = self.styles.clone();
        self.styles.apply(range, &change, whole);
        if self.styles != old_styles {
            self.invalidate_layout();
            let changes = before
                .map(|value| Change::Text { id: self.id, value })
                .into_iter()
                .collect();
            self.history.borrow_mut().record(changes, Some(group));
            cx.notify();
        }
    }
    // The caller records one atomic history entry for all selected objects.
    pub(crate) fn apply_batch_style(&mut self, change: StyleChange, cx: &mut Context<Self>) {
        self.styles.apply(0..self.content.len(), &change, true);
        self.invalidate_layout();
        cx.notify();
    }
    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            content: self.content.clone(),
            editing: self.editing,
            anchor: self.anchor,
            cursor: self.cursor,
            styles: self.styles.clone(),
        }
    }
    pub(crate) fn restore(&mut self, snapshot: Snapshot) {
        self.invalidate_layout();
        self.content = snapshot.content;
        self.editing = snapshot.editing;
        self.anchor = snapshot.anchor;
        self.cursor = snapshot.cursor;
        self.styles = snapshot.styles;
        self.marked = None;
        self.composition = None;
        self.selecting = false;
    }
    pub(crate) fn is_composing(&self) -> bool {
        self.marked.is_some()
    }
    pub(crate) fn finish_composition(&mut self) {
        if let Some(before) = self.composition.take() {
            if before.content != self.content {
                self.history.borrow_mut().record(
                    vec![Change::Text {
                        id: self.id,
                        value: before,
                    }],
                    None,
                );
            } else {
                self.invalidate_layout();
                self.styles = before.styles;
            }
        }
        self.marked = None;
    }
    fn edit(&mut self, range: Range<usize>, text: &str, group: bool) {
        let composition = self.composition.take();
        let composing = composition.is_some();
        let changed = &self.content[range.clone()] != text;
        if !composing && !changed {
            // A no-op replacement must not flatten mixed styles or discard redo.
            self.cursor = range.start + text.len();
            self.anchor = self.cursor;
            self.marked = None;
            return;
        }
        let group = (group && !composing).then_some(Group::Typing(self.id));
        // During a typing group only the first edit needs a content snapshot.
        let before = composition.or_else(|| {
            (changed && !self.history.borrow().can_merge(group.as_ref())).then(|| self.snapshot())
        });
        self.invalidate_layout();
        self.styles.replace(range.clone(), text.len());
        self.content.replace_range(range.clone(), text);
        if let Some(before) = before {
            if before.content != self.content {
                self.history.borrow_mut().record(
                    vec![Change::Text {
                        id: self.id,
                        value: before,
                    }],
                    group,
                );
            } else if composing {
                self.styles = before.styles;
            }
        } else if changed {
            // Refresh the grouping deadline without duplicating the first snapshot.
            self.history.borrow_mut().record(Vec::new(), group);
        }
        self.cursor = range.start + text.len();
        self.anchor = self.cursor;
        self.marked = None;
    }
    fn previous(&self) -> usize {
        self.content[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }
    fn next(&self) -> usize {
        self.content[self.cursor..]
            .graphemes(true)
            .next()
            .map_or(self.cursor, |g| self.cursor + g.len())
    }
    fn move_cursor(&mut self, offset: usize, extend: bool) {
        self.history.borrow_mut().break_group();
        self.cursor = offset.min(self.content.len());
        if !extend {
            self.anchor = self.cursor;
        }
    }
    fn mouse_index(&self, position: Point<Pixels>) -> usize {
        self.layout.as_ref().map_or(0, |l| l.index(position))
    }
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let command = modifiers.control || modifiers.platform;
        // Let the platform input method consume composition keys first.
        if self.marked.is_some() {
            return;
        }
        if command {
            match key {
                "a" => {
                    self.history.borrow_mut().break_group();
                    self.anchor = 0;
                    self.cursor = self.content.len();
                }
                "c" | "x" => {
                    let selection = self.selection();
                    if !selection.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            self.content[selection.clone()].to_owned(),
                        ));
                        if key == "x" {
                            self.edit(selection, "", false);
                        }
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                        self.edit(self.selection(), &normalize_newlines(&text), false);
                    }
                }
                "home" => self.move_cursor(0, modifiers.shift),
                "end" => self.move_cursor(self.content.len(), modifiers.shift),
                _ => return,
            }
        } else {
            match key {
                "backspace" | "delete" => {
                    let mut range = self.selection();
                    if range.is_empty() {
                        range = if key == "backspace" {
                            self.previous()..self.cursor
                        } else {
                            self.cursor..self.next()
                        };
                    }
                    if !range.is_empty() {
                        self.edit(range, "", false);
                    }
                }
                "enter" => self.edit(self.selection(), "\n", false),
                "tab" => self.edit(self.selection(), "    ", false),
                "left" | "right" => {
                    let range = self.selection();
                    let offset = if !modifiers.shift && !range.is_empty() {
                        if key == "left" {
                            range.start
                        } else {
                            range.end
                        }
                    } else if key == "left" {
                        self.previous()
                    } else {
                        self.next()
                    };
                    self.move_cursor(offset, modifiers.shift);
                }
                "up" | "down" | "home" | "end" => {
                    if let Some(layout) = &self.layout {
                        let offset = layout.navigate(self.cursor, key);
                        self.move_cursor(offset, modifiers.shift);
                    }
                }
                "escape" => {
                    self.editing = false;
                    self.selecting = false;
                    window.blur();
                }
                _ => return,
            }
        }
        // Prevent the canvas from interpreting typing as pan or object deletion.
        cx.stop_propagation();
        cx.notify();
    }
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}
fn from_utf16(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        if units >= offset {
            return index;
        }
        units += ch.len_utf16();
    }
    text.len()
}
fn to_utf16(text: &str, offset: usize) -> usize {
    text[..offset].encode_utf16().count()
}

impl EntityInputHandler for TextEditor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = from_utf16(&self.content, range.start)..from_utf16(&self.content, range.end);
        *actual = Some(to_utf16(&self.content, range.start)..to_utf16(&self.content, range.end));
        Some(self.content[range].to_owned())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let range = self.selection();
        Some(UTF16Selection {
            range: to_utf16(&self.content, range.start)..to_utf16(&self.content, range.end),
            reversed: self.cursor < self.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|r| to_utf16(&self.content, r.start)..to_utf16(&self.content, r.end))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.finish_composition();
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|r| from_utf16(&self.content, r.start)..from_utf16(&self.content, r.end))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.selection());
        let group =
            range.is_empty() && !text.contains(['\n', '\r']) && text.graphemes(true).count() == 1;
        self.edit(range, &normalize_newlines(text), group);
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|r| from_utf16(&self.content, r.start)..from_utf16(&self.content, r.end))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.selection());
        if text.is_empty() {
            // Empty preedit ends composition. Keeping Some(start..start) here
            // would make key_down keep yielding Backspace to an inactive IME.
            self.edit(range, "", false);
            cx.notify();
            return;
        }
        if self.composition.is_none() {
            self.history.borrow_mut().break_group();
            self.composition = Some(self.snapshot());
        }
        self.invalidate_layout();
        self.styles.replace(range.clone(), text.len());
        self.content.replace_range(range.clone(), text);
        self.marked = Some(range.start..range.start + text.len());
        let selected = selected
            .map(|r| from_utf16(text, r.start)..from_utf16(text, r.end))
            .unwrap_or(text.len()..text.len());
        self.anchor = range.start + selected.start;
        self.cursor = range.start + selected.end;
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        Some(layout.caret_bounds(from_utf16(&self.content, range.start)))
    }
    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(to_utf16(&self.content, self.mouse_index(position)))
    }
}

pub fn element(
    editor: &Entity<TextEditor>,
    zoom: f32,
    exit_focus: FocusHandle,
    cx: &App,
) -> impl IntoElement + use<> {
    let focus = editor.read(cx).focus.clone();
    let key = editor.clone();
    let down = editor.clone();
    let prepaint = editor.clone();
    let paint = editor.clone();
    div()
        .size_full()
        .overflow_hidden()
        .track_focus(&focus)
        .key_context("CanvasText")
        .on_key_down(move |event, window, cx| {
            let exit = event.keystroke.key == "escape" && key.read(cx).marked.is_none();
            key.update(cx, |text, cx| text.key_down(event, window, cx));
            if exit {
                exit_focus.focus(window, cx);
            }
        })
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            down.update(cx, |text, cx| {
                if text.focus.is_focused(window) {
                    let index = text.mouse_index(event.position);
                    text.move_cursor(index, event.modifiers.shift);
                    text.selecting = true;
                    cx.stop_propagation();
                    cx.notify();
                }
            });
        })
        .child(
            canvas(
                move |bounds, window, cx| {
                    let layout =
                        prepaint.update(cx, |text, _| text.layout_for_bounds(zoom, bounds, window));
                    (layout, window.insert_hitbox(bounds, HitboxBehavior::Normal))
                },
                move |bounds, (layout, hitbox), window, cx| {
                    let text = paint.read(cx);
                    let selecting = text.selecting;
                    let focus = text.focus.clone();
                    let focused = focus.is_focused(window);
                    layout.paint(
                        if text.editing { text.selection() } else { 0..0 },
                        text.cursor,
                        text.marked.clone(),
                        focused,
                        window,
                        cx,
                    );
                    window.handle_input(
                        &focus,
                        ElementInputHandler::new(bounds, paint.clone()),
                        cx,
                    );
                    if focused {
                        window.set_cursor_style(CursorStyle::IBeam, &hitbox);
                    }
                    if !focused && !selecting {
                        return;
                    }
                    let moving = paint.downgrade();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase != DispatchPhase::Capture {
                            return;
                        }
                        let _ = moving.update(cx, |text, cx| {
                            if text.selecting {
                                if event.pressed_button == Some(MouseButton::Left) {
                                    let index = text.mouse_index(event.position);
                                    if text.cursor == index {
                                        return;
                                    }
                                    text.move_cursor(index, true);
                                } else {
                                    text.selecting = false;
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }
                        });
                    });
                    let ending = paint.downgrade();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                            return;
                        }
                        let _ = ending.update(cx, |text, cx| {
                            if text.selecting {
                                text.move_cursor(text.mouse_index(event.position), true);
                                text.selecting = false;
                                cx.stop_propagation();
                                cx.notify();
                            }
                        });
                    });
                },
            )
            .size_full(),
        )
}
