use super::*;
use crate::i18n::t;
use uic::components::dropdown::{DropdownPlacement, DropdownState, dropdown};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tool {
    Move,
    Hand,
    Board,
    Rectangle,
    Ellipse,
    Arrow,
    Polygon,
    Star,
    Media,
    Line,
    Bezier,
    Pencil,
    Text,
}
impl Tool {
    fn details(self) -> (&'static str, &'static str, &'static str, LucideIcons) {
        match self {
            Self::Move => ("tool-move", t("tool-move"), "V", LucideIcons::MousePointer2),
            Self::Hand => ("tool-hand", t("tool-hand"), "H", LucideIcons::Hand),
            Self::Board => ("add-artboard", t("artboard"), "F", LucideIcons::Frame),
            Self::Rectangle => (
                "add-rectangle",
                t("shape-rectangle"),
                "R",
                LucideIcons::Square,
            ),
            Self::Ellipse => ("add-ellipse", t("shape-ellipse"), "O", LucideIcons::Circle),
            Self::Arrow => (
                "draw-arrow",
                t("shape-arrow"),
                "Shift+L",
                LucideIcons::ArrowUpRight,
            ),
            Self::Polygon => ("add-polygon", t("shape-polygon"), "", LucideIcons::Triangle),
            Self::Star => ("add-star", t("shape-star"), "", LucideIcons::Star),
            Self::Media => (
                "import-media",
                t("tool-media"),
                "Ctrl+Shift+K",
                LucideIcons::Image,
            ),
            Self::Line => ("draw-line", t("shape-line"), "L", LucideIcons::Minus),
            Self::Bezier => ("draw-bezier", t("tool-pen"), "P", LucideIcons::PenTool),
            Self::Pencil => ("draw-pen", t("tool-pencil"), "Shift+P", LucideIcons::Pencil),
            Self::Text => ("add-text", t("text-box"), "T", LucideIcons::Type),
        }
    }
}

pub(super) struct Toolbar {
    pub hand: bool,
    shape: Tool,
    pen: Tool,
    menus: [Entity<DropdownState>; 3],
}
impl Toolbar {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        Self {
            hand: false,
            shape: Tool::Rectangle,
            pen: Tool::Bezier,
            menus: std::array::from_fn(|_| cx.new(|cx| DropdownState::new(window, cx))),
        }
    }
}

pub(super) struct ToolTip(pub String);
impl Render for ToolTip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.))
            .py(px(7.))
            .rounded(px(7.))
            .bg(rgb(0x292c34))
            .border_1()
            .border_color(rgb(BORDER))
            .text_size(px(12.))
            .text_color(rgb(TEXT))
            .child(self.0.clone())
    }
}

