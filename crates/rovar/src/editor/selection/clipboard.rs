use super::*;
use crate::scene::history::SavedText;
use std::collections::BTreeMap;

#[derive(Clone, Default)]
struct ObjectClipboard {
    colors: crate::scene::color_styles::Palette,
    source: Option<gpui::EntityId>,
    source_page: Option<String>,
    hierarchy: crate::scene::layer::Hierarchy,
    definitions: crate::scene::components::Definitions,
    boards: Vec<Artboard>,
    shapes: Vec<Shape>,
    texts: Vec<SavedText>,
    roots: BTreeSet<usize>,
    root_parents: BTreeMap<usize, Option<usize>>,
    marker: String,
    pastes: usize,
}

#[derive(Default)]
struct Clipboard(Option<ObjectClipboard>);
impl gpui::Global for Clipboard {}

fn visit_clipboard_colors(
    clipboard: &mut ObjectClipboard,
    mut f: impl FnMut(
        &mut Option<String>,
        &mut gpui::Rgba,
        &mut FillMode,
        &mut crate::scene::artboard::LinearGradient,
    ),
) {
    for board in &mut clipboard.boards {
        f(
            &mut board.color_style,
            &mut board.color,
            &mut board.fill_mode,
            &mut board.gradient,
        );
    }
    for shape in &mut clipboard.shapes {
        f(
            &mut shape.color_style,
            &mut shape.color,
            &mut shape.fill_mode,
            &mut shape.gradient,
        );
        f(
            &mut shape.stroke.color_style,
            &mut shape.stroke.color,
            &mut shape.stroke.fill_mode,
            &mut shape.stroke.gradient,
        );
    }
    for text in &mut clipboard.texts {
        text.text.visit_color_styles(&mut f);
    }
    for definition in clipboard.definitions.values_mut() {
        crate::scene::color_styles::visit(&mut definition.page, &mut f);
    }
    for binding in clipboard.hierarchy.components.values_mut() {
        if let Ok(mut page) =
            serde_json::from_value::<crate::document::Page>(binding.baseline.clone())
        {
            crate::scene::color_styles::visit(&mut page, &mut f);
            binding.baseline = serde_json::to_value(page).unwrap();
        }
    }
}

