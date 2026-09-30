use super::*;
use gpui::AnyElement;
use uic::components::{
    context_menu::{self, ContextMenuItem},
    input::Input,
};

impl Workspace {
    pub(in crate::workspace) fn assets_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div().px(px(12.)).pt(px(10.)).flex().gap(px(4.)).children(
                    [
                        (Scope::Document, "assets-document"),
                        (Scope::Local, "assets-local"),
                    ]
                    .map(|(scope, label)| {
                        let active = self.assets.scope == scope;
                        div()
                            .id(label)
                            .debug_selector(move || label.into())
                            .flex_1()
                            .min_w_0()
                            .h(px(28.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(11.))
                            .cursor_pointer()
                            .bg(if active {
                                rgb(0x302c40).into()
                            } else {
                                gpui::transparent_black()
                            })
                            .text_color(rgb(if active { ACCENT } else { MUTED }))
                            .child(t(label))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.assets.scope = scope;
                                cx.notify();
                            }))
                    }),
                ),
            )
            .child(if self.assets.scope == Scope::Local {
                self.local_assets_panel(cx).into_any_element()
            } else {
                self.document_assets_panel(cx).into_any_element()
            })
            .into_any_element()
    }
    fn document_assets_panel(&self, cx: &mut Context<Self>) -> Div {
        let query = self.assets.search.read(cx).value().trim().to_lowercase();
        let entries: Vec<_> = self
            .components
            .definitions
            .iter()
            .filter(|(_, c)| c.name.to_lowercase().contains(&query))
            .map(|(id, c)| (id.clone(), c.name.clone()))
            .collect();
        let enabled = self.can_create_component();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(12.))
            .child(
                Input::new(&self.assets.search)
                    .w_full()
                    .h(px(32.))
                    .text_size(px(12.))
                    .bg(rgb(0x24262f))
                    .border_color(gpui::rgba(0xffffff10)),
            )
            .child(self.color_assets(cx))
            .child(
                div()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(t("assets-components")),
                    )
                    .when(enabled, |row| {
                        row.child(
                            div()
                                .id("create-document-component")
                                .debug_selector(|| "create-document-component".into())
                                .h(px(28.))
                                .px(px(8.))
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .text_size(px(11.))
                                .text_color(rgb(ACCENT))
                                .child(icon(LucideIcons::Plus, 13.))
                                .child(t("component-create-selection"))
                                .cursor_pointer()
                                .hover(|s| s.bg(gpui::rgba(0xb4a2ee18)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.create_component(window, cx)
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .id("document-components-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .when(entries.is_empty(), |el| {
                        el.child(
                            div()
                                .py(px(32.))
                                .text_center()
                                .text_size(px(12.))
                                .text_color(rgb(MUTED))
                                .child(t(if query.is_empty() {
                                    "components-document-empty"
                                } else {
                                    "assets-no-results"
                                })),
                        )
                    })
                    .children(entries.into_iter().map(|(id, name)| {
                        let insert = id.clone();
                        let edit = id.clone();
                        let drag = super::super::components::ComponentDrag {
                            owner: cx.entity_id(),
                            id: id.clone(),
                            name: name.clone(),
                            preview: self
                                .components
                                .previews
                                .get(&id)
                                .and_then(|(_, p)| p.clone()),
                        };
                        div()
                            .id(gpui::SharedString::from(format!("document-component-{id}")))
                            .debug_selector({
                                let id = id.clone();
                                move || format!("document-component-{id}")
                            })
                            .w_full()
                            .min_w_0()
                            .rounded(px(8.))
                            .p(px(10.))
                            .bg(rgb(0x22232c))
                            .border_1()
                            .border_color(gpui::rgba(0xffffff0b))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .cursor_pointer()
                            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                            .hover(|s| s.border_color(gpui::rgba(0xb4a2ee77)))
                            .child(
                                div()
                                    .w(px(64.))
                                    .h(px(48.))
                                    .flex_shrink_0()
                                    .rounded(px(5.))
                                    .overflow_hidden()
                                    .bg(rgb(0x15161c))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .map(|el| {
                                        if let Some(preview) = self
                                            .components
                                            .previews
                                            .get(&id)
                                            .and_then(|(_, p)| p.clone())
                                        {
                                            el.child(
                                                gpui::img(preview)
                                                    .size_full()
                                                    .object_fit(gpui::ObjectFit::Contain),
                                            )
                                        } else {
                                            el.child(
                                                icon(LucideIcons::Component, 22.)
                                                    .text_color(rgb(ACCENT)),
                                            )
                                        }
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.))
                                    .child(name),
                            )
                            .child(
                                div()
                                    .id(gpui::SharedString::from(format!("edit-component-{id}")))
                                    .size(px(24.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(MUTED))
                                    .child(icon(LucideIcons::Pencil, 14.))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.edit_document_component(&edit, window, cx);
                                        cx.stop_propagation();
                                    })),
                            )
                            .on_click(cx.listener(
                                move |this, event: &gpui::ClickEvent, window, cx| {
                                    if event.click_count() == 2 {
                                        this.insert_document_component(
                                            &insert, false, None, window, cx,
                                        );
                                    }
                                    cx.stop_propagation();
                                },
                            ))
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(
                                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                                        this.document_component_menu(
                                            id.clone(),
                                            event.position,
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    },
                                ),
                            )
                    })),
            )
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
    }
    fn document_component_menu(
        &mut self,
        id: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut menu = super::super::context_menu::menu(220., "document-component-menu");
        for (action, label) in [
            (0, "assets-insert"),
            (1, "component-edit-main"),
            (2, "component-save-local"),
            (3, "rename"),
            (4, "delete"),
        ] {
            let weak = cx.entity().downgrade();
            let id = id.clone();
            menu = menu.item(ContextMenuItem::action(t(label), move |window, cx| {
                let _ = weak.update(cx, |this, cx| match action {
                    0 => this.insert_document_component(&id, false, None, window, cx),
                    1 => this.edit_document_component(&id, window, cx),
                    2 => this.save_document_component_local(&id, window, cx),
                    3 => this.rename_document_component(&id, window, cx),
                    _ => this.delete_document_component(&id, window, cx),
                });
            }));
        }
        let _ = context_menu::show(menu, position, window, cx);
    }
}
