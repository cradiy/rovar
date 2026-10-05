use super::*;
use crate::i18n::t;
use crate::ui::theme::Color;
use layout::LayoutAction;
use shapes::NodeAction;
use std::collections::BTreeSet;
use uic::components::context_menu::{self, ContextMenu, ContextMenuAppearance, ContextMenuItem};

pub(super) fn menu(width: f32, selector: &'static str) -> ContextMenu {
    ContextMenu::new()
        .w(px(width))
        .max_h(px(500.))
        .bg(gpui::transparent_black())
        .border_0()
        .text_color(TEXT.color())
        .text_size(px(12.))
        .appearance(ContextMenuAppearance {
            muted_foreground: MUTED.color().into(),
            danger_foreground: Color::Danger.color().into(),
            selected_background: Color::Accent.color().opacity(0.1569).into(),
            selected_foreground: TEXT.color().into(),
            separator: Color::Accent.color().opacity(0.1569).into(),
            item_height: px(28.),
            ..Default::default()
        })
        .surface(move |state, content, _, _| {
            layers::glass_surface()
                .debug_selector(move || format!("{selector}-{}", state.depth))
                .rounded(px(12.))
                .border_1()
                .border_color(Color::Accent.color().opacity(0.2706))
                .shadow_lg()
                .child(content)
        })
}

#[derive(Clone, Copy)]
enum Command {
    SaveAsset,
    Export(crate::document::export::Format),
    Undo,
    Redo,
    Copy,
    Cut,
    PasteInPlace,
    Flip(bool),
    Paste,
    Duplicate,
    Delete,
    Rename,
    Group,
    Boolean(crate::scene::boolean::Operation),
    AutoLayout,
    CreateComponent,
    EditComponent,
    DetachComponent,
    ResetComponent,
    Ungroup,
    Raise,
    Lower,
    Front,
    Back,
    Lock(bool),
    Hide(bool),
    Layout(LayoutAction),
    Node(NodeAction),
}

