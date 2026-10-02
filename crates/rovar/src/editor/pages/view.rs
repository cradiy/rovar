use super::*;
use gpui::{AnyElement, rgba};
use uic::components::{
    context_menu::{self, ContextMenuItem},
    input::{Input, InputAppearance},
};

#[derive(Clone)]
struct PageDrag {
    workspace: gpui::EntityId,
    id: String,
    name: String,
}
impl Render for PageDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(12.))
            .py(px(8.))
            .rounded(px(8.))
            .bg(rgb(0x302b40))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .shadow_lg()
            .child(self.name.clone())
    }
}

impl Workspace {
    pub(in crate::editor) fn begin_page_rename(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_page_rename(true, window, cx);
        self.suspend(window, cx);
        let Some(page) = self.pages.entries.iter().find(|p| p.page.id == id) else {
            return;
        };
        let name = page.page.name.clone();
        self.pages.renaming = Some(id.into());
        self.pages.expanded = true;
        self.pages
            .input
            .update(cx, |input, cx| input.set_value(name, cx));
        self.pages.input.focus_handle(cx).focus(window, cx);
        let input = self.pages.input.clone();
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
    pub(in crate::editor) fn finish_page_rename(
        &mut self,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.pages.renaming.take() else {
            return;
        };
        let name = self.pages.input.read(cx).value().trim().to_owned();
        if commit
            && !name.is_empty()
            && name.chars().count() <= 200
            && self
                .pages
                .entries
                .iter()
                .any(|p| p.page.id == id && p.page.name != name)
        {
            let before = self.page_edit(std::slice::from_ref(&id), cx);
            self.pages
                .entries
                .iter_mut()
                .find(|p| p.page.id == id)
                .unwrap()
                .page
                .name = name;
            self.record_page_edit(before);
        }
        self.focus_canvas(window, cx);
        cx.notify();
    }
    fn page_menu(
        &mut self,
        id: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_page_rename(true, window, cx);
        self.seal_text_edits(cx);
        self.focus_canvas(window, cx);
        let mut menu = crate::editor::context_menu::menu(210., "page-context-glass");
        for (key, label, glyph) in [
            ("page-rename", "rename", LucideIcons::Pencil),
            ("page-duplicate", "page-duplicate", LucideIcons::Copy),
            ("page-delete", "page-delete", LucideIcons::Trash2),
        ] {
            let id = id.clone();
            let weak = cx.entity().downgrade();
            let disabled = key == "page-delete" && self.pages.entries.len() == 1;
            let item = ContextMenuItem::action_with(
                move |_, _| {
                    div()
                        .debug_selector(move || key.into())
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .opacity(if disabled { 0.4 } else { 1. })
                        .child(icon(glyph, 15.))
                        .child(div().flex_1().child(t(label)))
                },
                move |window, cx| {
                    let _ = weak.update(cx, |this, cx| match key {
                        "page-rename" => this.begin_page_rename(&id, window, cx),
                        "page-duplicate" => this.add_page(Some(&id), window, cx),
                        _ => {
                            this.suspend(window, cx);
                            this.pages.delete = Some(id.clone());
                            this.focus_canvas(window, cx);
                            cx.notify();
                        }
                    });
                },
            )
            .disabled(disabled);
            menu = menu.item(if key == "page-delete" {
                item.danger()
            } else {
                item
            });
        }
        let _ = context_menu::show(menu, position, window, cx);
    }
    pub(in crate::editor) fn pages_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        self.pages.row_bounds.borrow_mut().clear();
        let mut panel = div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(rgba(0xb4a2ee20))
            .child(
                div()
                    .h(px(36.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .id("toggle-pages")
                            .debug_selector(|| "toggle-pages".into())
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .cursor_pointer()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(icon(
                                if self.pages.expanded {
                                    LucideIcons::ChevronDown
                                } else {
                                    LucideIcons::ChevronRight
                                },
                                14.,
                            ))
                            .child(div().min_w_0().truncate().child(if self.pages.expanded {
                                t("pages").to_string()
                            } else {
                                self.pages.current().page.name.clone()
                            }))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.finish_page_rename(true, window, cx);
                                this.pages.expanded = !this.pages.expanded;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("add-page")
                            .debug_selector(|| "add-page".into())
                            .size(px(26.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_color(rgb(MUTED))
                            .hover(|s| s.bg(rgba(0xb4a2ee22)).text_color(rgb(ACCENT)))
                            .tooltip(|_, cx| {
                                cx.new(|_| toolbar::ToolTip(t("page-new").into())).into()
                            })
                            .child(icon(LucideIcons::Plus, 16.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_page(None, window, cx);
                                cx.stop_propagation();
                            })),
                    ),
            );
        if self.pages.expanded {
            let rows = self
                .pages
                .entries
                .iter()
                .map(|state| {
                    let id = state.page.id.clone();
                    let click_id = id.clone();
                    let menu_id = id.clone();
                    let drop_id = id.clone();
                    let bounds_id = id.clone();
                    let drag_bounds_id = id.clone();
                    let bounds = self.pages.row_bounds.clone();
                    let drag_bounds = bounds.clone();
                    let owner = cx.entity_id();
                    let active = self.pages.active == id;
                    let editing = self.pages.renaming.as_ref() == Some(&id);
                    let drag = PageDrag {
                        workspace: owner,
                        id: id.clone(),
                        name: state.page.name.clone(),
                    };
                    div()
                        .on_paint_before_children(move |rect, _, _, _| {
                            bounds.borrow_mut().insert(bounds_id.clone(), rect);
                        })
                        .id(gpui::SharedString::from(format!("page-{id}")))
                        .debug_selector(move || format!("page-{id}"))
                        .h(px(28.))
                        .px(px(8.))
                        .rounded(px(6.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(12.))
                        .text_color(rgb(if active { ACCENT } else { TEXT }))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgba(0xb4a2ee14)))
                        .when(active, |el| el.bg(rgba(0xb4a2ee18)))
                        .child(
                            div()
                                .w(px(3.))
                                .h(px(12.))
                                .rounded(px(2.))
                                .flex_shrink_0()
                                .when(active, |el| el.bg(rgb(ACCENT))),
                        )
                        .when_else(
                            editing,
                            |el| {
                                el.child(
                                    Input::new(&self.pages.input)
                                        .flex_1()
                                        .min_w_0()
                                        .h(px(26.))
                                        .px(px(3.))
                                        .py_0()
                                        .rounded(px(4.))
                                        .text_size(px(12.))
                                        .text_color(rgb(TEXT))
                                        .bg(rgb(0x2c3038))
                                        .border_color(rgb(0x2c3038))
                                        .appearance(InputAppearance {
                                            focus_border: rgb(ACCENT).into(),
                                            caret: rgb(ACCENT).into(),
                                            selection: rgba(0xb4a2ee44).into(),
                                            caret_height: px(16.),
                                            ..Default::default()
                                        }),
                                )
                            },
                            |el| {
                                el.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(state.page.name.clone()),
                                )
                            },
                        )
                        .on_click(
                            cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                if this.pages.renaming.as_ref() != Some(&click_id) {
                                    this.switch_page(&click_id, window, cx);
                                    if event.click_count() == 2 {
                                        this.begin_page_rename(&click_id, window, cx);
                                    }
                                }
                                cx.stop_propagation();
                            }),
                        )
                        .when(!editing, |el| {
                            el.on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                        })
                        .drag_over::<PageDrag>(move |style, drag, window, _| {
                            if drag.workspace != owner {
                                return style;
                            }
                            let after = drag_bounds
                                .borrow()
                                .get(&drag_bounds_id)
                                .is_some_and(|b| window.mouse_position().y > b.center().y);
                            if after {
                                style.border_b_2().border_color(rgb(ACCENT))
                            } else {
                                style.border_t_2().border_color(rgb(ACCENT))
                            }
                        })
                        .on_drop(cx.listener(move |this, drag: &PageDrag, window, cx| {
                            if drag.workspace != cx.entity_id() {
                                return;
                            }
                            let Some(index) =
                                this.pages.entries.iter().position(|p| p.page.id == drop_id)
                            else {
                                return;
                            };
                            let after = this
                                .pages
                                .row_bounds
                                .borrow()
                                .get(&drop_id)
                                .is_some_and(|b| window.mouse_position().y > b.center().y);
                            let before = this
                                .pages
                                .entries
                                .get(index + usize::from(after))
                                .map(|p| p.page.id.clone());
                            this.reorder_page(&drag.id, before.as_deref(), cx);
                            cx.stop_propagation();
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                this.page_menu(menu_id.clone(), event.position, window, cx);
                                cx.stop_propagation();
                            }),
                        )
                })
                .collect::<Vec<_>>();
            panel = panel.child(
                div()
                    .id("page-list")
                    .max_h(px(160.))
                    .overflow_y_scroll()
                    .px(px(8.))
                    .pb(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(rows),
            );
        }
        panel.into_any_element()
    }
    pub(in crate::editor) fn page_delete_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = self.pages.delete.clone().unwrap();
        let name = self
            .pages
            .entries
            .iter()
            .find(|p| p.page.id == id)
            .map(|p| p.page.name.clone())
            .unwrap_or_default();
        let body = div()
            .id("page-delete-dialog")
            .debug_selector(|| "page-delete-dialog".into())
            .w(px(360.))
            .max_w_full()
            .p(px(20.))
            .rounded(px(14.))
            .bg(rgb(0x202027))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow_xl()
            .flex()
            .flex_col()
            .gap(px(16.))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(div().text_size(px(15.)).child(t("page-delete")))
            .child(div().text_size(px(13.)).truncate().child(name))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(t("page-delete-hint")),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .children([false, true].map(|confirm| {
                        let id = id.clone();
                        let key = if confirm {
                            "confirm-page-delete"
                        } else {
                            "cancel-page-delete"
                        };
                        div()
                            .id(key)
                            .debug_selector(move || key.into())
                            .h(px(32.))
                            .px(px(14.))
                            .rounded(px(7.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.))
                            .cursor_pointer()
                            .bg(rgb(if confirm { 0x823f49 } else { 0x2b2c35 }))
                            .child(t(if confirm { "delete" } else { "cancel" }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if confirm {
                                    this.delete_page(&id, window, cx);
                                } else {
                                    this.pages.delete = None;
                                    cx.notify();
                                }
                            }))
                    })),
            );
        gpui::deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(rgba(0x00000066))
                .flex()
                .items_center()
                .justify_center()
                .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                .child(body),
        )
        .with_priority(30)
    }
}