impl Workspace {
    pub(super) fn close_tool_menus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for menu in &self.toolbar.menus {
            if menu.read(cx).is_open() {
                menu.update(cx, |menu, cx| menu.close(window, cx));
            }
        }
    }

    pub(super) fn choose_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tool_menus(window, cx);
        self.cancel_gesture(window, cx);
        self.media_request += 1;
        self.media_loading = false;
        self.media_error = None;
        if tool == Tool::Media {
            self.import_media(window, cx);
            return;
        }
        if !matches!(tool, Tool::Move | Tool::Hand) {
            self.exit_vector_edit(cx);
        }
        self.toolbar.hand = tool == Tool::Hand;
        match tool {
            Tool::Move | Tool::Hand => {
                self.finish_bezier(cx);
                self.draw_tool = None;
                self.seal_text_edits(cx);
                for text in &self.texts {
                    text.editor.update(cx, |editor, _| editor.editing = false);
                }
                self.focus.focus(window, cx);
            }
            _ => {
                let drawing = match tool {
                    Tool::Board => DrawTool::Board,
                    Tool::Text => DrawTool::Text,
                    Tool::Rectangle => {
                        self.toolbar.shape = tool;
                        DrawTool::Shape(ShapeKind::Rectangle)
                    }
                    Tool::Ellipse => {
                        self.toolbar.shape = tool;
                        DrawTool::Shape(ShapeKind::Ellipse)
                    }
                    Tool::Line => {
                        self.toolbar.shape = tool;
                        DrawTool::Shape(ShapeKind::Line)
                    }
                    Tool::Arrow | Tool::Polygon | Tool::Star => {
                        self.toolbar.shape = tool;
                        DrawTool::Shape(match tool {
                            Tool::Arrow => ShapeKind::Arrow,
                            Tool::Polygon => ShapeKind::Polygon,
                            _ => ShapeKind::Star,
                        })
                    }
                    Tool::Bezier => {
                        self.toolbar.pen = tool;
                        DrawTool::Shape(ShapeKind::Bezier)
                    }
                    Tool::Pencil => {
                        self.toolbar.pen = tool;
                        DrawTool::Shape(ShapeKind::Pen)
                    }
                    _ => unreachable!(),
                };
                self.activate_drawing(drawing, window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn toolbar_shortcut(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = event.keystroke.modifiers;
        if (modifiers.control || modifiers.platform)
            && modifiers.shift
            && event.keystroke.key.eq_ignore_ascii_case("k")
            && self.gesture.is_none()
        {
            if !event.is_held {
                self.choose_tool(Tool::Media, window, cx);
            }
            return true;
        }
        if modifiers.control || modifiers.platform || modifiers.alt || self.gesture.is_some() {
            return false;
        }
        let tool = match (event.keystroke.key.as_str(), modifiers.shift) {
            ("v", false) => Tool::Move,
            ("h", false) => Tool::Hand,
            ("f", false) => Tool::Board,
            ("r", false) => Tool::Rectangle,
            ("o", false) => Tool::Ellipse,
            ("l", false) => Tool::Line,
            ("l" | "L", true) => Tool::Arrow,
            ("p", false) => Tool::Bezier,
            ("p" | "P", true) => Tool::Pencil,
            ("t", false) => Tool::Text,
            _ => return false,
        };
        if !event.is_held {
            self.choose_tool(tool, window, cx);
        }
        true
    }

    fn active_tool(&self, cx: &Context<Self>) -> Tool {
        if self.toolbar.hand {
            return Tool::Hand;
        }
        match self.draw_tool {
            Some(DrawTool::Board) => Tool::Board,
            Some(DrawTool::Text) => Tool::Text,
            Some(DrawTool::Shape(ShapeKind::Rectangle)) => Tool::Rectangle,
            Some(DrawTool::Shape(ShapeKind::Ellipse)) => Tool::Ellipse,
            Some(DrawTool::Shape(ShapeKind::Arrow)) => Tool::Arrow,
            Some(DrawTool::Shape(ShapeKind::Polygon)) => Tool::Polygon,
            Some(DrawTool::Shape(ShapeKind::Star)) => Tool::Star,
            Some(DrawTool::Shape(ShapeKind::Line)) => Tool::Line,
            Some(DrawTool::Shape(ShapeKind::Pen)) => Tool::Pencil,
            Some(DrawTool::Shape(ShapeKind::Bezier)) => Tool::Bezier,
            _ if self
                .selected_text()
                .is_some_and(|text| text.editor.read(cx).editing) =>
            {
                Tool::Text
            }
            _ => Tool::Move,
        }
    }

    fn tool_button(&self, tool: Tool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (id, label, key, glyph) = tool.details();
        let active = self.active_tool(cx) == tool;
        div()
            .id(id)
            .debug_selector(move || id.into())
            .size(px(40.))
            .flex_shrink_0()
            .rounded(px(8.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x3b3e47)))
            .when(active, |el| el.bg(rgb(0x7960be)))
            .tooltip(move |_, cx| cx.new(|_| ToolTip(format!("{label}  {key}"))).into())
            .child(icon(glyph, 23.).text_color(rgb(if active { 0xffffff } else { TEXT })))
            .on_click(cx.listener(move |this, _, window, cx| this.choose_tool(tool, window, cx)))
    }

    fn tool_group(
        &self,
        index: usize,
        selected: Tool,
        tools: &'static [Tool],
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let menu = self.toolbar.menus[index].clone();
        let open = menu.read(cx).is_open();
        let id = ["navigation-menu", "shapes-menu", "pen-menu"][index];
        let rows: Vec<_> = tools
            .iter()
            .copied()
            .map(|tool| {
                let (id, label, key, glyph) = tool.details();
                div()
                    .id(("tool-option", tool as usize))
                    .debug_selector(move || format!("option-{id}"))
                    .h(px(36.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0x3b3e47)))
                    .child(div().w(px(16.)).child(
                        icon(LucideIcons::Check, 15.).when(tool != selected, |el| el.opacity(0.)),
                    ))
                    .child(icon(glyph, 19.))
                    .child(div().flex_1().child(label))
                    .child(div().text_color(rgb(MUTED)).child(key))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.choose_tool(tool, window, cx)),
                    )
            })
            .collect();
        div()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(self.tool_button(selected, cx))
            .child(
                dropdown(&menu)
                    .placement(if index == 0 {
                        DropdownPlacement::TopStart
                    } else {
                        DropdownPlacement::TopEnd
                    })
                    .menu_gap(px(14.))
                    .w(px(if index == 1 { 304. } else { 216. }))
                    .min_w(px(if index == 1 { 304. } else { 216. }))
                    .p(px(6.))
                    .rounded(px(12.))
                    .shadow_lg()
                    .bg(rgb(0x252830))
                    .border_color(rgb(0x414550))
                    .text_color(rgb(TEXT))
                    .text_size(px(13.))
                    .trigger(
                        div()
                            .id(id)
                            .debug_selector(move || id.into())
                            .w(px(18.))
                            .h(px(40.))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(open, |el| el.bg(rgb(0x3b3e47)))
                            .hover(|s| s.bg(rgb(0x3b3e47)))
                            .child(icon(LucideIcons::ChevronDown, 12.).text_color(rgb(MUTED))),
                    )
                    .menu(
                        div()
                            .debug_selector(move || format!("{id}-popup"))
                            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .flex()
                            .flex_col()
                            .children(rows),
                    ),
            )
    }

    pub(super) fn tool_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let hint = match self.draw_tool {
            Some(DrawTool::Board) => Some(t("draw-board-hint")),
            Some(DrawTool::Text) => Some(t("draw-text-hint")),
            Some(DrawTool::Shape(
                ShapeKind::Rectangle | ShapeKind::Ellipse | ShapeKind::Polygon | ShapeKind::Star,
            )) => Some(t("draw-shape-hint")),
            Some(DrawTool::Shape(ShapeKind::Line | ShapeKind::Arrow)) => Some(t("draw-line-hint")),
            Some(DrawTool::Shape(ShapeKind::Bezier)) => Some(t("draw-pen-hint")),
            Some(DrawTool::Shape(ShapeKind::Pen)) => Some(t("draw-pencil-hint")),
            _ if self.toolbar.hand => Some(t("pan-hint")),
            _ => None,
        };
        div()
            .absolute()
            .bottom(px(panels::PANEL_BOTTOM))
            .left_0()
            .right_0()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .when_some(hint, |el, hint| {
                el.child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(hint))
            })
            .child(
                super::layers::glass_surface()
                    .id("tool-bar")
                    .debug_selector(|| "tool-bar".into())
                    .occlude()
                    .p(px(7.))
                    .rounded(px(14.))
                    .border_1()
                    .border_color(rgb(0x444852))
                    .shadow_lg()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(self.tool_group(
                        0,
                        if self.toolbar.hand {
                            Tool::Hand
                        } else {
                            Tool::Move
                        },
                        &[Tool::Move, Tool::Hand],
                        cx,
                    ))
                    .child(div().w(px(1.)).h(px(24.)).mx(px(2.)).bg(rgb(0x464952)))
                    .child(self.tool_button(Tool::Board, cx))
                    .child(self.tool_group(
                        1,
                        self.toolbar.shape,
                        &[
                            Tool::Rectangle,
                            Tool::Line,
                            Tool::Arrow,
                            Tool::Ellipse,
                            Tool::Polygon,
                            Tool::Star,
                            Tool::Media,
                        ],
                        cx,
                    ))
                    .child(self.tool_group(2, self.toolbar.pen, &[Tool::Bezier, Tool::Pencil], cx))
                    .child(self.tool_button(Tool::Text, cx)),
            )
    }
}

#[cfg(test)]
mod tests;
