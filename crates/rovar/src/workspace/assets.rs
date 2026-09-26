use super::*;
use crate::{
    component_library::{Entry, Library},
    i18n::t,
};
use gpui::{AppContext, Focusable};
mod document;
#[cfg(test)]
mod tests;
mod view;
use document::SavedComponent;

pub(super) enum Dialog {
    Save(SavedComponent),
    Rename(Entry),
    Delete(Entry),
}

pub(super) struct State {
    library: Option<Entity<Library>>,
    search: Entity<TextInput>,
    name: Entity<TextInput>,
    pub dialog: Option<Dialog>,
    pub inserting: bool,
    insert_request: u64,
    error: Option<String>,
    observer: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let search = cx.new(|cx| TextInput::new(cx).placeholder(t("assets-search")));
        let name = cx.new(TextInput::new);
        let subscriptions = vec![
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.observe(&name, |_, _, cx| cx.notify()),
            cx.subscribe_in(&name, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Submit(_)) {
                    this.confirm_asset_dialog(window, cx);
                }
            }),
        ];
        Self {
            library: None,
            search,
            name,
            dialog: None,
            inserting: false,
            insert_request: 0,
            error: None,
            observer: None,
            _subscriptions: subscriptions,
        }
    }
}

impl Workspace {
    pub(super) fn suspend_assets(&mut self) {
        self.assets.dialog = None;
        self.assets.insert_request += 1;
        self.assets.inserting = false;
    }
    pub(super) fn refresh_assets_language(&self, cx: &mut Context<Self>) {
        self.assets.search.update(cx, |input, cx| {
            input.set_placeholder(t("assets-search"));
            cx.notify();
        });
    }
    pub(crate) fn attach_library(&mut self, library: Entity<Library>, cx: &mut Context<Self>) {
        self.assets.observer = Some(cx.observe(&library, |_, _, cx| cx.notify()));
        self.assets.library = Some(library);
        cx.notify();
    }

    fn library_available(&self, cx: &gpui::App) -> bool {
        self.assets
            .library
            .as_ref()
            .is_some_and(|library| library.read(cx).ready && !library.read(cx).busy)
    }

    pub(super) fn can_save_asset(&self, cx: &gpui::App) -> bool {
        let ids = self.selection_ids();
        !ids.is_empty()
            && ids.iter().all(|id| self.layer_editable(*id))
            && self.library_available(cx)
            && !self.assets.inserting
            && !self.media_loading
            && self.image_fill_loading.is_none()
            && self.gesture.is_none()
            && self.bezier_draft.is_none()
    }

    pub(super) fn begin_save_asset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_save_asset(cx) {
            return;
        }
        self.seal_text_edits(cx);
        self.close_tool_menus(window, cx);
        self.sidebar.resources = true;
        self.sidebar.collapsed = false;
        self.assets.error = None;
        match self.component_snapshot(cx) {
            Ok(snapshot) => {
                let ids = self.selection_ids();
                let name = if ids.len() == 1 {
                    self.layer_name(*ids.first().unwrap(), cx)
                } else {
                    t("asset-default-name").into()
                };
                self.assets.dialog = Some(Dialog::Save(snapshot));
                self.focus_asset_name(name, window, cx);
            }
            Err(error) => self.assets.error = Some(format!("{error:#}")),
        }
        cx.notify();
    }

    fn focus_asset_name(&self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.assets
            .name
            .update(cx, |input, cx| input.set_value(name, cx));
        let input = self.assets.name.clone();
        input.focus_handle(cx).focus(window, cx);
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| {
                if input.focus_handle(cx).is_focused(window)
                    && let Ok(action) = cx.build_action("text_input::SelectAll", None)
                {
                    window.dispatch_action(action, cx);
                }
            })
        });
    }

    fn edit_asset(
        &mut self,
        entry: Entry,
        delete: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.library_available(cx) {
            return;
        }
        self.assets.error = None;
        if delete {
            self.assets.dialog = Some(Dialog::Delete(entry));
            self.focus.focus(window, cx);
        } else {
            let name = entry.name.clone();
            self.assets.dialog = Some(Dialog::Rename(entry));
            self.focus_asset_name(name, window, cx);
        }
        cx.notify();
    }

    pub(super) fn cancel_asset_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.assets.dialog = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn confirm_asset_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.library_available(cx) {
            return;
        }
        let name = self.assets.name.read(cx).value().trim().to_owned();
        if !matches!(self.assets.dialog, Some(Dialog::Delete(_)))
            && (name.is_empty() || name.chars().count() > 200)
        {
            return;
        }
        let Some(dialog) = self.assets.dialog.take() else {
            return;
        };
        let library = self.assets.library.clone().unwrap();
        library.update(cx, |library, cx| match dialog {
            Dialog::Save(snapshot) => {
                library.save(name, snapshot.json, snapshot.sources, snapshot.size, cx)
            }
            Dialog::Rename(entry) => library.rename(entry, name, cx),
            Dialog::Delete(entry) => library.delete(entry, cx),
        });
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn insert_asset(
        &mut self,
        entry: Entry,
        center: Option<Point<f32>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if entry.error.is_some()
            || self.assets.inserting
            || !self.library_available(cx)
            || self.assets.dialog.is_some()
        {
            return;
        }
        let bounds = self.bounds.get();
        let (left, right) = self.canvas_insets();
        let center = center.unwrap_or_else(|| {
            self.view.world(point(
                (left + f32::from(bounds.size.width) - right) / 2.,
                f32::from(bounds.size.height) / 2.,
            ))
        });
        let offset = point(center.x - entry.size[0] / 2., center.y - entry.size[1] / 2.);
        self.assets.insert_request += 1;
        let request = self.assets.insert_request;
        self.assets.inserting = true;
        self.assets.error = None;
        self.focus.focus(window, cx);
        let library = self.assets.library.clone().unwrap();
        cx.spawn_in(window, async move |this, cx| {
            let id = entry.id;
            let name = entry.name;
            let result = cx
                .background_executor()
                .spawn(async move { crate::document::load(&entry.path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if request != this.assets.insert_request {
                    return;
                }
                this.assets.inserting = false;
                match result.and_then(crate::document::Loaded::into_document) {
                    Ok(document) => {
                        this.insert_component_document(document, name, offset, window, cx)
                    }
                    Err(error) => library.update(cx, |library, cx| {
                        library.mark_failed(&id, format!("{error:#}"), cx);
                    }),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn drop_asset(
        &mut self,
        drag: &AssetDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.assets.library.as_ref() != Some(&drag.library) {
            return;
        }
        let bounds = self.bounds.get();
        let position = window.mouse_position();
        if !bounds.contains(&position) {
            return;
        }
        let x = f32::from(position.x - bounds.left());
        let y = f32::from(position.y - bounds.top());
        let (left, right) = self.canvas_insets();
        if x < left
            || x > f32::from(bounds.size.width) - right
            || y < 56.
            || y > f32::from(bounds.size.height) - 76.
        {
            return;
        }
        self.insert_asset(
            drag.entry.clone(),
            Some(self.view.world(point(x, y))),
            window,
            cx,
        );
        cx.stop_propagation();
    }
}

#[derive(Clone)]
pub(super) struct AssetDrag {
    entry: Entry,
    library: Entity<Library>,
}
impl Render for AssetDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(112.))
            .p(px(7.))
            .rounded(px(10.))
            .bg(rgb(0x282630))
            .border_1()
            .border_color(rgb(ACCENT))
            .shadow_lg()
            .child(view::thumbnail(&self.entry, 76.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT))
                    .truncate()
                    .child(self.entry.name.clone()),
            )
    }
}