impl Workspace {
    fn import_clipboard_colors(&mut self, clipboard: &mut ObjectClipboard, same_document: bool) {
        let mut used = BTreeSet::new();
        visit_clipboard_colors(clipboard, |reference, _, _, _| {
            if let Some(id) = reference {
                used.insert(id.clone());
            }
        });
        let mut mapped = BTreeMap::new();
        for id in used {
            let Some(color) = clipboard.colors.get(&id) else {
                continue;
            };
            let target =
                if !same_document && self.colors.palette.get(&id).is_some_and(|c| c != color) {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    id.clone()
                };
            self.colors
                .palette
                .entry(target.clone())
                .or_insert_with(|| color.clone());
            mapped.insert(id, target);
        }
        visit_clipboard_colors(clipboard, |reference, color, mode, gradient| {
            if let Some(id) = reference.clone() {
                *reference = mapped.get(&id).cloned();
                if let Some(style) = reference
                    .as_ref()
                    .and_then(|id| self.colors.palette.get(id))
                {
                    style.apply(color, mode, gradient);
                }
            }
        });
    }
    pub(in crate::editor) fn can_paste_objects(&self, cx: &gpui::App) -> bool {
        cx.try_global::<Clipboard>()
            .and_then(|c| c.0.as_ref())
            .is_some_and(|clipboard| {
                cx.read_from_clipboard()
                    .as_ref()
                    .and_then(|item| item.metadata())
                    == Some(&clipboard.marker)
            })
    }
    fn snapshot_selection(&self, cx: &gpui::App) -> ObjectClipboard {
        let roots = self.selection_ids();
        let included_ids = self.descendants(&roots);
        let boards: Vec<_> = self
            .boards
            .iter()
            .filter(|b| included_ids.contains(&b.id))
            .cloned()
            .collect();
        let copied_boards: BTreeSet<_> = boards.iter().map(|b| b.id).collect();
        let included = |id, parent: Option<usize>| {
            included_ids.contains(&id) || parent.is_some_and(|p| copied_boards.contains(&p))
        };
        let shapes = self
            .shapes
            .iter()
            .filter(|s| included(s.id, s.board))
            .cloned()
            .map(|mut s| {
                if s.board.is_none_or(|p| !copied_boards.contains(&p)) {
                    s.rect = self.world_rect(s.id).unwrap();
                    s.board = None;
                }
                s
            })
            .collect();
        let texts = self
            .texts
            .iter()
            .enumerate()
            .filter(|(_, t)| included(t.id, t.board))
            .map(|(i, _)| {
                let mut t = self.saved_text(i, cx);
                if t.board.is_none_or(|p| !copied_boards.contains(&p)) {
                    t.rect = self.world_rect(t.id).unwrap();
                    t.board = None;
                }
                t
            })
            .collect();
        let mut hierarchy = self.hierarchy.clone();
        hierarchy.groups.retain(|id, _| included_ids.contains(id));
        hierarchy
            .parents
            .retain(|id, p| included_ids.contains(id) && included_ids.contains(p));
        hierarchy.names.retain(|id, _| included_ids.contains(id));
        hierarchy.order.retain(|id| included_ids.contains(id));
        hierarchy.layouts.retain(|id, _| included_ids.contains(id));
        hierarchy.sizing.retain(|id, _| included_ids.contains(id));
        hierarchy.exports.retain(|id, _| included_ids.contains(id));
        hierarchy.shadows.retain(|id, _| included_ids.contains(id));
        hierarchy
            .components
            .retain(|id, _| included_ids.contains(id));
        for (id, layout) in &mut hierarchy.layouts {
            if let Some(group) = hierarchy.groups.get(id)
                && group.board.is_none_or(|b| !copied_boards.contains(&b))
            {
                let origin = self.parent_origin(group.board);
                layout.frame.x += origin.x;
                layout.frame.y += origin.y;
            }
        }
        let root_parents = roots
            .iter()
            .map(|id| (*id, self.layer_parent(*id)))
            .collect();
        ObjectClipboard {
            colors: self.colors.palette.clone(),
            definitions: self
                .components
                .definitions
                .iter()
                .filter(|(id, _)| hierarchy.components.values().any(|b| &b.component == *id))
                .map(|(id, definition)| (id.clone(), definition.clone()))
                .collect(),
            root_parents,
            hierarchy,
            boards,
            shapes,
            texts,
            roots,
            ..Default::default()
        }
    }

    pub(in crate::editor) fn copy_selection(&mut self, cx: &mut Context<Self>) {
        if self.selection_ids().is_empty() {
            return;
        }
        self.seal_text_edits(cx);
        let mut clipboard = self.snapshot_selection(cx);
        clipboard.source = Some(cx.entity_id());
        clipboard.source_page = Some(self.pages.active.clone());
        clipboard.marker = format!("rovar-objects:{:?}", web_time::SystemTime::now());
        cx.write_to_clipboard(gpui::ClipboardItem::new_string_with_metadata(
            crate::i18n::count("clipboard-summary", clipboard.roots.len()),
            clipboard.marker.clone(),
        ));
        cx.set_global(Clipboard(Some(clipboard)));
    }

