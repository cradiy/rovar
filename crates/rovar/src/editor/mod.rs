//! Editor state and feature composition. Application windows and shared UI live outside this module.

mod assets;
mod auto_layout;
mod boards;
mod boolean;
mod canvas;
mod color_styles;
mod components;
mod context_menu;
mod creation;
mod document;
mod editing;
mod export;
mod gradient_editor;
mod history;
mod image_crop;
mod initialize;
mod inspector;
mod interaction;
mod layers;
mod layout;
mod mask;
mod media;
mod organization;
mod pages;
mod panels;
mod preview;
mod rotation;
mod selection;
mod shapes;
#[cfg(test)]
mod tests;
mod text_boxes;
mod toolbar;
mod view;
mod zoom;

use creation::{BoxDraft, DrawTool};
use inspector::PROPERTY_COUNT;
use interaction::{Gesture, GestureKind, resize_cursor};
pub(crate) use pages::PageViews;

use crate::scene::artboard::{Artboard, FillMode, Handle, Rect, Viewport};
use crate::scene::history::{Change, Group, SharedHistory};
use crate::scene::property::{Property, TextProperty};
use crate::scene::shape::{Shape, ShapeKind};
use crate::scene::text::{StyleChange, TextBox, TextEditor};
use crate::ui::{ACCENT, BORDER, MUTED, PANEL, TEXT, WORKSPACE, font_picker, icon};
#[cfg(test)]
use gpui::size;
use gpui::{
    Bounds, Context, Div, Entity, FocusHandle, Focusable, FontWeight, HitboxId, IntoElement,
    MouseButton, Pixels, Point, Render, Subscription, Window, div, point, prelude::*, px, rgb,
};
use std::{cell::Cell, rc::Rc};
use uic::assets::LucideIcons;
use uic::components::{
    color_picker::{ColorPickerEvent, ColorPickerState},
    input::{InputEvent, TextInput},
};

pub(crate) struct Workspace {
    inspector: inspector::State,
    pages: pages::State,
    assets: assets::State,
    colors: color_styles::State,
    export: export::ExportState,
    snapping: layout::Snapping,
    spacing: layout::Spacing,
    corner_editor: shapes::Corners,
    image_crop: Option<image_crop::State>,
    measure_target: Option<usize>,
    pick_hover: Option<usize>,
    hierarchy: crate::scene::layer::Hierarchy,
    auto_layout: auto_layout::State,
    components: components::State,
    rename_input: Entity<TextInput>,
    layer_drag: Option<organization::LayerDrag>,
    layer_row_bounds: Rc<std::cell::RefCell<std::collections::HashMap<usize, Bounds<Pixels>>>>,
    multi_selection: std::collections::BTreeSet<usize>,
    duplicate: Option<selection::Duplicate>,
    selection_resize: Option<selection::Resize>,
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
    vector_bend: bool,
    vector_segment_t: f32,
    shape_paths: shapes::ShapePaths,
    boolean_cache: std::cell::RefCell<crate::scene::boolean::Cache>,
    media: media::State,
    draw_tool: Option<DrawTool>,
    box_draft: Option<BoxDraft>,
    toolbar: toolbar::Toolbar,
    sidebar: layers::Sidebar,
    panels: panels::Panels,
    zoom_menu: zoom::ZoomMenu,
    scene: Entity<canvas::CanvasScene>,
    draft: Option<shapes::Draft>,
    path_before: Option<Shape>,
    bezier_draft: Option<shapes::BezierDraft>,
    next_id: usize,
    history: SharedHistory,
    #[cfg(test)]
    pub(crate) snapshot_count: Cell<usize>,
    view: Viewport,
    preview: Option<preview::State>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    capture: Rc<Cell<Option<HitboxId>>>,
    gesture: Option<Gesture>,
    space_down: bool,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}
