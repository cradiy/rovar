use super::*;
use crate::{
    document::Page,
    history::PageEdit,
    i18n::{message, t},
};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;
mod view;
pub(super) use crate::history::SavedPage as PageState;

#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Views {
    pub active: String,
    pub pages: BTreeMap<String, [f32; 3]>,
}

pub(super) struct State {
    pub entries: Vec<PageState>,
    pub active: String,
    pub generation: usize,
    expanded: bool,
    pub(super) renaming: Option<String>,
    input: Entity<TextInput>,
    pub(super) delete: Option<String>,
    row_bounds: Rc<std::cell::RefCell<BTreeMap<String, Bounds<Pixels>>>>,
    _subscriptions: Vec<Subscription>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let page = Page::empty(message("page-default-name", &[("id", "1".into())]));
        let input = cx.new(TextInput::new);
        let mut subscriptions =
            vec![
                cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Submit(_)) {
                        this.finish_page_rename(true, window, cx);
                    }
                }),
            ];
        subscriptions.push(
            cx.on_blur(&input.focus_handle(cx), window, |this, window, cx| {
                if this.pages.renaming.is_some() {
                    this.finish_page_rename(true, window, cx);
                }
            }),
        );
        Self {
            active: page.id.clone(),
            generation: 0,
            entries: vec![PageState::new(page)],
            expanded: true,
            renaming: None,
            input,
            delete: None,
            row_bounds: Default::default(),
            _subscriptions: subscriptions,
        }
    }
    pub fn current(&self) -> &PageState {
        self.entries
            .iter()
            .find(|state| state.page.id == self.active)
            .expect("active page")
    }
}

