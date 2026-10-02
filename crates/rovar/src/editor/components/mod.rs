use super::*;
use crate::document::{Document, Page};
use crate::scene::components::{self as model, Binding, Definition, Definitions};
use std::collections::BTreeSet;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct ComponentDrag {
    pub owner: gpui::EntityId,
    pub id: String,
    pub name: String,
    pub preview: Option<std::sync::Arc<gpui::RenderImage>>,
}
impl Render for ComponentDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(112.))
            .p(px(7.))
            .rounded(px(10.))
            .bg(rgb(0x282630))
            .border_1()
            .border_color(rgb(ACCENT))
            .shadow_lg()
            .when_some(self.preview.clone(), |el, preview| {
                el.child(
                    gpui::img(preview)
                        .w_full()
                        .h(px(76.))
                        .object_fit(gpui::ObjectFit::Contain),
                )
            })
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT))
                    .truncate()
                    .child(self.name.clone()),
            )
    }
}

#[derive(Default)]
pub(super) struct State {
    pub definitions: Definitions,
    pub revision: Option<u64>,
    pub previews:
        std::collections::BTreeMap<String, (Vec<u8>, Option<std::sync::Arc<gpui::RenderImage>>)>,
}

impl Workspace {
    pub(super) fn component_parent_allowed(&self, id: usize, parent: Option<usize>) -> bool {
        if !self
            .descendants(&BTreeSet::from([id]))
            .iter()
            .any(|id| self.hierarchy.components.contains_key(id))
        {
            return true;
        }
        parent.is_none_or(|p| {
            !self.hierarchy.components.contains_key(&p)
                && !self
                    .ancestors(p)
                    .iter()
                    .any(|p| self.hierarchy.components.contains_key(p))
        })
    }
    pub(super) fn avoid_component_nesting(&mut self, root: usize) {
        if !self.component_parent_allowed(root, self.layer_parent(root)) {
            self.hierarchy.parents.remove(&root);
            self.restore_copy_parent(root, None);
        }
    }
    fn component_pages(&self, cx: &gpui::App) -> Vec<Page> {
        self.pages
            .entries
            .iter()
            .map(|p| {
                if p.page.id == self.pages.active {
                    self.snapshot_page(cx).0
                } else {
                    p.page.clone()
                }
            })
            .collect()
    }
    pub(super) fn sync_components(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let revision = self.history.borrow().revision();
        if self.components.revision == Some(revision) || self.gesture.is_some() {
            return;
        }
        self.components.revision = Some(revision);
        if self.components.definitions.is_empty() {
            return;
        }
        // Unbound pages cannot contribute masters or receive instance updates.
        let mut pages: Vec<_> = self
            .pages
            .entries
            .iter()
            .filter_map(|state| {
                if state.page.id == self.pages.active {
                    (!self.hierarchy.components.is_empty()).then(|| self.snapshot_page(cx).0)
                } else {
                    (!state.page.hierarchy.components.is_empty()).then(|| state.page.clone())
                }
            })
            .collect();
        let before = pages
            .iter()
            .find(|page| page.id == self.pages.active)
            .cloned();
        if let Err(error) = model::synchronize(&mut pages, &mut self.components.definitions) {
            self.assets.error = Some(error.to_string());
            return;
        }
        for page in pages {
            if page.id == self.pages.active && before.as_ref() != Some(&page) {
                self.apply_component_page(page.clone(), window, cx);
            }
            if let Some(state) = self.pages.entries.iter_mut().find(|p| p.page.id == page.id) {
                state.page = page;
            }
        }
        self.refresh_component_previews(cx);
    }
    fn refresh_component_previews(&mut self, cx: &mut Context<Self>) {
        self.components
            .previews
            .retain(|id, _| self.components.definitions.contains_key(id));
        for (id, definition) in &self.components.definitions {
            let signature = serde_json::to_vec(&definition.page).unwrap();
            if self
                .components
                .previews
                .get(id)
                .is_some_and(|(old, _)| old == &signature)
            {
                continue;
            }
            let previous = self
                .components
                .previews
                .get(id)
                .and_then(|(_, p)| p.clone());
            self.components
                .previews
                .insert(id.clone(), (signature.clone(), previous));
            let id = id.clone();
            let json = signature.clone();
            let sources = model::sources(&definition.page);
            let text_system = cx.text_system().clone();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        crate::render::raster::prepare().await?;
                        let page: Page = serde_json::from_slice(&json)?;
                        let png = crate::document::render_preview(&page, &sources, &text_system)?;
                        let mut pixels = image::load_from_memory(&png)?.into_rgba8();
                        for pixel in pixels.pixels_mut() {
                            pixel.0.swap(0, 2);
                        }
                        anyhow::Ok(std::sync::Arc::new(gpui::RenderImage::new(vec![
                            image::Frame::new(pixels),
                        ])))
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if let Some((current, preview)) = this.components.previews.get_mut(&id)
                        && *current == signature
                    {
                        *preview = result.ok();
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
    pub(super) fn apply_component_page(
        &mut self,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.boards = page.boards;
        self.shapes = page.shapes;
        self.hierarchy = page.hierarchy;
        self.next_id = page.next_id;
        self.texts
            .retain(|t| page.texts.iter().any(|x| x.id == t.id));
        for text in page.texts {
            if !self.texts.iter().any(|t| t.id == text.id) {
                let item = self.make_text(text.id, text.board, text.rect, window, cx);
                self.texts.push(item);
            }
            let item = self.texts.iter_mut().find(|t| t.id == text.id).unwrap();
            let old = item
                .editor
                .read(cx)
                .document_text(item.id, item.board, item.rect, item.layer);
            item.rect = text.rect;
            item.board = text.board;
            item.layer = text.layer;
            if old.content != text.content || old.styles != text.styles {
                item.editor.update(cx, |editor, cx| {
                    editor.restore(crate::scene::text::Snapshot::from_document(text));
                    cx.notify();
                });
            }
        }
        self.auto_layout.revision = None;
        self.reflow_layout(cx);
        let selected = self
            .selection_ids()
            .into_iter()
            .filter(|id| self.layer_info(*id).is_some())
            .collect();
        self.set_selection(selected, cx);
        self.sync_fields(cx);
        self.scene.update(cx, |_, cx| cx.notify());
    }
    pub(super) fn can_create_component(&self) -> bool {
        let ids = self.selection_ids();
        self.gesture.is_none()
            && !ids.is_empty()
            && self.common_parent(&ids).is_some()
            && ids.iter().all(|id| {
                self.layer_editable(*id)
                    && !self.hierarchy.components.contains_key(id)
                    && !self
                        .ancestors(*id)
                        .iter()
                        .any(|p| self.hierarchy.components.contains_key(p))
                    && !self
                        .descendants(&BTreeSet::from([*id]))
                        .iter()
                        .any(|c| self.hierarchy.components.contains_key(c))
            })
    }
    pub(super) fn create_component(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_create_component() {
            return;
        }
        self.suspend(window, cx);
        let before = self.page_edit(std::slice::from_ref(&self.pages.active), cx);
        if self.selection_ids().len() > 1 {
            self.history.borrow_mut().suppressed = true;
            self.group_selection(cx);
            self.history.borrow_mut().suppressed = false;
        }
        let root = *self.selection_ids().first().unwrap();
        let page = self.snapshot_page(cx).0;
        let Ok(template) = model::extract(&page, root) else {
            return;
        };
        let id = uuid::Uuid::new_v4().to_string();
        let definition = Definition {
            name: self.layer_name(root, cx),
            source: None,
            root,
            page: template,
        };
        let link = Binding {
            component: id.clone(),
            master: true,
            nodes: model::ids(&definition.page)
                .into_iter()
                .map(|i| (i, i))
                .collect(),
            baseline: serde_json::to_value(&definition.page).unwrap(),
        };
        self.hierarchy.components.insert(root, link);
        self.components.definitions.insert(id, definition);
        self.record_page_edit(before);
        self.assets.scope = assets::Scope::Document;
        self.sidebar.resources = true;
        self.sidebar.collapsed = false;
        self.components.revision = None;
        cx.notify();
    }
    pub(super) fn insert_document_component(
        &mut self,
        id: &str,
        master: bool,
        offset: Option<Point<f32>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.suspend(window, cx);
        self.sync_components(window, cx);
        let Some(definition) = self.components.definitions.get(id).cloned() else {
            return;
        };
        let before = self.page_edit(std::slice::from_ref(&self.pages.active), cx);
        let size = model::bounds(&definition.page, definition.root).unwrap();
        let offset = offset.unwrap_or_else(|| {
            let bounds = self.bounds.get();
            let (left, right) = self.canvas_insets();
            let center = self.view.world(point(
                (left + f32::from(bounds.size.width) - right) / 2.,
                f32::from(bounds.size.height) / 2.,
            ));
            point(center.x - size.width / 2., center.y - size.height / 2.)
        });
        let first = self.next_id;
        let nodes = model::ids(&definition.page)
            .into_iter()
            .enumerate()
            .map(|(n, id)| (id, first + n))
            .collect();
        let suppressed = self.history.borrow().suppressed;
        self.history.borrow_mut().suppressed = true;
        self.insert_component_document(
            Document::single(definition.page.clone()),
            definition.name.clone(),
            offset,
            window,
            cx,
        );
        self.history.borrow_mut().suppressed = suppressed;
        let root = *self.selection_ids().first().unwrap();
        self.hierarchy.components.insert(
            root,
            Binding {
                component: id.into(),
                master,
                nodes,
                baseline: serde_json::to_value(&definition.page).unwrap(),
            },
        );
        self.avoid_component_nesting(root);
        self.record_page_edit(before);
        self.components.revision = None;
        cx.notify();
    }
    pub(super) fn import_library_component(
        &mut self,
        source: String,
        name: String,
        document: Document,
        offset: Point<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self
            .components
            .definitions
            .iter()
            .find(|(_, c)| c.source.as_ref() == Some(&source))
            .map(|(id, _)| id.clone())
        {
            self.insert_document_component(&id, false, Some(offset), window, cx);
            return;
        }
        self.suspend(window, cx);
        let before = self.page_edit(std::slice::from_ref(&self.pages.active), cx);
        let mut page = document.first_page().clone();
        page.hierarchy.components.clear();
        let roots: Vec<_> = model::ids(&page)
            .into_iter()
            .filter(|id| model::parent(&page, *id).is_none())
            .collect();
        let root = if roots.len() == 1 {
            roots[0]
        } else {
            let root = page.next_id;
            page.next_id += 1;
            page.hierarchy.groups.insert(
                root,
                crate::scene::layer::LayerGroup {
                    name: name.clone(),
                    board: None,
                    layer: Default::default(),
                },
            );
            for id in roots {
                page.hierarchy.parents.insert(id, root);
            }
            root
        };
        let id = uuid::Uuid::new_v4().to_string();
        self.components.definitions.insert(
            id.clone(),
            Definition {
                name,
                source: Some(source),
                root,
                page,
            },
        );
        self.history.borrow_mut().suppressed = true;
        self.insert_document_component(&id, false, Some(offset), window, cx);
        self.history.borrow_mut().suppressed = false;
        // Insertion records a page edit; replace its scope with the pre-import state.
        self.record_page_edit(before);
        cx.notify();
    }
    pub(super) fn edit_document_component(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = self.component_pages(cx).iter().find_map(|p| {
            p.hierarchy
                .components
                .iter()
                .find(|(_, b)| b.master && b.component == id)
                .map(|(root, _)| (p.id.clone(), *root))
        });
        if let Some((page, root)) = existing {
            self.switch_page(&page, window, cx);
            self.set_selection(BTreeSet::from([root]), cx);
            self.sidebar.resources = false;
            cx.notify();
        } else {
            self.insert_document_component(id, true, None, window, cx);
            self.sidebar.resources = false;
        }
    }
    pub(super) fn detach_component(&mut self, root: usize, cx: &mut Context<Self>) {
        if !self.layer_editable(root) || !self.hierarchy.components.contains_key(&root) {
            return;
        }
        let before = self.snapshot_hierarchy();
        self.hierarchy.components.remove(&root);
        self.history.borrow_mut().record(vec![before], None);
        self.components.revision = None;
        cx.notify();
    }
    pub(super) fn reset_component(
        &mut self,
        root: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(link) = self
            .hierarchy
            .components
            .get(&root)
            .filter(|l| !l.master)
            .cloned()
        else {
            return;
        };
        let before = self.page_edit(std::slice::from_ref(&self.pages.active), cx);
        let page = self.snapshot_page(cx).0;
        if let Ok(current) = model::extract(&page, root) {
            let mut reverse: std::collections::BTreeMap<_, _> =
                link.nodes.iter().map(|(a, b)| (*b, *a)).collect();
            let mut next = self.components.definitions[&link.component]
                .page
                .next_id
                .max(link.nodes.keys().last().copied().unwrap_or(0) + 1);
            for id in model::ids(&current) {
                reverse.entry(id).or_insert_with(|| {
                    let id = next;
                    next += 1;
                    id
                });
            }
            let template = model::place(&current, &reverse, [0., 0.], None);
            let binding = self.hierarchy.components.get_mut(&root).unwrap();
            binding
                .nodes
                .extend(reverse.into_iter().map(|(a, b)| (b, a)));
            binding.baseline = serde_json::to_value(template).unwrap();
            self.components.revision = None;
            self.sync_components(window, cx);
            self.record_page_edit(before);
            cx.notify();
        }
    }
}
