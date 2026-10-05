//! Editor construction and subscriptions.

use super::*;

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::ui::theme::activate(window, cx);
        let rename_input = cx.new(TextInput::new);
        let mut subscriptions = Vec::new();
        let inspector = inspector::State::new(window, cx);
        let zoom_menu = zoom::ZoomMenu::new(window, cx, &mut subscriptions);
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
        let focus = cx.focus_handle();
        subscriptions.push(cx.observe(
            &uic::components::context_menu::layer(cx),
            |this, menu, cx| {
                if !menu.read(cx).is_open() && this.pick_hover.take().is_some() {
                    cx.notify();
                }
            },
        ));
        subscriptions.push(cx.on_blur(&focus, window, |this, window, cx| {
            let had_corner = this.corner_editor.clear_hover();
            if this.measure_target.take().is_some() || had_corner {
                cx.notify();
            }
            this.space_down = false;
            this.cancel_gesture(window, cx);
        }));
        subscriptions.push(cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                let had_corner = this.corner_editor.clear_hover();
                if this.measure_target.take().is_some() || had_corner {
                    cx.notify();
                }
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
            inspector,
            assets: assets::State::new(window, cx),
            colors: color_styles::State::new(window, cx),
            hierarchy: Default::default(),
            auto_layout: auto_layout::State::new(window, cx),
            components: components::State::default(),
            snapping: Default::default(),
            spacing: layout::Spacing::new(window, cx),
            corner_editor: shapes::Corners::new(window, cx),
            image_crop: None,
            measure_target: None,
            pick_hover: None,
            export: export::ExportState::new(),
            rename_input,
            layer_drag: None,
            layer_row_bounds: Default::default(),
            multi_selection: Default::default(),
            duplicate: None,
            selection_resize: None,
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
            vector_bend: false,
            vector_segment_t: 0.5,
            shape_paths: Default::default(),
            boolean_cache: Default::default(),
            media: Default::default(),
            zoom_menu,
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
            preview: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
            capture: Rc::new(Cell::new(None)),
            gesture: None,
            space_down: false,
            focus,
            _subscriptions: subscriptions,
        }
    }
}