impl Workspace {
    fn current_page_state(&self, cx: &gpui::App) -> PageState {
        PageState {
            page: self.snapshot_page(cx).0,
            view: self.view_state(),
            selection: self.selection_ids(),
            folded: self.sidebar.folded.clone(),
        }
    }
    fn park_page(&mut self, cx: &gpui::App) {
        let state = self.current_page_state(cx);
        *self
            .pages
            .entries
            .iter_mut()
            .find(|p| p.page.id == self.pages.active)
            .unwrap() = state;
    }
    fn activate_page(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let state = self
            .pages
            .entries
            .iter()
            .find(|p| p.page.id == id)
            .unwrap()
            .clone();
        self.pages.active = id.into();
        self.pages.generation += 1;
        self.history.borrow_mut().set_page(id.into());
        self.selected = None;
        self.selected_text = None;
        self.selected_shape = None;
        self.multi_selection.clear();
        self.vector_edit = None;
        self.selected_node = None;
        self.vector_hover = None;
        self.draw_tool = None;
        self.shape_paths.borrow_mut().clear();
        self.videos.clear();
        self.video_loading.clear();
        self.image_fill_loading = None;
        self.media_error = None;
        self.load_page(state.page, window, cx);
        self.restore_view(state.view);
        self.sidebar.folded = state.folded;
        self.set_selection(state.selection, cx);
        self.focus_canvas(window, cx);
        cx.notify();
    }
    pub(super) fn switch_page(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.pages.active == id || !self.pages.entries.iter().any(|p| p.page.id == id) {
            return;
        }
        self.finish_page_rename(true, window, cx);
        self.finish_rename(true, window, cx);
        self.suspend(window, cx);
        self.sync_components(window, cx);
        self.park_page(cx);
        self.activate_page(id, window, cx);
    }
    pub(crate) fn page_views(&self) -> Views {
        Views {
            active: self.pages.active.clone(),
            pages: self
                .pages
                .entries
                .iter()
                .map(|state| {
                    (
                        state.page.id.clone(),
                        if state.page.id == self.pages.active {
                            self.view_state()
                        } else {
                            state.view
                        },
                    )
                })
                .collect(),
        }
    }
    pub(crate) fn restore_page_views(
        &mut self,
        views: Views,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for state in &mut self.pages.entries {
            if let Some(view) = views
                .pages
                .get(&state.page.id)
                .filter(|view| view.iter().all(|v| v.is_finite()))
            {
                state.view = *view;
            }
        }
        let id = if self.pages.entries.iter().any(|p| p.page.id == views.active) {
            views.active
        } else {
            self.pages.active.clone()
        };
        self.activate_page(&id, window, cx);
    }
    pub(in crate::workspace) fn page_edit(&self, ids: &[String], cx: &gpui::App) -> PageEdit {
        PageEdit {
            components: self.components.definitions.clone(),
            pages: ids
                .iter()
                .map(|id| {
                    let state = self
                        .pages
                        .entries
                        .iter()
                        .find(|p| &p.page.id == id)
                        .map(|state| {
                            if id == &self.pages.active {
                                self.current_page_state(cx)
                            } else {
                                state.clone()
                            }
                        });
                    (id.clone(), state)
                })
                .collect(),
            order: self
                .pages
                .entries
                .iter()
                .map(|p| p.page.id.clone())
                .collect(),
            active: self.pages.active.clone(),
        }
    }
    pub(in crate::workspace) fn record_page_edit(&mut self, before: PageEdit) {
        self.history
            .borrow_mut()
            .record_for(before.active.clone(), vec![Change::Pages { value: before }]);
    }
    pub(super) fn replay_page_edit(
        &mut self,
        value: PageEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PageEdit {
        let ids = value.pages.keys().cloned().collect::<Vec<_>>();
        self.suspend(window, cx);
        self.park_page(cx);
        let inverse = self.page_edit(&ids, cx);
        self.components.definitions = value.components;
        self.components.revision = None;
        for (id, state) in value.pages {
            self.pages.entries.retain(|p| p.page.id != id);
            if let Some(state) = state {
                self.pages.entries.push(state);
            }
        }
        self.pages
            .entries
            .sort_by_key(|p| value.order.iter().position(|id| id == &p.page.id));
        self.activate_page(&value.active, window, cx);
        inverse
    }
    pub(super) fn add_page(
        &mut self,
        duplicate: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.suspend(window, cx);
        self.finish_page_rename(true, window, cx);
        self.sync_components(window, cx);
        self.park_page(cx);
        let page = if let Some(id) = duplicate {
            let Some(source) = self.pages.entries.iter().find(|p| p.page.id == id) else {
                return;
            };
            let mut page = source.page.clone();
            for link in page.hierarchy.components.values_mut() {
                link.master = false;
            }
            page.id = uuid::Uuid::new_v4().to_string();
            page.name = message(
                "page-copy-name",
                &[("name", page.name.chars().take(180).collect())],
            );
            page
        } else {
            let mut number = self.pages.entries.len() + 1;
            let name = loop {
                let name = message("page-default-name", &[("id", number.to_string())]);
                if !self.pages.entries.iter().any(|p| p.page.name == name) {
                    break name;
                }
                number += 1;
            };
            Page::empty(name)
        };
        let id = page.id.clone();
        let before = self.page_edit(std::slice::from_ref(&id), cx);
        let index = duplicate
            .and_then(|id| self.pages.entries.iter().position(|p| p.page.id == id))
            .unwrap_or_else(|| {
                self.pages
                    .entries
                    .iter()
                    .position(|p| p.page.id == self.pages.active)
                    .unwrap()
            })
            + 1;
        self.pages.entries.insert(index, PageState::new(page));
        self.pages.expanded = true;
        self.activate_page(&id, window, cx);
        self.record_page_edit(before);
    }
    pub(super) fn delete_page(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.pages.delete = None;
        if self.pages.entries.len() <= 1 {
            return;
        }
        let Some(index) = self.pages.entries.iter().position(|p| p.page.id == id) else {
            return;
        };
        self.suspend(window, cx);
        self.park_page(cx);
        let before = self.page_edit(&[id.into()], cx);
        let next = self.pages.entries[if index + 1 < self.pages.entries.len() {
            index + 1
        } else {
            index - 1
        }]
        .page
        .id
        .clone();
        self.pages.entries.remove(index);
        if self.pages.active == id {
            self.activate_page(&next, window, cx);
        }
        self.record_page_edit(before);
        cx.notify();
    }
    pub(super) fn reorder_page(&mut self, id: &str, before: Option<&str>, cx: &mut Context<Self>) {
        let Some(index) = self.pages.entries.iter().position(|p| p.page.id == id) else {
            return;
        };
        if before == Some(id) {
            return;
        }
        let previous = self.page_edit(&[], cx);
        let page = self.pages.entries.remove(index);
        let index = before
            .and_then(|id| self.pages.entries.iter().position(|p| p.page.id == id))
            .unwrap_or(self.pages.entries.len());
        self.pages.entries.insert(index, page);
        if self
            .pages
            .entries
            .iter()
            .map(|p| &p.page.id)
            .ne(previous.order.iter())
        {
            self.record_page_edit(previous);
            cx.notify();
        }
    }
}
