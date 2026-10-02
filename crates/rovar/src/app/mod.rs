//! Application windows, document tabs, session lifecycle, and account UI.

mod comparison;
mod export;
mod files;
mod home;
mod language;
mod loading;
mod menu;
#[cfg(target_os = "macos")]
pub(crate) mod native_menu;
mod open_error;
mod preferences;
#[cfg(test)]
mod record_tests;
mod records;
mod servers;
mod sorting;
mod source_menu;
mod spaces;
mod startup;
#[cfg(test)]
mod storage_tests;
mod tabs;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod transfer_tests;
mod window_controls;
mod windows;

pub(crate) use startup::start;

use crate::platform::now;
use crate::ui::{ACCENT, BORDER, MUTED, PANEL, TEXT, icon};
use crate::{editor::Workspace, i18n::t};
use gpui::{
    AppContext, Context, Entity, FocusHandle, IntoElement, Render, Subscription, Task, Window, div,
    prelude::*, px, rgb,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use uic::{
    assets::LucideIcons,
    components::{
        dropdown::DropdownState,
        input::{InputEvent, TextInput},
    },
};
use web_time::Duration;

#[derive(Clone, Serialize, Deserialize)]
struct Recent {
    path: PathBuf,
    title: String,
    created: u64,
    modified: u64,
    #[serde(default)]
    views: crate::editor::PageViews,
    preview: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Session {
    recent: Vec<Recent>,
    #[serde(skip)]
    removed_recent: std::collections::BTreeSet<PathBuf>,
    open: Vec<PathBuf>,
    #[serde(skip)]
    windows: std::collections::BTreeMap<u64, Vec<PathBuf>>,
}

pub(crate) struct Tab {
    token: usize,
    document_id: String,
    file: Recent,
    editor: Option<Entity<Workspace>>,
    last_saved: Vec<u8>,
    saved_revision: Option<u64>,
    needs_upgrade: bool,
    loading: bool,
    saving: bool,
    close_after_save: bool,
    exporting: bool,
    save_requested: bool,
    error: Option<String>,
    _subscription: Option<Subscription>,
}

pub(crate) struct Studio {
    remote: Entity<crate::remote::Remote>,
    _remote_subscription: Subscription,
    source: Option<String>,
    servers: Option<servers::Panel>,
    comparison: Option<comparison::Panel>,
    spaces: Option<spaces::Panel>,
    signing_out: bool,
    source_menu: Entity<DropdownState>,
    server_info: Entity<DropdownState>,
    library: Entity<crate::document::library::Library>,
    _library_subscription: Subscription,
    awaiting_library: bool,
    tabs: Vec<Tab>,
    active: Option<usize>,
    next_token: usize,
    directory: PathBuf,
    session: Rc<RefCell<Session>>,
    window_id: u64,
    chrome: crate::ui::titlebar::Chrome,
    dragging: Option<usize>,
    tab_scroll: gpui::ScrollHandle,
    strip: tabs::TabStrip,
    search: Entity<TextInput>,
    rename_input: Entity<TextInput>,
    renaming: Option<PathBuf>,
    deleting_document: Option<PathBuf>,
    open_errors: std::collections::VecDeque<open_error::OpenError>,
    _rename_subscriptions: Vec<Subscription>,
    all_files: bool,
    home_page: usize,
    home_scroll: gpui::ScrollHandle,
    file_sort: sorting::FileSort,
    sort_menu: Entity<DropdownState>,
    menu: Entity<DropdownState>,
    language_menu: Entity<DropdownState>,
    preferences: Option<preferences::Panel>,
    focus: FocusHandle,
    error: Option<String>,
    closing: bool,
    quit_requested: bool,
    _timer: Task<()>,
    _search_subscription: Subscription,
}

impl Studio {
    pub(crate) fn new(directory: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        window.set_window_title("Rovar");
        let directory = rovar_storage::fs::canonicalize(&directory).unwrap_or(directory);
        let remote = crate::remote::Remote::shared(&directory, cx);
        let remote_subscription = cx.observe(&remote, |this, _, cx| {
            this.update_remote_catalog(cx);
        });
        let library = crate::document::library::Library::open(&directory, cx);
        let library_subscription = cx.observe_in(&library, window, |this, _, window, cx| {
            if this.closing {
                this.autosave(window, cx);
            }
            cx.notify();
        });
        let session_path = directory.join("session.json");
        let (mut session, error) = match rovar_storage::fs::read(&session_path) {
            Ok(bytes) => match serde_json::from_slice::<Session>(&bytes) {
                Ok(session) => (session, None),
                Err(error) => (Session::default(), Some(error.to_string())),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (Session::default(), None)
            }
            Err(error) => (Session::default(), Some(error.to_string())),
        };
        session.removed_recent = records::removed_recent(&directory);
        session
            .recent
            .retain(|file| files::is_internal(&directory, &file.path));
        session
            .open
            .retain(|path| files::is_internal(&directory, path));
        let search = cx.new(|cx| TextInput::new(cx).placeholder(t("home-search")));
        let search_subscription = cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change(_)) {
                this.set_home_page(0, cx);
            }
        });
        let rename_input = cx.new(TextInput::new);
        let rename_subscriptions = vec![
            cx.subscribe_in(
                &rename_input,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Submit(_)) {
                        this.finish_document_rename(true, window, cx);
                    }
                },
            ),
            cx.observe(&rename_input, |_, _, cx| cx.notify()),
        ];
        let recovery_directory = directory.clone();
        cx.spawn_in(window, async move |this, cx| {
            let files = cx
                .background_executor()
                .spawn(async move { files::recover_documents(recovery_directory) })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                let mut changed = false;
                for mut file in files {
                    if !rovar_storage::exists(&file.path) {
                        continue;
                    }
                    let known = {
                        let mut session = this.session.borrow_mut();
                        if let Some(item) = session
                            .recent
                            .iter_mut()
                            .find(|item| item.path == file.path)
                        {
                            if item.preview != file.preview {
                                item.preview = file.preview.clone();
                                changed = true;
                            }
                            true
                        } else {
                            false
                        }
                    };
                    if !known {
                        file.title = t("recovered-document").into();
                        this.remember(file);
                        changed = true;
                    }
                }
                if changed {
                    this.persist_session();
                    cx.notify();
                }
            });
        })
        .detach();
        let timer = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(1000))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| this.autosave(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut tabs = Vec::new();
        for path in &session.open {
            if let Some(file) = session.recent.iter().find(|file| &file.path == path) {
                tabs.push(Tab {
                    token: tabs.len() + 1,
                    document_id: String::new(),
                    file: file.clone(),
                    editor: None,
                    last_saved: Vec::new(),
                    saved_revision: None,
                    needs_upgrade: false,
                    loading: false,
                    saving: false,
                    close_after_save: false,
                    exporting: false,
                    save_requested: false,
                    error: None,
                    _subscription: None,
                });
            }
        }
        let next_token = tabs.len() + 1;
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                if this.closing
                    && !this.library.read(cx).busy
                    && !this.awaiting_library
                    && this
                        .tabs
                        .iter()
                        .all(|tab| !tab.saving && !tab.exporting && tab.editor.is_none())
                {
                    return true;
                }
                this.begin_close(window, cx);
                false
            })
            .unwrap_or(true)
        });
        let window_id = window.window_handle().window_id().as_u64();
        let weak = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |this, cx| this.initialize_servers(window, cx));
        });
        session.windows.insert(
            window_id,
            tabs.iter().map(|tab| tab.file.path.clone()).collect(),
        );
        Self {
            remote,
            _remote_subscription: remote_subscription,
            source: None,
            servers: None,
            comparison: None,
            spaces: None,
            signing_out: false,
            source_menu: cx.new(|cx| DropdownState::new(window, cx)),
            server_info: cx.new(|cx| DropdownState::new(window, cx)),
            library,
            _library_subscription: library_subscription,
            awaiting_library: false,
            tabs,
            active: None,
            next_token,
            directory,
            session: Rc::new(RefCell::new(session)),
            window_id,
            chrome: crate::ui::titlebar::Chrome::current(cx),
            dragging: None,
            tab_scroll: gpui::ScrollHandle::new(),
            strip: Default::default(),
            search,
            rename_input,
            renaming: None,
            deleting_document: None,
            open_errors: Default::default(),
            _rename_subscriptions: rename_subscriptions,
            all_files: false,
            home_page: 0,
            home_scroll: gpui::ScrollHandle::new(),
            file_sort: Default::default(),
            sort_menu: cx.new(|cx| DropdownState::new(window, cx)),
            menu: cx.new(|cx| DropdownState::new(window, cx)),
            language_menu: cx.new(|cx| DropdownState::new(window, cx)),
            preferences: None,
            focus,
            error,
            closing: false,
            quit_requested: false,
            _timer: timer,
            _search_subscription: search_subscription,
        }
    }

    pub(crate) fn active_editor(&self) -> Option<Entity<Workspace>> {
        self.tabs
            .iter()
            .find(|tab| Some(tab.token) == self.active)
            .and_then(|tab| tab.editor.clone())
    }

    fn select_tab(&mut self, token: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        self.server_info
            .update(cx, |state, cx| state.close(window, cx));
        if self.remote.read(cx).busy
            && self.tabs.iter().any(|tab| {
                Some(tab.token) == token
                    && tab.editor.is_none()
                    && self.remote.read(cx).link(&tab.file.path).is_some()
            })
        {
            return;
        }
        self.dismiss_tab_preview(cx);
        if self.active == token {
            if let Some(token) = token {
                self.load_tab(token, window, cx);
            }
            return;
        }
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.suspend(window, cx));
        }
        self.active = token;
        if let Some(index) = self.tabs.iter().position(|tab| Some(tab.token) == token) {
            self.reveal_tab(index, window);
        }
        self.focus.focus(window, cx);
        if let Some(token) = token {
            self.load_tab(token, window, cx);
            if let Some(editor) = self.active_editor() {
                editor.update(cx, |editor, cx| editor.focus_canvas(window, cx));
            }
        }
        cx.notify();
    }

    fn new_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_create_document(window, cx) {
            return;
        }
        self.dismiss_tab_preview(cx);
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.suspend(window, cx));
        }
        let token = self.next_token;
        self.next_token += 1;
        let document_id = uuid::Uuid::new_v4().to_string();
        let editor = cx.new(|cx| Workspace::new(window, cx));
        let library = self.source_library(self.source.clone(), cx);
        editor.update(cx, |editor, cx| editor.attach_library(library, cx));
        let subscription = cx.observe(&editor, |_, _, cx| cx.notify());
        let file = Recent {
            path: self
                .directory
                .join("documents")
                .join(format!("{document_id}.rovar")),
            title: crate::i18n::message("untitled-name", &[("id", token.to_string())]),
            created: sorting::creation_time(),
            modified: now(),
            views: Default::default(),
            preview: None,
        };
        if let Some(connection) = &self.source {
            self.remote.update(cx, |remote, cx| {
                remote.track(
                    file.path.clone(),
                    connection.clone(),
                    file.title.clone(),
                    rovar_api::Kind::Document,
                    cx,
                )
            });
        }
        self.tabs.push(Tab {
            token,
            document_id,
            file,
            editor: Some(editor.clone()),
            last_saved: Vec::new(),
            saved_revision: None,
            needs_upgrade: false,
            loading: false,
            saving: false,
            close_after_save: false,
            exporting: false,
            save_requested: false,
            error: None,
            _subscription: Some(subscription),
        });
        self.active = Some(token);
        self.reveal_tab(self.tabs.len() - 1, window);
        editor.update(cx, |editor, cx| editor.focus_canvas(window, cx));
        self.save_tab(token, window, cx);
        cx.notify();
    }
}

