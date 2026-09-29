use super::tabs::{DragTab, SpringSlot, slot_left};
use super::*;

fn find_window(id: u64, cx: &gpui::App) -> Option<gpui::WindowHandle<Studio>> {
    cx.windows()
        .into_iter()
        .find(|window| window.window_id().as_u64() == id)?
        .downcast::<Studio>()
}

impl Studio {
    pub(super) fn receive_tab(
        &mut self,
        payload: &DragTab,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = payload.transaction.borrow().owner;
        if let Some((owner, token)) = owner {
            if owner == self.window_id {
                self.select_tab(Some(token), window, cx);
                self.dragging = None;
                self.strip.snap_index = None;
                self.strip.snap_left = None;
                self.persist_session();
                return;
            }
            if let Some(source) = find_window(owner, cx) {
                let _ = source.update(cx, |source, window, cx| {
                    source.detach_owned_tab(payload, window, cx)
                });
            }
        }
        let is_source = payload.window.window_id().as_u64() == self.window_id;
        let prepared = (|| -> anyhow::Result<Option<Entity<Workspace>>> {
            let transaction = payload.transaction.borrow();
            let tab = transaction
                .detached
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing dragged document"))?;
            let Some(editor) = &tab.editor else {
                return Ok(None);
            };
            if is_source {
                return Ok(Some(editor.clone()));
            }
            let transfer = editor.read(cx).transfer(&tab.document_id, cx)?;
            let editor = cx.new(|cx| Workspace::new(window, cx));
            editor.update(cx, |editor, cx| {
                editor.attach_library(self.library.clone(), cx)
            });
            editor.update(cx, |editor, cx| {
                editor.receive_transfer(transfer, window, cx)
            })?;
            Ok(Some(editor))
        })();
        let editor = match prepared {
            Ok(editor) => editor,
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let Some(mut tab) = payload.transaction.borrow_mut().detached.take() else {
            return;
        };
        if let Some(current) = self.active_editor() {
            current.update(cx, |editor, cx| editor.suspend(window, cx));
        }
        if !is_source {
            tab.token = self.next_token;
            self.next_token += 1;
        }
        tab.editor = editor;
        tab._subscription = tab
            .editor
            .as_ref()
            .map(|editor| cx.observe(editor, |_, _, cx| cx.notify()));
        let token = tab.token;
        let index = index.unwrap_or(self.tabs.len()).min(self.tabs.len());
        let width = self.tab_width(f32::from(window.viewport_size().width), self.tabs.len() + 1);
        self.strip
            .slots
            .insert(token, SpringSlot::settled(slot_left(index, width)));
        self.tabs.insert(index, tab);
        payload.transaction.borrow_mut().owner = Some((self.window_id, token));
        self.dragging = None;
        self.strip.snap_index = None;
        self.strip.snap_left = None;
        self.select_tab(Some(token), window, cx);
        self.persist_session();
        window.activate_window();
        cx.notify();
    }

    pub(super) fn open_detached_tab(
        payload: &DragTab,
        origin: Option<gpui::Point<gpui::Pixels>>,
        cx: &mut gpui::App,
    ) -> bool {
        let owner = payload.transaction.borrow().owner;
        if let Some((owner, _)) = owner
            && let Some(source) = find_window(owner, cx)
        {
            let _ = source.update(cx, |source, window, cx| {
                source.detach_owned_tab(payload, window, cx)
            });
        }
        if payload.transaction.borrow().detached.is_none() {
            return false;
        }
        let Ok((directory, session)) = payload.source.read_with(cx, |source, _| {
            (source.directory.clone(), source.session.clone())
        }) else {
            return false;
        };
        let mut options = crate::window_options(cx);
        if let Some(origin) = origin {
            options.window_bounds = Some(gpui::WindowBounds::Windowed(gpui::Bounds::new(
                origin,
                gpui::size(px(1280.), px(800.)),
            )));
        }
        let result = cx.open_window(options, |window, cx| {
            cx.new(|cx| {
                let mut studio = Self::new(directory, window, cx);
                studio.tabs.clear();
                studio.session = session;
                studio.receive_tab(payload, None, window, cx);
                studio
            })
        });
        match result {
            Ok(handle) if payload.transaction.borrow().owner.is_none() => {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
                false
            }
            Ok(_) => true,
            Err(error) => {
                let _ = payload.source.update(cx, |source, cx| {
                    source.error = Some(error.to_string());
                    cx.notify();
                });
                false
            }
        }
    }

    fn restore_drag_source(payload: &DragTab, cx: &mut gpui::App) -> bool {
        payload
            .window
            .update(cx, |source, window, cx| {
                let (index, selected) = {
                    let tx = payload.transaction.borrow();
                    (tx.origin_index, tx.origin_selected)
                };
                let owned = source
                    .tabs
                    .iter()
                    .position(|tab| tab.token == payload.token)
                    .map(|index| source.tabs.remove(index));
                let tab = owned.or_else(|| payload.transaction.borrow_mut().detached.take());
                let Some(tab) = tab else {
                    return false;
                };
                let token = tab.token;
                let index = index.min(source.tabs.len());
                let width = source.tab_width(
                    f32::from(window.viewport_size().width),
                    source.tabs.len() + 1,
                );
                source
                    .strip
                    .slots
                    .insert(token, SpringSlot::settled(slot_left(index, width)));
                source.tabs.insert(index, tab);
                payload.transaction.borrow_mut().owner = Some((source.window_id, token));
                let selected = selected.map(|selected| {
                    if source.tabs.iter().any(|tab| tab.token == selected) {
                        selected
                    } else {
                        token
                    }
                });
                source.select_tab(selected, window, cx);
                source.persist_session();
                cx.notify();
                true
            })
            .unwrap_or(false)
    }

    pub(super) fn finish_tab_drag(payload: DragTab, event: gpui::DragEnd, cx: &mut gpui::App) {
        let native = payload.transaction.borrow().native;
        let finished = match event {
            gpui::DragEnd::Dropped { .. } => payload.transaction.borrow().owner.is_some(),
            gpui::DragEnd::Unaccepted => Self::open_detached_tab(&payload, None, cx),
            gpui::DragEnd::Cancelled if native => Self::open_detached_tab(&payload, None, cx),
            gpui::DragEnd::Cancelled
            | gpui::DragEnd::Failed(_)
            | gpui::DragEnd::ExternalDropped { .. } => Self::restore_drag_source(&payload, cx),
        };
        if !finished {
            Self::restore_drag_source(&payload, cx);
        }
        for handle in cx
            .windows()
            .into_iter()
            .filter_map(|handle| handle.downcast::<Studio>())
        {
            let _ = handle.update(cx, |studio, _, cx| {
                if studio
                    .strip
                    .drag
                    .as_ref()
                    .is_some_and(|drag| drag.same(&payload))
                {
                    studio.strip.drag = None;
                    studio.strip.snap_index = None;
                    studio.strip.snap_left = None;
                    studio.dragging = None;
                    studio.persist_session();
                    cx.notify();
                }
            });
        }
        let source = payload.window;
        if payload
            .transaction
            .borrow()
            .owner
            .is_some_and(|(owner, _)| owner != source.window_id().as_u64())
        {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(32))
                    .await;
                let _ = source.update(cx, |studio, window, _| {
                    if studio.tabs.is_empty() {
                        studio.persist_session();
                        window.remove_window();
                    }
                });
            })
            .detach();
        }
    }
}
