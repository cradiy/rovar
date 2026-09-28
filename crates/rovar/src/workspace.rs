mod assets;
mod canvas;
mod context_menu;
mod creation;
mod document;
mod editing;
mod export;
mod layout;
mod rotation;
mod selection;
use creation::{BoxDraft, DrawTool};
pub(crate) mod font_picker;
mod history;
mod inspector;
mod layers;
mod media;
mod organization;
pub(crate) mod pages;
mod panels;
mod shapes;
#[cfg(test)]
mod tests;
mod text_boxes;
mod toolbar;

use crate::artboard::{Artboard, FillMode, Handle, Rect, Viewport};
use crate::history::{Change, Group, SharedHistory};
use crate::property::{Property, TextProperty};
use crate::shape::{Shape, ShapeKind};
use crate::text::{StyleChange, TextBox, TextEditor};
#[cfg(test)]
use gpui::size;
use gpui::{
    Bounds, Context, Div, Entity, FocusHandle, Focusable, FontWeight, HitboxId, IntoElement,
    MouseButton, Pixels, Point, Render, Subscription, Window, div, point, prelude::*, px, rgb, svg,
};
use std::{cell::Cell, rc::Rc};
use uic::assets::LucideIcons;
use uic::components::{
    color_picker::{ColorPickerEvent, ColorPickerState},
    input::{InputEvent, TextInput},
};

pub(crate) const PANEL: u32 = 0x1d2026;
const WORKSPACE: u32 = 0x15171c;
pub(crate) const BORDER: u32 = 0x30343d;
pub(crate) const TEXT: u32 = 0xdde0e8;
pub(crate) const MUTED: u32 = 0x959ba9;
pub(crate) const ACCENT: u32 = 0xb4a2ee;
const PROPERTY_COUNT: usize = 18;
// Independent persistent inputs for the sidebar, solid, gradient, and image editors.
const PROPERTY_SURFACES: usize = 4;

fn resize_cursor(handle: Handle) -> gpui::CursorStyle {
    use gpui::CursorStyle;
    match (handle.0, handle.1) {
        (0, _) => CursorStyle::ResizeUpDown,
        (_, 0) => CursorStyle::ResizeLeftRight,
        (-1, -1) | (1, 1) => CursorStyle::ResizeUpLeftDownRight,
        _ => CursorStyle::ResizeUpRightDownLeft,
    }
}

#[derive(Clone, Copy)]
enum GestureKind {
    Rotate {
        id: usize,
        original: f32,
    },
    LayerSort,
    Marquee,
    SelectionMove,
    MultiProperty {
        property: Property,
    },
    Draw,
    Panel {
        side: panels::Side,
        original: f32,
        displayed: f32,
        limit: f32,
    },
    Property {
        index: usize,
        original: f32,
    },
    BezierPlace,
    BezierEdit {
        id: usize,
        index: usize,
        part: usize,
    },
    LineEnd {
        id: usize,
        end: usize,
    },
    Move {
        id: usize,
        original: Rect,
    },
    Resize {
        id: usize,
        original: Rect,
        handle: Handle,
    },
    Text {
        id: usize,
        original: Rect,
        handle: Option<Handle>,
    },
    Shape {
        id: usize,
        original: Rect,
        handle: Option<Handle>,
    },
    Pan {
        original: Point<f32>,
    },
}
#[derive(Clone, Copy)]
struct Gesture {
    kind: GestureKind,
    start: Point<Pixels>,
    button: MouseButton,
}

