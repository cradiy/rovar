use super::*;
use crate::ui::theme::Color;
use uic::components::{
    context_menu::{ContextMenu, ContextMenuItem, ContextMenuTrigger},
    input::{Input, InputAppearance},
};

fn icon_action(id: &'static str, glyph: LucideIcons) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .size(px(24.))
        .flex_shrink_0()
        .rounded(px(5.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(MUTED.color())
        .hover(|s| s.bg(BORDER.color()).text_color(TEXT.color()))
        .child(icon(glyph, 14.))
}

fn text_action(id: &'static str, label: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(28.))
        .px(px(6.))
        .rounded(px(5.))
        .flex()
        .items_center()
        .gap(px(6.))
        .cursor_pointer()
        .text_size(px(12.))
        .text_color(ACCENT.color())
        .hover(|s| s.bg(Color::Selected.color()))
        .child(icon(LucideIcons::Plus, 14.))
        .child(label)
}

fn rename_input(rename: &Rename) -> Input {
    Input::new(&rename.input)
        .w_full()
        .h(px(28.))
        .px(px(8.))
        .py_0()
        .rounded(px(5.))
        .text_size(px(12.))
        .text_color(TEXT.color())
        .bg(Color::Input.color())
        .border_color(if rename.error {
            Color::Danger.color()
        } else {
            BORDER.color()
        })
        .appearance(InputAppearance {
            focus_border: if rename.error {
                Color::Danger.color()
            } else {
                ACCENT.color()
            }
            .into(),
            ..crate::ui::theme::input_appearance()
        })
}

impl Workspace {
    pub(in crate::editor) fn variant_controls(&self, cx: &mut Context<Self>) -> Div {
        if self.selected_masters().is_some() {
            return crate::editor::inspector::inspector_section(t("component-variants")).child(
                div().flex().child(
                    text_action("combine-variants", t("variant-combine")).on_click(
                        cx.listener(|this, _, window, cx| this.combine_variants(window, cx)),
                    ),
                ),
            );
        }
        let Some((root, binding)) = self.selected_component() else {
            return div();
        };
        let set = self.variant_set(&binding.component).map(|(_, set)| set);
        let editable = binding.master && self.layer_editable(root);
        if set.is_none() && !binding.master {
            return div();
        }
        let rename = self
            .components
            .rename
            .as_ref()
            .filter(|r| r.page == self.pages.active && r.component == binding.component);
        let mut section = div()
            .flex_shrink_0()
            .px(px(14.))
            .py(px(12.))
            .border_b_1()
            .border_color(BORDER.color())
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t("component-variants")),
                    )
                    .when(set.is_some() && editable, |el| {
                        el.child(icon_action("add-variant", LucideIcons::Plus).on_click(
                            cx.listener(|this, _, window, cx| this.add_variant(window, cx)),
                        ))
                    }),
            );
        if let Some(set) = set {
            let title = if let Some(rename) = rename.filter(|r| r.set_name) {
                div().child(rename_input(rename))
            } else {
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(icon(LucideIcons::Component, 14.).text_color(ACCENT.color()))
                    .child(
                        div()
                            .id("variant-set-name")
                            .debug_selector(|| "variant-set-name".into())
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.))
                            .child(set.name.clone())
                            .when(editable, |el| {
                                el.on_click(cx.listener(
                                    |this, event: &gpui::ClickEvent, window, cx| {
                                        if event.click_count() == 2 {
                                            this.begin_variant_rename(true, window, cx);
                                        }
                                    },
                                ))
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(MUTED.color())
                            .child(set.variants.len().to_string()),
                    )
            };
            section = section.child(title);
            let mut row = div().flex().items_center().gap(px(4.));
            if let Some(rename) = rename.filter(|r| !r.set_name) {
                row = row.child(div().flex_1().min_w_0().child(rename_input(rename)));
            } else {
                let value = div()
                    .id("variant-selector")
                    .debug_selector(|| "variant-selector".into())
                    .h(px(28.))
                    .px(px(8.))
                    .rounded(px(5.))
                    .bg(Color::Input.color())
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(set.variants[&binding.component].clone()),
                    );
                let value = if self.layer_editable(root) {
                    let weak = cx.entity().downgrade();
                    let page = self.pages.active.clone();
                    ContextMenuTrigger::new(
                        value
                            .cursor_pointer()
                            .hover(|s| s.bg(BORDER.color()))
                            .child(icon(LucideIcons::ChevronDown, 12.).text_color(MUTED.color())),
                        move |_, cx| {
                            weak.update(cx, |this, cx| this.variant_menu(root, &page, cx))
                                .unwrap_or_default()
                        },
                    )
                    .id("variant-selector-trigger")
                    .into_any_element()
                } else {
                    value.into_any_element()
                };
                row = row
                    .child(div().flex_1().min_w_0().child(value))
                    .when(editable, |el| {
                        el.child(icon_action("rename-variant", LucideIcons::Pencil).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.begin_variant_rename(false, window, cx)
                            }),
                        ))
                    });
            }
            section = section.child(row);
        } else if editable {
            section = section.child(
                div().flex().child(
                    text_action("add-variant", t("variant-create"))
                        .on_click(cx.listener(|this, _, window, cx| this.add_variant(window, cx))),
                ),
            );
        }
        section
    }

    fn variant_menu(&self, root: usize, page: &str, cx: &mut Context<Self>) -> ContextMenu {
        let mut menu = crate::editor::context_menu::menu(210., "variant-menu");
        if self.pages.active != page {
            return menu;
        }
        let Some(link) = self.hierarchy.components.get(&root) else {
            return menu;
        };
        let Some((_, set)) = self.variant_set(&link.component) else {
            return menu;
        };
        let mut variants: Vec<_> = set.variants.iter().collect();
        variants.sort_by_key(|(_, name)| *name);
        for (target, name) in variants {
            let checked = &link.component == target;
            let target = target.clone();
            let name = name.clone();
            let weak = cx.entity().downgrade();
            let page = page.to_owned();
            menu = menu.item(ContextMenuItem::action_with(
                move |_, _| {
                    div()
                        .debug_selector({
                            let name = name.clone();
                            move || format!("variant-option-{name}")
                        })
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .w(px(14.))
                                .when(checked, |el| el.child(icon(LucideIcons::Check, 13.))),
                        )
                        .child(name.clone())
                },
                move |window, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.switch_variant(root, &page, &target, window, cx)
                    });
                },
            ));
        }
        menu
    }
}
