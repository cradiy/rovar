use super::*;
use crate::i18n::t;
use layout::LayoutAction;
use shapes::NodeAction;
use std::collections::BTreeSet;
use uic::components::context_menu::{self, ContextMenu, ContextMenuAppearance, ContextMenuItem};

#[derive(Clone, Copy)]
enum Command {
    SaveAsset,
    Export(crate::component_export::Format),
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
            .shortcut(shortcut)
            .disabled(!enabled)
        };
        let order_enabled = editable && self.common_parent(&self.selection_ids()).is_some();
        let paste_enabled = self.can_paste_objects(cx);
        let mut menu = ContextMenu::new()
            .w(px(238.))
            .max_h(px(500.))
            .bg(gpui::transparent_black())
            .border_0()
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .appearance(ContextMenuAppearance {
                muted_foreground: rgb(MUTED).into(),
                selected_background: gpui::rgba(0xb4a2ee28).into(),
                selected_foreground: rgb(TEXT).into(),
                separator: gpui::rgba(0xb4a2ee28).into(),
                item_height: px(28.),
                ..Default::default()
            })
            .surface(|state, content, _, _| {
                layers::glass_surface()
                    .debug_selector(move || format!("editor-context-glass-{}", state.depth))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(gpui::rgba(0xb4a2ee45))
                    .shadow_lg()
                    .child(content)
            })
            .item(item(
                "context-undo",
                t("undo"),
                LucideIcons::Undo2,
                "Ctrl+Z",
                Command::Undo,
                self.history.borrow().can_undo(),
            ))
            .item(item(
                "context-redo",
                t("redo"),
                LucideIcons::Redo2,
                "Ctrl+Shift+Z",
                Command::Redo,
                self.history.borrow().can_redo(),
            ))
            .separator()
            .item(item(
                "context-cut",
                t("cut"),
                LucideIcons::Scissors,
                "Ctrl+X",
                Command::Cut,
                editable,
            ))
            .item(item(
                "context-copy",
                t("copy"),
                LucideIcons::Copy,
                "Ctrl+C",
                Command::Copy,
                editable,
            ))
            .item(item(
                "context-paste",
                t("paste"),
                LucideIcons::Clipboard,
                "Ctrl+V",
                Command::Paste,
                paste_enabled,
            ))
            .item(item(
                "context-paste-in-place",
                t("paste-in-place"),
                LucideIcons::ClipboardPaste,
                "Ctrl+Shift+V",
                Command::PasteInPlace,
                paste_enabled,
            ))
            .item(item(
                "context-duplicate",
                t("duplicate"),
                LucideIcons::CopyPlus,
                "Ctrl+D",
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
                t("assets-save-selection"),
                LucideIcons::Component,
                "",
                Command::SaveAsset,
                self.can_save_asset(cx),
            ))
            .separator()
            .item(item(
                "context-group",
                t("group"),
                LucideIcons::Group,
                "Ctrl+G",
                Command::Group,
                self.can_group(),
            ))
            .item(item(
                "context-ungroup",
                t("ungroup"),
                LucideIcons::Ungroup,
                "Ctrl+Shift+G",
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
                        "Ctrl+Shift+]",
                        Command::Front,
                        order_enabled,
                    ))
                    .item(item(
                        "context-raise",
                        t("bring-forward"),
                        LucideIcons::ArrowUp,
                        "Ctrl+]",
                        Command::Raise,
                        order_enabled,
                    ))
                    .item(item(
                        "context-lower",
                        t("send-backward"),
                        LucideIcons::ArrowDown,
                        "Ctrl+[",
                        Command::Lower,
                        order_enabled,
                    ))
                    .item(item(
                        "context-back",
                        t("send-back"),
                        LucideIcons::ChevronsDown,
                        "Ctrl+Shift+[",
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
                Command::Export(crate::component_export::Format::Png),
                !self.export.busy,
            ));
        } else {
            menu = menu.submenu(t("export-selection"), |menu| {
                use crate::component_export::Format;
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
            Command::Export(format) => self.export_selection(format, window, cx),
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