impl Render for Studio {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(target_family = "wasm")]
        crate::web::set_unsaved(self.tabs.iter().any(|tab| {
            tab.saving
                || tab.editor.as_ref().is_some_and(|editor| {
                    tab.saved_revision != Some(editor.read(cx).document_revision())
                })
        }));
        #[cfg(target_family = "wasm")]
        if self.source.is_none() || self.signing_out {
            if self.servers.is_some() && !self.signing_out {
                return self.web_login_page(cx).into_any_element();
            }
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(PANEL))
                .text_color(rgb(MUTED))
                .child(
                    self.error
                        .clone()
                        .unwrap_or_else(|| t("server-connecting").into()),
                )
                .into_any_element();
        }
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(PANEL))
            .text_color(rgb(TEXT))
            .font_family(crate::ui::font::family(cx))
            .on_drag_move::<tabs::DragTab>(cx.listener(Self::tab_drag_moved))
            .on_mouse_exit(cx.listener(Self::tab_drag_exited))
            .on_drop(cx.listener(Self::dropped_as_window))
            .track_focus(&self.focus)
            .on_any_mouse_down(cx.listener(|this, _, window, cx| {
                this.dismiss_tab_preview(cx);
                if let Some(editor) = this.active_editor() {
                    editor.update(cx, |editor, cx| editor.dismiss_menus(window, cx));
                }
            }))
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if let Some(panel) = &this.comparison {
                    if event.keystroke.key == "escape" {
                        this.close_comparison(window, cx);
                        cx.stop_propagation();
                        window.prevent_default();
                    } else if panel.server_visible {
                        if let Some(editor) = panel.editor() {
                            editor.update(cx, |editor, cx| editor.preview_key(event, cx));
                        }
                        cx.stop_propagation();
                        window.prevent_default();
                    }
                    return;
                }
                if !this.open_errors.is_empty() {
                    if matches!(event.keystroke.key.as_str(), "escape" | "enter") {
                        this.dismiss_open_error(window, cx);
                    }
                    cx.stop_propagation();
                    window.prevent_default();
                    return;
                }
                if this.deleting_document.is_some() {
                    if event.keystroke.key == "escape" {
                        this.finish_document_delete(false, window, cx);
                    }
                    cx.stop_propagation();
                    window.prevent_default();
                    return;
                }
                if this.renaming.is_some() {
                    if event.keystroke.key == "escape" {
                        this.finish_document_rename(false, window, cx);
                        cx.stop_propagation();
                        window.prevent_default();
                    }
                    return;
                }
                if this.preferences.is_some() {
                    if event.keystroke.key == "escape" {
                        this.settings_escape(window, cx);
                        cx.stop_propagation();
                        window.prevent_default();
                    }
                    return;
                }
                if this.spaces.is_some() {
                    if event.keystroke.key == "escape" {
                        this.spaces = None;
                        this.focus.focus(window, cx);
                        cx.notify();
                    }
                    return;
                }
                if this.servers.is_some() {
                    if event.keystroke.key == "escape" {
                        this.servers = None;
                        this.focus.focus(window, cx);
                        cx.notify();
                    }
                    return;
                }
                if uic::components::context_menu::is_open(cx) {
                    return;
                }
                let modifiers = event.keystroke.modifiers;
                if !(modifiers.control || modifiers.platform) {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "," => this.open_settings(window, cx),
                    "n" | "t" => this.new_document(window, cx),
                    "o" => this.open_dialog(window, cx),
                    "s" => this.save_command(window, cx),
                    "e" if modifiers.shift => this.export_dialog(window, cx),
                    "w" => {
                        if let Some(token) = this.active {
                            this.close_tab(token, window, cx);
                        } else if cfg!(target_os = "macos") {
                            this.begin_close(window, cx);
                        }
                    }
                    "tab" => {
                        if !this.tabs.is_empty() {
                            let index = this
                                .tabs
                                .iter()
                                .position(|t| Some(t.token) == this.active)
                                .unwrap_or(if modifiers.shift {
                                    0
                                } else {
                                    this.tabs.len() - 1
                                });
                            let offset = if modifiers.shift {
                                this.tabs.len() - 1
                            } else {
                                1
                            };
                            this.select_tab(
                                Some(this.tabs[(index + offset) % this.tabs.len()].token),
                                window,
                                cx,
                            );
                        }
                    }
                    _ => return,
                }
                cx.stop_propagation();
                window.prevent_default();
            }))
            .child(self.tab_bar(window, cx))
            .when_some(self.remote_status(cx), |el, status| {
                el.child(
                    div()
                        .px(px(24.))
                        .py(px(6.))
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(div().flex_1().child(status))
                        .when_some(self.active_remote_conflict(cx), |el, path| {
                            el.child(
                                div()
                                    .id("server-save-copy")
                                    .cursor_pointer()
                                    .text_color(rgb(ACCENT))
                                    .child(t("compare-versions"))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_comparison(&path, window, cx);
                                    })),
                            )
                        }),
                )
            })
            .when_some(self.error.clone(), |el, error| {
                el.child(
                    div()
                        .px(px(24.))
                        .py(px(8.))
                        .bg(rgb(0x4a3034))
                        .text_size(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(div().flex_1().min_w_0().child(error))
                        .child(
                            div()
                                .id("dismiss-error")
                                .size(px(22.))
                                .flex_shrink_0()
                                .cursor_pointer()
                                .child(icon(LucideIcons::X, 14.))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.error = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when_some(
                        self.active_editor().filter(|_| self.comparison.is_none()),
                        |el, editor| el.child(editor),
                    )
                    .when(self.active.is_none(), |el| el.child(self.home(window, cx)))
                    .when(
                        self.active.is_some() && self.active_editor().is_none(),
                        |el| {
                            el.child(
                                div()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(MUTED))
                                    .child(loading::view(self.active.unwrap())),
                            )
                        },
                    ),
            )
            .when(self.active_editor().is_none(), |el| {
                el.child(uic::components::context_menu::layer(cx))
            })
            .children(self.tab_preview(window, cx))
            .when(self.renaming.is_some(), |el| {
                el.child(self.rename_dialog(cx))
            })
            .when(self.preferences.is_some(), |el| {
                el.child(self.settings_dialog(cx))
            })
            .when(self.spaces.is_some(), |el| el.child(self.spaces_dialog(cx)))
            .when(self.servers.is_some(), |el| {
                el.child(self.server_dialog(cx))
            })
            .when(self.deleting_document.is_some(), |el| {
                el.child(self.document_delete_dialog(cx))
            })
            .when(!self.open_errors.is_empty(), |el| {
                el.child(self.open_error_dialog(cx))
            })
            .when(self.comparison.is_some(), |el| {
                el.child(self.comparison_view(window, cx))
            })
            .map(|el| {
                #[cfg(target_os = "macos")]
                let el = self.native_menu_actions(el, cx);
                #[cfg(target_os = "linux")]
                let el = el.when(
                    self.chrome.mode != uic::desktop::TitleBarMode::System
                        && !window.is_maximized()
                        && !window.is_fullscreen(),
                    |el| el.children(crate::ui::titlebar::resize_edges()),
                );
                el
            })
            .into_any_element()
    }
}