impl Workspace {
    pub(super) fn open_context_menu(
        &mut self,
        target: Option<usize>,
        tree: bool,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some() || self.bezier_draft.is_some() {
            return;
        }
        self.pick_hover = None;
        self.close_tool_menus(window, cx);
        self.finish_rename(true, window, cx);
        self.seal_text_edits(cx);
        self.focus.focus(window, cx);
        let target = target.map(|id| {
            if tree || self.is_selected(id) {
                id
            } else {
                self.group_target(id)
            }
        });
        if let Some(id) = target {
            if !self.is_selected(id) {
                self.set_selection(BTreeSet::from([id]), cx);
            }
        } else {
            self.set_selection(BTreeSet::new(), cx);
        }
        let ids: Vec<_> = target
            .filter(|id| !self.layer_editable(*id))
            .map_or_else(|| self.selection_ids().into_iter().collect(), |id| vec![id]);
        let editable = !ids.is_empty() && ids.iter().all(|id| self.layer_editable(*id));
        let weak = cx.entity().downgrade();
        let item = |key: &'static str,
                    label: &'static str,
                    glyph,
                    shortcut: &'static str,
                    command,
                    enabled| {
            let weak = weak.clone();
            let ids = ids.clone();
            ContextMenuItem::action_with(
                move |_, _| {
                    div()
                        .debug_selector(move || key.into())
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .opacity(if enabled { 1. } else { 0.4 })
                        .child(icon(glyph, 15.))
                        .child(label)
                },
                move |window, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.run_menu_command(command, &ids, window, cx)
                    });
                },
            )
            .shortcut(crate::ui::shortcuts::label(shortcut))
            .disabled(!enabled)
        };
        let order_enabled = editable && self.common_parent(&self.selection_ids()).is_some();
        let paste_enabled = self.can_paste_objects(cx);
        let mut menu = menu(238., "editor-context-glass");
        if !tree {
            menu = self.with_layer_picker(menu, position, cx);
        }
        if self.can_boolean() {
            menu = menu.submenu_with(
                |_, _| div().child(t("boolean-operations")),
                |mut menu| {
                    for operation in crate::scene::boolean::Operation::ALL {
                        menu = menu.item(item(
                            operation.key(),
                            operation.label(),
                            LucideIcons::Layers,
                            "",
                            Command::Boolean(operation),
                            true,
                        ));
                    }
                    menu
                },
            );
        }
        menu = menu
            .item(item(
                "context-undo",
                t("undo"),
                LucideIcons::Undo2,
                "Mod+Z",
                Command::Undo,
                self.history.borrow().can_undo(),
            ))
            .item(item(
                "context-redo",
                t("redo"),
                LucideIcons::Redo2,
                "Mod+Shift+Z",
                Command::Redo,
                self.history.borrow().can_redo(),
            ))
            .separator()
            .item(item(
                "context-cut",
                t("cut"),
                LucideIcons::Scissors,
                "Mod+X",
                Command::Cut,
                editable,
            ))
            .item(item(
                "context-copy",
                t("copy"),
                LucideIcons::Copy,
                "Mod+C",
                Command::Copy,
                editable,
            ))
            .item(item(
                "context-paste",
                t("paste"),
                LucideIcons::Clipboard,
                "Mod+V",
                Command::Paste,
                paste_enabled,
            ))
            .item(item(
                "context-paste-in-place",
                t("paste-in-place"),
                LucideIcons::ClipboardPaste,
                "Mod+Shift+V",
                Command::PasteInPlace,
                paste_enabled,
            ))
            .item(item(
                "context-duplicate",
                t("duplicate"),
                LucideIcons::CopyPlus,
                "Mod+D",
                Command::Duplicate,
                editable,
            ))
            .item(item(
                "context-rename",
                t("rename"),
                LucideIcons::Pencil,
                "F2",
                Command::Rename,
                editable && ids.len() == 1,
            ))
            .item(item(
                "context-save-asset",
                t("component-save-local"),
                LucideIcons::Component,
                "",
                Command::SaveAsset,
                self.can_save_asset(cx),
            ))
            .separator();
        if self.can_create_component() {
            menu = menu.item(item(
                "context-create-component",
                t("component-create"),
                LucideIcons::Component,
                "",
                Command::CreateComponent,
                true,
            ));
        }
        if let Some(link) = ids
            .first()
            .filter(|_| ids.len() == 1)
            .and_then(|id| self.hierarchy.components.get(id))
        {
            if !link.master {
                menu = menu
                    .item(item(
                        "context-edit-component",
                        t("component-edit-main"),
                        LucideIcons::Pencil,
                        "",
                        Command::EditComponent,
                        true,
                    ))
                    .item(item(
                        "context-reset-component",
                        t("component-reset"),
                        LucideIcons::RotateCcw,
                        "",
                        Command::ResetComponent,
                        editable,
                    ));
            }
            menu = menu.item(item(
                "context-detach-component",
                t("component-detach"),
                LucideIcons::Unlink,
                "",
                Command::DetachComponent,
                editable,
            ));
        }
        if self.can_auto_layout() {
            menu = menu.item(item(
                "context-auto-layout",
                t(if self.has_auto_layout() {
                    "layout-disable"
                } else {
                    "layout-enable"
                }),
                LucideIcons::PanelsTopLeft,
                "Shift+A",
                Command::AutoLayout,
                editable && self.can_auto_layout(),
            ));
        }
        menu = menu
            .item(item(
                "context-group",
                t("group"),
                LucideIcons::Group,
                "Mod+G",
                Command::Group,
                self.can_group(),
            ))
            .item(item(
                "context-ungroup",
                t("ungroup"),
                LucideIcons::Ungroup,
                "Mod+Shift+G",
                Command::Ungroup,
                editable && ids.iter().any(|id| self.hierarchy.groups.contains_key(id)),
            ))
            .submenu_with(
                |_, _| {
                    div()
                        .debug_selector(|| "context-order".into())
                        .child(t("arrange"))
                },
                |menu| {
                    menu.item(item(
                        "context-front",
                        t("bring-front"),
                        LucideIcons::ChevronsUp,
                        "Mod+Shift+]",
                        Command::Front,
                        order_enabled,
                    ))
                    .item(item(
                        "context-raise",
                        t("bring-forward"),
                        LucideIcons::ArrowUp,
                        "Mod+]",
                        Command::Raise,
                        order_enabled,
                    ))
                    .item(item(
                        "context-lower",
                        t("send-backward"),
                        LucideIcons::ArrowDown,
                        "Mod+[",
                        Command::Lower,
                        order_enabled,
                    ))
                    .item(item(
                        "context-back",
                        t("send-back"),
                        LucideIcons::ChevronsDown,
                        "Mod+Shift+[",
                        Command::Back,
                        order_enabled,
                    ))
                },
            )
            .submenu_with(
                |_, _| {
                    div()
                        .debug_selector(|| "context-layout".into())
                        .child(t("align-distribute"))
                },
                |mut menu| {
                    for action in LayoutAction::ALL {
                        let (key, label, glyph) = action.details();
                        menu = menu.item(item(
                            key,
                            label,
                            glyph,
                            "",
                            Command::Layout(action),
                            self.can_layout(action),
                        ));
                    }
                    menu
                },
            );
        menu = menu.submenu(t("flip"), |menu| {
            menu.item(item(
                "context-flip-x",
                t("flip-horizontal"),
                LucideIcons::FlipHorizontal2,
                "Shift+H",
                Command::Flip(true),
                self.can_flip(),
            ))
            .item(item(
                "context-flip-y",
                t("flip-vertical"),
                LucideIcons::FlipVertical2,
                "Shift+V",
                Command::Flip(false),
                self.can_flip(),
            ))
        });
        if !ids.is_empty() && self.export_video_count() == self.selection_ids().len() {
            menu = menu.item(item(
                "context-export-video",
                t("export-video"),
                LucideIcons::Download,
                "",
                Command::Export(crate::document::export::Format::Png),
                !self.export.busy,
            ));
        } else {
            menu = menu.submenu(t("export-selection"), |menu| {
                use crate::document::export::Format;
                menu.item(item(
                    "context-export-png",
                    "PNG",
                    LucideIcons::Download,
                    "",
                    Command::Export(Format::Png),
                    !ids.is_empty() && !self.export.busy,
                ))
                .item(item(
                    "context-export-svg",
                    "SVG",
                    LucideIcons::Download,
                    "",
                    Command::Export(Format::Svg),
                    !ids.is_empty() && !self.export.busy,
                ))
            });
        }
        if self.vector_edit.is_some() {
            menu = menu.submenu_with(
                |_, _| {
                    div()
                        .debug_selector(|| "context-nodes".into())
                        .child(t("path-nodes"))
                },
                |mut menu| {
                    for (action, key, label, glyph) in [
                        (
                            NodeAction::Insert,
                            "context-node-insert",
                            t("insert-node"),
                            LucideIcons::Plus,
                        ),
                        (
                            NodeAction::Delete,
                            "context-node-delete",
                            t("delete-node"),
                            LucideIcons::Trash2,
                        ),
                        (
                            NodeAction::Corner,
                            "context-node-corner",
                            t("corner-node"),
                            LucideIcons::Diamond,
                        ),
                        (
                            NodeAction::Smooth,
                            "context-node-smooth",
                            t("smooth-node"),
                            LucideIcons::Circle,
                        ),
                        (
                            NodeAction::ToggleClosed,
                            "context-path-closed",
                            if self.selected_shape().unwrap().closed {
                                t("open-path")
                            } else {
                                t("close-path")
                            },
                            LucideIcons::Link,
                        ),
                    ] {
                        menu = menu.item(item(
                            key,
                            label,
                            glyph,
                            "",
                            Command::Node(action),
                            self.node_action_enabled(action),
                        ));
                    }
                    menu
                },
            );
        }
        if !ids.is_empty() {
            let locked = ids
                .iter()
                .all(|id| self.layer_info(*id).is_some_and(|(s, _)| s.locked));
            let hidden = ids
                .iter()
                .all(|id| self.layer_info(*id).is_some_and(|(s, _)| s.hidden));
            let inherited = |visibility| {
                ids.iter().all(|id| {
                    self.layer_info(*id).is_some_and(|(_, p)| {
                        let state = self.effective_layer(Default::default(), p);
                        if visibility {
                            !state.hidden
                        } else {
                            !state.locked
                        }
                    })
                })
            };
            menu = menu
                .separator()
                .item(item(
                    "context-lock",
                    if locked { t("unlock") } else { t("lock") },
                    if locked {
                        LucideIcons::LockOpen
                    } else {
                        LucideIcons::Lock
                    },
                    "",
                    Command::Lock(!locked),
                    inherited(false),
                ))
                .item(item(
                    "context-hide",
                    if hidden { t("show") } else { t("hide") },
                    if hidden {
                        LucideIcons::Eye
                    } else {
                        LucideIcons::EyeOff
                    },
                    "",
                    Command::Hide(!hidden),
                    inherited(true),
                ))
                .item(
                    item(
                        "context-delete",
                        t("delete"),
                        LucideIcons::Trash2,
                        "Delete",
                        Command::Delete,
                        editable,
                    )
                    .danger(),
                );
        }
        if editable && self.pages.entries.len() > 1 {
            let pages: Vec<_> = self
                .pages
                .entries
                .iter()
                .filter(|p| p.page.id != self.pages.active)
                .map(|p| (p.page.id.clone(), p.page.name.clone()))
                .collect();
            let weak = cx.entity().downgrade();
            menu = menu.submenu(t("page-move-selection"), move |mut menu| {
                for (id, name) in pages {
                    let weak = weak.clone();
                    menu = menu.item(ContextMenuItem::action(name, move |window, cx| {
                        let _ = weak
                            .update(cx, |this, cx| this.move_selection_to_page(&id, window, cx));
                    }));
                }
                menu
            });
        }
        let _ = context_menu::show(menu, position, window, cx);
        cx.notify();
    }
    fn run_menu_command(
        &mut self,
        command: Command,
        ids: &[usize],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window, cx);
        self.choose_tool(toolbar::Tool::Move, window, cx);
        match command {
            Command::SaveAsset => self.begin_save_asset(window, cx),
            Command::Export(format) => self.export_selection(Some(format), window, cx),
            Command::Node(action) => self.run_node_action(action, cx),
            Command::Undo => self.replay_history(false, window, cx),
            Command::Redo => self.replay_history(true, window, cx),
            Command::Copy => self.copy_selection(cx),
            Command::Cut => self.cut_selection(cx),
            Command::PasteInPlace => self.paste_in_place(window, cx),
            Command::Flip(horizontal) => self.flip_selection(horizontal, cx),
            Command::Paste => self.paste_selection(window, cx),
            Command::Duplicate => self.duplicate_selection(window, cx),
            Command::Delete => self.delete_selected(cx),
            Command::Rename => {
                if let Some(id) = ids.first() {
                    self.begin_rename(*id, window, cx);
                }
            }
            Command::Group => self.group_selection(cx),
            Command::Boolean(operation) => self.apply_boolean(operation, window, cx),
            Command::AutoLayout => self.enable_auto_layout(window, cx),
            Command::CreateComponent => self.create_component(window, cx),
            Command::EditComponent => {
                if let Some(id) = ids
                    .first()
                    .and_then(|id| self.hierarchy.components.get(id))
                    .map(|b| b.component.clone())
                {
                    self.edit_document_component(&id, window, cx);
                }
            }
            Command::DetachComponent => {
                if let Some(id) = ids.first() {
                    self.detach_component(*id, cx);
                }
            }
            Command::ResetComponent => {
                if let Some(id) = ids.first() {
                    self.reset_component(*id, window, cx);
                }
            }
            Command::Ungroup => self.ungroup_selection(cx),
            Command::Raise => self.shift_layers(true, false, cx),
            Command::Lower => self.shift_layers(false, false, cx),
            Command::Front => self.shift_layers(true, true, cx),
            Command::Back => self.shift_layers(false, true, cx),
            Command::Layout(action) => self.arrange_selection(action, cx),
            Command::Lock(value) | Command::Hide(value) => {
                let mut changes = Vec::new();
                for id in ids {
                    if let Some(layer) = self.layer_state_mut(*id) {
                        let before = *layer;
                        if matches!(command, Command::Lock(_)) {
                            layer.locked = value;
                        } else {
                            layer.hidden = value;
                        }
                        if before != *layer {
                            changes.push(Change::Layer {
                                id: *id,
                                value: before,
                            });
                        }
                    }
                }
                if !changes.is_empty() {
                    self.history.borrow_mut().record(changes, None);
                }
                let selected = self.selection_ids();
                self.set_selection(selected, cx);
            }
        }
        cx.notify();
    }
}
#[cfg(test)]
mod tests;