pub struct Workspace {
    pages: pages::State,
    assets: assets::State,
    export: export::ExportState,
    snapping: layout::Snapping,
    hierarchy: crate::layer::Hierarchy,
    rename_input: Entity<TextInput>,
    layer_drag: Option<organization::LayerDrag>,
    layer_row_bounds: Rc<std::cell::RefCell<std::collections::HashMap<usize, Bounds<Pixels>>>>,
    multi_selection: std::collections::BTreeSet<usize>,
    marquee: Option<selection::Marquee>,
    marquee_additive: bool,
    batch_before: Vec<Change>,
    batch_values: Vec<(usize, f32)>,
    boards: Vec<Artboard>,
    selected: Option<usize>,
    texts: Vec<TextBox>,
    selected_text: Option<usize>,
    shapes: Vec<Shape>,
    selected_shape: Option<usize>,
    selected_node: Option<(usize, usize)>,
    vector_edit: Option<usize>,
    vector_hover: Option<(usize, usize)>,
    image_fill_request: usize,
    image_fill_loading: Option<usize>,
    gradient_menu: Entity<uic::components::dropdown::DropdownState>,
    vector_bend: bool,
    vector_segment_t: f32,
    shape_paths: shapes::ShapePaths,
    media_request: u64,
    media_loading: bool,
    media_decoding: std::collections::BTreeSet<String>,
    media_error: Option<String>,
    videos: std::collections::HashMap<usize, media::VideoRuntime>,
    video_loading: std::collections::BTreeSet<usize>,
    video_playback_generation: u64,
    stroke_editing: bool,
    paint_stops: [usize; 2],
    paint_popovers: [Entity<uic::components::popover::PopoverState>; 2],
    draw_tool: Option<DrawTool>,
    box_draft: Option<BoxDraft>,
    toolbar: toolbar::Toolbar,
    sidebar: layers::Sidebar,
    panels: panels::Panels,
    scene: Entity<canvas::CanvasScene>,
    draft: Option<shapes::Draft>,
    path_before: Option<Shape>,
    bezier_draft: Option<shapes::BezierDraft>,
    next_id: usize,
    history: SharedHistory,
    #[cfg(test)]
    pub(crate) snapshot_count: Cell<usize>,
    view: Viewport,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    capture: Rc<Cell<Option<HitboxId>>>,
    gesture: Option<Gesture>,
    space_down: bool,
    focus: FocusHandle,
    fields: Vec<Entity<TextInput>>,
    name_scroll: Rc<Cell<Pixels>>,
    pending: Vec<(usize, gpui::SharedString)>,
    invalid: [bool; PROPERTY_COUNT * PROPERTY_SURFACES],
    active_stop: usize,
    picker: Entity<ColorPickerState>,
    alpha_picker: Entity<ColorPickerState>,
    font_picker: Entity<font_picker::FontPicker>,
    _subscriptions: Vec<Subscription>,
}