    pub(in crate::editor) fn paste_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.paste_objects(false, window, cx);
    }
    pub(in crate::editor) fn paste_in_place(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.paste_objects(true, window, cx);
    }
    pub(in crate::editor) fn cut_selection(&mut self, cx: &mut Context<Self>) {
        if self.selection_ids().is_empty() {
            return;
        }
        self.copy_selection(cx);
        self.delete_selected(cx);
    }
    fn paste_objects(&mut self, in_place: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut clipboard) = cx.try_global::<Clipboard>().and_then(|c| c.0.clone()) else {
            return;
        };
        if cx
            .read_from_clipboard()
            .as_ref()
            .and_then(|item| item.metadata())
            != Some(&clipboard.marker)
        {
            return;
        }
        let offset = if in_place {
            0.
        } else {
            clipboard.pastes += 1;
            20. * clipboard.pastes as f32
        };
        cx.set_global(Clipboard(Some(clipboard.clone())));
        if clipboard.source != Some(cx.entity_id())
            || clipboard.source_page.as_deref() != Some(&self.pages.active)
        {
            clipboard.root_parents.clear();
        }
        self.insert_copies(clipboard, point(offset, offset), in_place, true, window, cx);
    }

    pub(in crate::editor) fn duplicate_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.seal_text_edits(cx);
        self.sync_components(window, cx);
        let ids = self.selection_ids();
        if ids.iter().any(|id| !self.layer_editable(*id)) {
            return;
        }
        let Some(bounds) = ids
            .iter()
            .filter_map(|id| self.world_bounds(*id))
            .reduce(super::super::layout::union)
        else {
            return;
        };
        let origin = point(bounds.x, bounds.y);
        let offset = self
            .duplicate
            .as_ref()
            .filter(|d| d.ids == ids)
            .map_or(point(20., 20.), |d| origin - d.source_origin);
        self.insert_copies(self.snapshot_selection(cx), offset, false, true, window, cx);
        self.duplicate = Some(super::Duplicate {
            ids: self.selection_ids(),
            source_origin: origin,
        });
    }

    pub(in crate::editor) fn move_selection_to_page(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if id == self.pages.active
            || !self.pages.entries.iter().any(|p| p.page.id == id)
            || self.selection_ids().is_empty()
            || self
                .selection_ids()
                .iter()
                .any(|id| !self.layer_editable(*id))
        {
            return;
        }
        self.suspend(window, cx);
        let before = self.page_edit(&[self.pages.active.clone(), id.into()], cx);
        let mut clipboard = self.snapshot_selection(cx);
        clipboard.root_parents.clear();
        self.history.borrow_mut().suppressed = true;
        self.delete_selected(cx);
        self.switch_page(id, window, cx);
        self.insert_copies(clipboard, point(0., 0.), true, false, window, cx);
        self.history.borrow_mut().suppressed = false;
        self.record_page_edit(before);
        cx.notify();
    }

    pub(in crate::editor) fn insert_component_document(
        &mut self,
        document: crate::document::Document,
        name: String,
        offset: Point<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut document = document.first_page().clone();
        let roots = document
            .boards
            .iter()
            .map(|b| (b.id, None))
            .chain(document.shapes.iter().map(|s| (s.id, s.board)))
            .chain(document.texts.iter().map(|t| (t.id, t.board)))
            .chain(
                document
                    .hierarchy
                    .groups
                    .iter()
                    .map(|(id, g)| (*id, g.board)),
            )
            .filter(|(id, board)| board.is_none() && !document.hierarchy.parents.contains_key(id))
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>();
        for id in &roots {
            document.hierarchy.names.insert(*id, name.clone());
        }
        let clipboard = ObjectClipboard {
            boards: document.boards,
            shapes: document.shapes,
            hierarchy: document.hierarchy,
            roots,
            texts: document
                .texts
                .into_iter()
                .map(|text| SavedText {
                    uid: text.uid,
                    id: text.id,
                    layer: text.layer,
                    board: text.board,
                    rect: text.rect,
                    text: crate::scene::text::Snapshot::from_document(text),
                })
                .collect(),
            ..Default::default()
        };
        self.cancel_gesture(window, cx);
        self.seal_text_edits(cx);
        self.draw_tool = None;
        self.vector_edit = None;
        self.insert_copies(clipboard, offset, false, false, window, cx);
    }

    pub(in crate::editor) fn restore_copy_parent(&mut self, id: usize, parent: Option<usize>) {
        let board = parent.and_then(|parent| {
            if self.boards.iter().any(|b| b.id == parent) {
                Some(parent)
            } else {
                self.hierarchy.groups.get(&parent).and_then(|g| g.board)
            }
        });
        let ids = self.descendants(&BTreeSet::from([id]));
        let positions: Vec<_> = ids
            .iter()
            .filter_map(|id| {
                self.object_rect(*id)
                    .filter(|(p, _)| p.is_none_or(|p| !ids.contains(&p)))
                    .and_then(|_| self.world_rect(*id).map(|r| (*id, r)))
            })
            .collect();
        let origin = self.parent_origin(board);
        for (id, mut rect) in positions {
            if self.boards.iter().any(|b| b.id == id) {
                continue;
            }
            rect.x -= origin.x;
            rect.y -= origin.y;
            self.set_object_rect(id, board, rect);
        }
        for child in &ids {
            if let Some(group) = self.hierarchy.groups.get_mut(child)
                && group.board.is_none_or(|p| !ids.contains(&p))
            {
                group.board = board;
            }
        }
        if let Some(parent) = parent.filter(|p| self.hierarchy.groups.contains_key(p)) {
            self.hierarchy.parents.insert(id, parent);
        }
    }
    fn insert_copies(
        &mut self,
        mut clipboard: ObjectClipboard,
        offset: Point<f32>,
        in_place: bool,
        rename: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let before = self.snapshot_hierarchy();
        let import_before = (clipboard
            .colors
            .iter()
            .any(|(id, c)| self.colors.palette.get(id) != Some(c))
            || clipboard
                .definitions
                .keys()
                .any(|id| !self.components.definitions.contains_key(id)))
        .then(|| self.page_edit(std::slice::from_ref(&self.pages.active), cx));
        let same_document = clipboard
            .source
            .is_none_or(|source| source == cx.entity_id());
        self.import_clipboard_colors(&mut clipboard, same_document);
        for (id, definition) in clipboard.definitions {
            self.components.definitions.entry(id).or_insert(definition);
        }
        let mut id_map = BTreeMap::new();
        // Preserve the relative stacking of mixed text and shape objects.
        let ids: BTreeSet<_> = clipboard
            .boards
            .iter()
            .map(|b| b.id)
            .chain(clipboard.shapes.iter().map(|s| s.id))
            .chain(clipboard.texts.iter().map(|t| t.id))
            .chain(clipboard.hierarchy.groups.keys().copied())
            .collect();
        for id in ids {
            id_map.insert(id, self.next_id);
            self.next_id += 1;
        }
        let mut changes = Vec::new();
        for mut board in clipboard.boards {
            board.uid = uuid::Uuid::new_v4();
            board.id = id_map[&board.id];
            if rename {
                board.name = crate::i18n::message("copy-name", &[("name", board.name.clone())]);
            }
            board.rect.x += offset.x;
            board.rect.y += offset.y;
            changes.push(Change::Board {
                id: board.id,
                index: self.boards.len(),
                value: None,
            });
            self.boards.push(board);
        }
        for mut shape in clipboard.shapes {
            shape.uid = uuid::Uuid::new_v4();
            shape.id = id_map[&shape.id];
            if rename {
                shape.name = crate::i18n::message("copy-name", &[("name", shape.name.clone())]);
            }
            if let Some(parent) = shape.board {
                shape.board = Some(id_map[&parent]);
            } else {
                shape.rect.x += offset.x;
                shape.rect.y += offset.y;
                (shape.board, shape.rect) = self.parent_for_rect(None, shape.rect);
            }
            changes.push(Change::Shape {
                id: shape.id,
                index: self.shapes.len(),
                value: None,
            });
            self.shapes.push(shape);
        }
        for mut saved in clipboard.texts {
            saved.id = id_map[&saved.id];
            if let Some(parent) = saved.board {
                saved.board = Some(id_map[&parent]);
            } else {
                saved.rect.x += offset.x;
                saved.rect.y += offset.y;
                (saved.board, saved.rect) = self.parent_for_rect(None, saved.rect);
            }
            let mut text = self.make_text(saved.id, saved.board, saved.rect, window, cx);
            text.layer = saved.layer;
            text.editor.update(cx, |editor, _| {
                editor.restore(saved.text);
                editor.editing = false;
            });
            changes.push(Change::TextBox {
                id: text.id,
                index: self.texts.len(),
                value: None,
            });
            self.texts.push(text);
        }
        for (id, mut group) in clipboard.hierarchy.groups {
            group.uid = uuid::Uuid::new_v4();
            group.board = group.board.and_then(|p| id_map.get(&p).copied());
            self.hierarchy.groups.insert(id_map[&id], group);
        }
        for (id, parent) in clipboard.hierarchy.parents {
            self.hierarchy.parents.insert(id_map[&id], id_map[&parent]);
        }
        for (id, mut layout) in clipboard.hierarchy.layouts {
            if self
                .hierarchy
                .groups
                .get(&id_map[&id])
                .is_some_and(|g| g.board.is_none())
            {
                layout.frame.x += offset.x;
                layout.frame.y += offset.y;
            }
            self.hierarchy.layouts.insert(id_map[&id], layout);
        }
        for (id, sizing) in clipboard.hierarchy.sizing {
            self.hierarchy.sizing.insert(id_map[&id], sizing);
        }
        for (id, presets) in clipboard.hierarchy.exports {
            self.hierarchy.exports.insert(id_map[&id], presets);
        }
        for (id, shadows) in clipboard.hierarchy.shadows {
            self.hierarchy.shadows.insert(id_map[&id], shadows);
        }
        for (root, mut link) in clipboard.hierarchy.components {
            for id in link.nodes.values_mut() {
                *id = id_map.get(id).copied().unwrap_or_else(|| {
                    let reserved = self.next_id;
                    self.next_id += 1;
                    reserved
                });
            }
            if rename {
                link.master = false;
            }
            self.hierarchy.components.insert(id_map[&root], link);
        }
        for (id, name) in clipboard.hierarchy.names {
            self.hierarchy.names.insert(id_map[&id], name);
        }
        // Materialize existing order before adding copied objects above it.
        let mut existing = self.layer_ids();
        existing.retain(|id| !id_map.values().any(|new| new == id));
        let ranks: BTreeMap<_, _> = self
            .hierarchy
            .order
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        existing.sort_unstable_by_key(|id| ranks.get(id).map_or((1, *id), |rank| (0, *rank)));
        let mut copied: Vec<_> = id_map.keys().copied().collect();
        let ranks: BTreeMap<_, _> = clipboard
            .hierarchy
            .order
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        copied.sort_unstable_by_key(|id| ranks.get(id).map_or((1, *id), |rank| (0, *rank)));
        existing.extend(copied.into_iter().map(|id| id_map[&id]));
        self.hierarchy.order = existing;
        for id in &clipboard.roots {
            let parent = clipboard.root_parents.get(id).copied().flatten();
            let id = id_map[id];
            if in_place && parent.is_none_or(|p| self.layer_editable(p)) {
                self.restore_copy_parent(id, parent);
            } else if self.hierarchy.groups.contains_key(&id) {
                self.reparent_group(id);
            }
            self.avoid_component_nesting(id);
            self.refresh_constraints(id);
        }
        if let Change::Hierarchy { ref value, .. } = before
            && *value != self.hierarchy
        {
            changes.insert(0, before);
        }
        if changes.is_empty() {
            return;
        }
        if let Some(before) = import_before {
            self.record_page_edit(before);
        } else {
            self.history.borrow_mut().record(changes, None);
        }
        self.set_selection(
            clipboard.roots.into_iter().map(|id| id_map[&id]).collect(),
            cx,
        );
        self.focus.focus(window, cx);
        cx.notify();
    }
}