pub(crate) fn icon(glyph: LucideIcons, size: f32) -> gpui::Svg {
    svg().path(glyph).size(px(size)).text_color(rgb(TEXT))
}
impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let fields: Vec<_> = (0..PROPERTY_COUNT * PROPERTY_SURFACES)
            .map(|_| cx.new(TextInput::new))
            .collect();
        fields[0].update(cx, |input, cx| {
            input.set_appearance(
                uic::components::input::InputAppearance {
                    caret: rgb(ACCENT).into(),
                    selection: gpui::rgba(0xb4a2ee44).into(),
                    caret_height: px(16.),
                    ..Default::default()
                },
                cx,
            )
        });
        let paint_popovers = std::array::from_fn(|_| {
            cx.new(|cx| uic::components::popover::PopoverState::new(window, cx))
        });
        let rename_input = cx.new(TextInput::new);
        let picker = cx.new(|cx| ColorPickerState::new(rgb(0xffffff), cx));
        let alpha_picker = cx.new(|cx| ColorPickerState::new(rgb(0xffffff), cx));
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.observe(&fields[0], |_, _, cx| cx.notify()));
        for (index, popover) in paint_popovers.iter().enumerate() {
            subscriptions.push(cx.subscribe_in(
                popover,
                window,
                move |this, _, event: &uic::components::popover::PopoverEvent, window, cx| {
                    if *event == uic::components::popover::PopoverEvent::Opened {
                        this.paint_popovers[1 - index].update(cx, |state, cx| {
                            state.dismiss(
                                uic::components::popover::PopoverDismissReason::Programmatic,
                                false,
                                window,
                                cx,
                            )
                        });
                        if this.selected_shape.is_some() {
                            this.activate_paint(index == 1, cx);
                        }
                    }
                    cx.notify();
                },
            ));
        }
        subscriptions.push(cx.subscribe_in(
            &rename_input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Submit(_)) {
                    this.finish_rename(true, window, cx);
                }
            },
        ));
        subscriptions.push(cx.on_blur(
            &rename_input.focus_handle(cx),
            window,
            |this, window, cx| {
                this.finish_rename(true, window, cx);
            },
        ));
        let font_picker = cx.new(|cx| font_picker::FontPicker::new(window, cx));
        subscriptions.push(cx.subscribe(
            &font_picker,
            |this, _, event: &font_picker::FontChosen, cx| {
                this.history.borrow_mut().break_group();
                if let Some(text) = this.selected_text_mut() {
                    text.editor.update(cx, |editor, cx| {
                        editor.apply_style(StyleChange::Family(event.0.clone()), cx)
                    });
                    cx.notify();
                }
            },
        ));
        for (index, field) in fields.iter().enumerate() {
            subscriptions.push(cx.subscribe(field, move |this, _, event: &InputEvent, cx| {
                this.edit_field(index, event, cx);
            }));
            let focus = field.read(cx).focus_handle(cx);
            subscriptions.push(cx.on_blur(&focus, window, move |this, _, cx| {
                this.history.borrow_mut().break_group();
                this.sync_input_field(index, cx);
            }));
        }
        for (state, alpha_only) in [(&picker, false), (&alpha_picker, true)] {
            subscriptions.push(cx.subscribe(
                state,
                move |this, _, event: &ColorPickerEvent, cx| {
                    let (ColorPickerEvent::Preview(color) | ColorPickerEvent::Commit(color)) =
                        *event;
                    if !this.multi_selection.is_empty() {
                        let index = if alpha_only { 6 } else { 5 };
                        let value = if alpha_only {
                            inspector::number(color.a * 100.)
                        } else {
                            inspector::hex(color)
                        };
                        this.edit_multi_field(
                            if alpha_only {
                                Property::Opacity
                            } else {
                                Property::Color
                            },
                            &value,
                            cx,
                        );
                        if matches!(event, ColorPickerEvent::Commit(_)) {
                            this.history.borrow_mut().break_group();
                        }
                        this.sync_field(index, cx);
                        if let Some(color) = this.multi_picker_color(cx) {
                            this.sync_picker(color, cx);
                        }
                        return;
                    }
                    let stop = this.active_stop;
                    let target = this.selected_text.or(this.selected_shape).or(this.selected);
                    if let Some(id) = target {
                        this.history
                            .borrow_mut()
                            .set_scope(Group::Color(id, stop), true);
                    }
                    if this.selected_text.is_some() {
                        this.change_text_color(color, alpha_only, cx);
                    } else if this.selected_shape.is_some() {
                        let stroke = this.stroke_editing;
                        this.edit_shape(|shape| {
                            if let Some(target) = shape.paint_color_mut(stop, stroke) {
                                if alpha_only {
                                    target.a = color.a;
                                } else {
                                    target.r = color.r;
                                    target.g = color.g;
                                    target.b = color.b;
                                }
                            }
                        });
                    } else {
                        this.edit_board(None, |board| {
                            if let Some(target) = board.editable_color_mut(stop) {
                                if alpha_only {
                                    target.a = color.a;
                                } else {
                                    target.r = color.r;
                                    target.g = color.g;
                                    target.b = color.b;
                                }
                            }
                        });
                    }
                    this.history.borrow_mut().clear_scope();
                    if matches!(event, ColorPickerEvent::Commit(_)) {
                        this.history.borrow_mut().break_group();
                    }
                    this.sync_field(
                        if this.selected_shape.is_some() && this.stroke_editing {
                            if alpha_only { 17 } else { 16 }
                        } else {
                            if alpha_only { 6 } else { 5 }
                        },
                        cx,
                    );
                    this.sync_picker(color, cx);
                    cx.notify();
                },
            ));
        }
        let focus = cx.focus_handle();
        subscriptions.push(cx.on_blur(&focus, window, |this, window, cx| {
            this.space_down = false;
            this.cancel_gesture(window, cx);
        }));
        subscriptions.push(cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.space_down = false;
                this.cancel_gesture(window, cx);
            }
        }));
        let toolbar = toolbar::Toolbar::new(window, cx);
        let workspace = cx.entity().downgrade();
        let scene = cx.new(|_| canvas::CanvasScene::new(workspace));
        let pages = pages::State::new(window, cx);
        let history = SharedHistory::default();
        history.borrow_mut().set_page(pages.active.clone());
        Self {
            pages,
            assets: assets::State::new(window, cx),
            hierarchy: Default::default(),
            snapping: Default::default(),
            export: export::ExportState::new(window, cx),
            rename_input,
            layer_drag: None,
            layer_row_bounds: Default::default(),
            multi_selection: Default::default(),
            marquee: None,
            marquee_additive: false,
            batch_before: Vec::new(),
            batch_values: Vec::new(),
            scene,
            toolbar,
            sidebar: Default::default(),
            panels: Default::default(),
            boards: Vec::new(),
            selected: None,
            texts: Vec::new(),
            selected_text: None,
            shapes: Vec::new(),
            selected_shape: None,
            selected_node: None,
            vector_edit: None,
            vector_hover: None,
            image_fill_request: 0,
            image_fill_loading: None,
            gradient_menu: cx.new(|cx| uic::components::dropdown::DropdownState::new(window, cx)),
            vector_bend: false,
            vector_segment_t: 0.5,
            shape_paths: Default::default(),
            name_scroll: Rc::new(Cell::new(px(0.))),
            media_request: 0,
            media_loading: false,
            media_decoding: Default::default(),
            media_error: None,
            videos: Default::default(),
            video_loading: Default::default(),
            video_playback_generation: 0,
            stroke_editing: false,
            paint_stops: [0; 2],
            paint_popovers,
            draw_tool: None,
            box_draft: None,
            draft: None,
            path_before: None,
            bezier_draft: None,
            next_id: 1,
            history,
            #[cfg(test)]
            snapshot_count: Cell::new(0),
            view: Viewport::default(),
            bounds: Rc::new(Cell::new(Bounds::default())),
            capture: Rc::new(Cell::new(None)),
            gesture: None,
            space_down: false,
            focus,
            fields,
            pending: Vec::new(),
            invalid: [false; PROPERTY_COUNT * PROPERTY_SURFACES],
            active_stop: 0,
            picker,
            alpha_picker,
            font_picker,
            _subscriptions: subscriptions,
        }
    }

    fn selected_board(&self) -> Option<&Artboard> {
        self.boards.iter().find(|b| Some(b.id) == self.selected)
    }
    fn select(&mut self, id: Option<usize>, cx: &mut Context<Self>) {
        if id.is_some_and(|id| !self.layer_editable(id)) {
            return;
        }
        self.multi_selection.clear();
        self.seal_text_edits(cx);
        for text in &self.texts {
            text.editor.update(cx, |editor, _| editor.editing = false);
        }
        self.selected_text = None;
        self.selected_shape = None;
        self.selected_node = None;
        self.vector_edit = None;
        self.vector_hover = None;
        if self.selected != id {
            self.active_stop = self
                .boards
                .iter()
                .find(|b| Some(b.id) == id)
                .map(|b| b.gradient.stops()[0].id)
                .unwrap_or(0);
        }
        self.selected = id;
        self.invalid.fill(false);
        self.sync_fields(cx);
        cx.notify();
    }
    fn add_artboard(&mut self, rect: Rect, cx: &mut Context<Self>) {
        self.finish_bezier(cx);
        self.draw_tool = None;
        self.seal_text_edits(cx);
        let id = self.next_id;
        self.next_id += 1;
        self.boards.push(Artboard {
            id,
            layer: Default::default(),
            name: crate::i18n::message("frame-name", &[("id", id.to_string())]),
            image_fill: Default::default(),
            rect,
            color: rgb(0xffffff),
            fill_mode: FillMode::Solid,
            gradient: Default::default(),
        });
        self.history.borrow_mut().record(
            vec![Change::Board {
                id,
                index: self.boards.len() - 1,
                value: None,
            }],
            None,
        );
        self.select(Some(id), cx);
    }
    fn fit_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(board) = self.selected_board() {
            let rect = board.rect;
            let size = self.bounds.get().size;
            let width = f32::from(size.width).max(100.);
            let height = f32::from(size.height).max(100.);
            self.view.zoom = ((width - 96.) / rect.width)
                .min((height - 120.) / rect.height)
                .clamp(0.1, 1.);
            self.view.pan = point(
                width / 2. - (rect.x + rect.width / 2.) * self.view.zoom,
                height / 2. - (rect.y + rect.height / 2.) * self.view.zoom,
            );
            cx.notify();
        }
    }
    fn begin(
        &mut self,
        kind: GestureKind,
        position: Point<Pixels>,
        button: MouseButton,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = match kind {
            GestureKind::Rotate { id, .. } => Some(id),
            GestureKind::Move { id, .. }
            | GestureKind::Resize { id, .. }
            | GestureKind::Text { id, .. }
            | GestureKind::Shape { id, .. }
            | GestureKind::LineEnd { id, .. }
            | GestureKind::BezierEdit { id, .. } => Some(id),
            _ => None,
        };
        if target.is_some_and(|id| !self.layer_editable(id)) {
            cx.stop_propagation();
            return;
        }
        self.seal_text_edits(cx);
        self.begin_snapping(kind);
        self.gesture = Some(Gesture {
            kind,
            start: position,
            button,
        });
        // A numeric drag inside a color popover must not dismiss its own editor.
        // Pointer capture still routes moves/releases outside the popover to the canvas.
        let popover_focus = matches!(kind, GestureKind::Property { .. })
            .then(|| {
                self.paint_popovers.iter().find_map(|popover| {
                    let state = popover.read(cx);
                    let focus = state.focus_handle(cx);
                    (state.is_open() && focus.contains_focused(window, cx)).then_some(focus)
                })
            })
            .flatten();
        popover_focus
            .unwrap_or_else(|| self.focus.clone())
            .focus(window, cx);
        if let Some(hitbox) = self.capture.get() {
            window.capture_pointer(hitbox);
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn move_gesture(&mut self, position: Point<Pixels>, shift: bool, cx: &mut Context<Self>) {
        let Some(gesture) = self.gesture else { return };
        let delta = point(
            f32::from(position.x - gesture.start.x),
            f32::from(position.y - gesture.start.y),
        );
        let snapped = self.snap_delta(delta / self.view.zoom);
        match gesture.kind {
            GestureKind::Rotate { id, original } => {
                self.move_rotation(id, original, gesture.start, position, shift, cx)
            }
            GestureKind::LayerSort => self.move_layer_sort(position),
            GestureKind::Marquee => self.move_marquee(position, cx),
            GestureKind::SelectionMove => self.move_selection(snapped),
            GestureKind::MultiProperty { property } => {
                self.move_multi_property(property, delta.x, shift, cx)
            }
            GestureKind::Panel {
                side,
                displayed,
                limit,
                ..
            } => {
                let width = displayed
                    + if side == panels::Side::Left {
                        delta.x
                    } else {
                        -delta.x
                    };
                let width = width.clamp(side.minimum(), limit);
                if self.panels.width(side) == width {
                    return;
                }
                self.panels.set(side, width);
            }
            GestureKind::Property { index, original } => {
                self.scrub_property(index, original, delta.x, shift, cx)
            }
            GestureKind::Draw => self.move_drawing(position, shift),
            GestureKind::BezierPlace => self.move_bezier_place(position, shift),
            GestureKind::BezierEdit { id, index, part } => {
                self.move_bezier_node(id, index, part, position, shift, cx)
            }
            GestureKind::LineEnd { id, end } => {
                self.move_line_endpoint(id, end, position, shift, cx)
            }
            GestureKind::Pan { original } => self.view.pan = original + delta,
            GestureKind::Text {
                id,
                original,
                handle,
            }
            | GestureKind::Shape {
                id,
                original,
                handle,
            } => {
                let rect = if let Some(handle) = handle {
                    self.resize_with_snapping(id, original, handle, delta / self.view.zoom, shift)
                } else {
                    Rect {
                        x: original.x + snapped.x,
                        y: original.y + snapped.y,
                        ..original
                    }
                };
                if let Some((parent, _)) = self.object_rect(id) {
                    self.set_object_rect(id, parent, rect);
                }
                self.sync_fields(cx);
            }
            GestureKind::Move { id, original } | GestureKind::Resize { id, original, .. } => {
                let rect = if let GestureKind::Resize { handle, .. } = gesture.kind {
                    self.resize_with_snapping(id, original, handle, delta / self.view.zoom, shift)
                } else {
                    Rect {
                        x: original.x + snapped.x,
                        y: original.y + snapped.y,
                        ..original
                    }
                };
                self.set_object_rect(id, None, rect);
                self.sync_fields(cx);
            }
        }
        cx.notify();
    }
    fn cancel_gesture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.snapping.clear();
        if let Some(gesture) = self.gesture.take() {
            match gesture.kind {
                GestureKind::Rotate { id, original } => {
                    if let Some(layer) = self.layer_state_mut(id) {
                        layer.rotation = original;
                    }
                }
                GestureKind::LayerSort => self.layer_drag = None,
                GestureKind::Marquee => {
                    if let Some(m) = self.marquee.take() {
                        self.set_selection(m.initial, cx);
                    }
                }
                GestureKind::SelectionMove | GestureKind::MultiProperty { .. } => {
                    let before = std::mem::take(&mut self.batch_before);
                    self.restore_batch(&before, cx);
                    self.batch_values.clear();
                }
                GestureKind::Panel { side, original, .. } => self.panels.set(side, original),
                GestureKind::Property { .. } => self.finish_property_scrub(false, cx),
                GestureKind::Draw => {
                    self.box_draft = None;
                    self.draft = None;
                    self.shape_paths.borrow_mut().remove(&0);
                }
                GestureKind::BezierPlace => self.cancel_bezier_place(),
                GestureKind::LineEnd { id, .. } | GestureKind::BezierEdit { id, .. } => {
                    if let Some(before) = self.path_before.take()
                        && let Some(shape) = self.shapes.iter_mut().find(|s| s.id == id)
                    {
                        *shape = before;
                    }
                }
                GestureKind::Pan { original } => self.view.pan = original,
                GestureKind::Text { id, original, .. } => {
                    if let Some(text) = self.texts.iter_mut().find(|t| t.id == id) {
                        text.rect = original;
                    }
                }
                GestureKind::Shape { id, original, .. } => {
                    if let Some(shape) = self.shapes.iter_mut().find(|s| s.id == id) {
                        shape.rect = original;
                    }
                }
                GestureKind::Move { id, original } | GestureKind::Resize { id, original, .. } => {
                    if let Some(board) = self.boards.iter_mut().find(|b| b.id == id) {
                        board.rect = original;
                    }
                }
            }
            window.release_pointer();
            self.sync_fields(cx);
            cx.notify();
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_video_visibility(cx);
        self.load_visible_media(window, cx);
        self.panels.window_width = f32::from(window.viewport_size().width);
        let drag_cursor = self.gesture.map(|gesture| match gesture.kind {
            GestureKind::Rotate { .. } => gpui::CursorStyle::Crosshair,
            GestureKind::LayerSort => gpui::CursorStyle::ClosedHand,
            GestureKind::Panel { .. }
            | GestureKind::Property { .. }
            | GestureKind::MultiProperty { .. } => gpui::CursorStyle::ResizeLeftRight,
            GestureKind::Resize { handle, .. } => resize_cursor(handle),
            GestureKind::Text {
                id,
                handle: Some(handle),
                ..
            }
            | GestureKind::Shape {
                id,
                handle: Some(handle),
                ..
            } => rotation::handle_cursor(handle, self.object_rotation(id)),
            GestureKind::Marquee => gpui::CursorStyle::Arrow,
            GestureKind::BezierEdit { .. } if self.vector_edit.is_some() => {
                gpui::CursorStyle::Arrow
            }
            GestureKind::Draw
            | GestureKind::BezierPlace
            | GestureKind::BezierEdit { .. }
            | GestureKind::LineEnd { .. } => gpui::CursorStyle::Crosshair,
            GestureKind::SelectionMove
            | GestureKind::Move { .. }
            | GestureKind::Text { handle: None, .. }
            | GestureKind::Shape { handle: None, .. }
            | GestureKind::Pan { .. } => gpui::CursorStyle::OpenHand,
        });
        div()
            .relative()
            .when_some(drag_cursor, |el, cursor| {
                // Every drag keeps its interaction cursor outside the starting hitbox. The
                // request disappears on the first frame after release/cancel.
                el.on_paint_before_children(move |_, _, window, _| {
                    window.set_window_cursor_style(cursor);
                })
            })
            .on_modifiers_changed(cx.listener(
                |this, event: &gpui::ModifiersChangedEvent, window, cx| {
                    if this.gesture.is_some_and(|g| {
                        matches!(
                            g.kind,
                            GestureKind::SelectionMove
                                | GestureKind::Rotate { .. }
                                | GestureKind::Move { .. }
                                | GestureKind::Resize { .. }
                                | GestureKind::Text { .. }
                                | GestureKind::Shape { .. }
                                | GestureKind::Draw
                                | GestureKind::LineEnd { .. }
                        )
                    }) {
                        this.snapping.bypass = event.modifiers.alt;
                        this.move_gesture(window.mouse_position(), event.modifiers.shift, cx);
                    }
                },
            ))
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if let Some(id) = this.pages.delete.clone() {
                    if event.keystroke.key == "escape" {
                        this.pages.delete = None;
                        cx.notify();
                    } else if event.keystroke.key == "enter" {
                        this.delete_page(&id, window, cx);
                    }
                    cx.stop_propagation();
                    return;
                }
                if this.pages.renaming.is_some() {
                    if event.keystroke.key == "escape" {
                        this.finish_page_rename(false, window, cx);
                        cx.stop_propagation();
                    }
                    return;
                }
                if this.assets.dialog.is_some() {
                    if event.keystroke.key == "escape" {
                        this.cancel_asset_dialog(window, cx);
                        cx.stop_propagation();
                    } else if event.keystroke.key == "enter"
                        && matches!(this.assets.dialog, Some(assets::Dialog::Delete(_)))
                    {
                        this.confirm_asset_dialog(window, cx);
                        cx.stop_propagation();
                    }
                    return;
                }
                if uic::components::context_menu::is_open(cx) {
                    return;
                }
                if this.gesture.is_some_and(|g| {
                    matches!(
                        g.kind,
                        GestureKind::LayerSort
                            | GestureKind::Property { .. }
                            | GestureKind::Panel { .. }
                            | GestureKind::MultiProperty { .. }
                    )
                }) {
                    if event.keystroke.key == "escape"
                        || ((event.keystroke.modifiers.control
                            || event.keystroke.modifiers.platform)
                            && event.keystroke.key == "z")
                    {
                        this.cancel_gesture(window, cx);
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.gradient_menu.read(cx).is_open() {
                    this.gradient_menu
                        .update(cx, |state, cx| state.close(window, cx));
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape"
                    && this
                        .paint_popovers
                        .iter()
                        .any(|popover| popover.read(cx).is_open())
                {
                    for popover in &this.paint_popovers {
                        popover.update(cx, |state, cx| state.close(window, cx));
                    }
                    cx.stop_propagation();
                } else {
                    this.history_key(event, window, cx);
                }
            }))
            .on_any_mouse_down(cx.listener(|this, _, window, cx| {
                this.close_tool_menus(window, cx);
            }))
            .size_full()
            .overflow_hidden()
            .bg(rgb(PANEL))
            .text_color(rgb(TEXT))
            .font_family(crate::ui_font::family(cx))
            .flex()
            .flex_col()
            .child(div().flex_1().min_h_0().flex().child(self.editor(cx)))
            .child(uic::components::context_menu::layer(cx))
            .when(self.assets.dialog.is_some(), |el| {
                el.child(self.asset_dialog(cx))
            })
            .when(self.pages.delete.is_some(), |el| {
                el.child(self.page_delete_dialog(cx))
            })
    }
}
