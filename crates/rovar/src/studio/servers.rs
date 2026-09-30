use super::*;
use crate::remote::Client;
use gpui::{Focusable, SharedString, rgba};
mod account;
mod authentication;
#[cfg(any(target_family = "wasm", test))]
mod login;
mod status;
#[cfg(test)]
mod tests;
mod view;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Servers,
    Accounts,
    Login,
    Rename,
    Settings,
}

pub(super) struct Panel {
    id: uuid::Uuid,
    view: View,
    selected: Option<String>,
    url: Entity<TextInput>,
    name: Entity<TextInput>,
    username: Entity<TextInput>,
    password: Entity<TextInput>,
    new_password: Entity<TextInput>,
    confirm_password: Entity<TextInput>,
    account: Option<String>,
    sessions: Vec<rovar_api::AccountSession>,
    success: Option<String>,
    resume: Option<String>,
    team_name: Entity<TextInput>,
    registration: Option<rovar_api::RegistrationPolicy>,
    mode: &'static str,
    busy: bool,
    error: Option<String>,
    #[cfg(any(target_family = "wasm", test))]
    motion: Rc<RefCell<login::silk::Motion>>,
    _subscriptions: Vec<Subscription>,
}

impl Panel {
    pub(super) fn refresh_language(&self, cx: &mut gpui::App) {
        for (input, key) in [
            (&self.username, "server-username"),
            (&self.password, "server-password"),
            (&self.new_password, "account-new-password"),
            (&self.confirm_password, "account-confirm-password"),
            (&self.team_name, "space-team-name"),
        ] {
            input.update(cx, |input, cx| {
                input.set_placeholder(t(key));
                cx.notify();
            });
        }
    }
}

impl Studio {
    pub(super) fn initialize_servers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.update_remote_catalog(cx);
        #[cfg(target_family = "wasm")]
        {
            self.initialize_web_login(window, cx);
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let connections = self.remote.read(cx).connections().to_vec();
            for connection in connections {
                let credentials = cx.read_credentials(&format!("rovar-server/{}", connection.id));
                cx.spawn(async move |this, cx| {
                    if let Ok(Some((_, token))) = credentials.await {
                        let _ = this.update(cx, |this, cx| {
                            this.remote.update(cx, |remote, cx| {
                                remote.restore_credentials(
                                    &connection.id,
                                    String::from_utf8_lossy(&token).into_owned(),
                                    cx,
                                );
                            });
                        });
                    }
                })
                .detach();
            }
            let _ = window;
        }
    }

    pub(super) fn source_library(
        &self,
        source: Option<String>,
        cx: &mut Context<Self>,
    ) -> Entity<crate::component_library::Library> {
        if let Some(id) = source {
            let root = self.remote.read(cx).library_root(&id);
            crate::component_library::Library::open(&root, cx)
        } else {
            self.library.clone()
        }
    }

    pub(super) fn can_create_document(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        !(cfg!(target_family = "wasm") && self.source.is_none())
    }

    pub(super) fn source_matches(&self, path: &std::path::Path, cx: &gpui::App) -> bool {
        if cfg!(target_family = "wasm") && self.source.is_none() {
            return false;
        }
        let link = self.remote.read(cx).link(path);
        link.is_none_or(|link| !link.object.deleted)
            && link.map(|link| &link.connection) == self.source.as_ref()
    }

    pub(super) fn update_remote_catalog(&mut self, cx: &mut Context<Self>) {
        let links = self.remote.read(cx).links().clone();
        let mut session = self.session.borrow_mut();
        for (path, link) in links {
            if link.object.kind != rovar_api::Kind::Document {
                continue;
            }
            if link.object.deleted {
                session.recent.retain(|file| file.path != path);
                continue;
            }
            if let Some(file) = session.recent.iter_mut().find(|file| file.path == path) {
                file.title = link.object.title.clone();
                file.created = link.object.created * 1000;
                file.modified = link.object.modified;
            } else if rovar_storage::exists(&path) {
                session.recent.push(Recent {
                    path,
                    title: link.object.title,
                    created: link.object.created * 1000,
                    modified: link.object.modified,
                    views: Default::default(),
                    preview: None,
                });
            }
        }
        drop(session);
        let libraries = self
            .remote
            .update(cx, |remote, _| remote.take_library_updates());
        for id in libraries {
            let library = self.source_library(Some(id), cx);
            library.update(cx, |library, cx| library.refresh(cx));
        }
        self.persist_session();
        cx.notify();
    }

    pub(super) fn select_source(
        &mut self,
        source: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_family = "wasm")]
        self.tabs.retain(|tab| {
            let remote = self.remote.read(cx);
            let current = source.as_ref().and_then(|id| remote.connection(id));
            let linked = remote
                .link(&tab.file.path)
                .and_then(|link| remote.connection(&link.connection));
            current.zip(linked).is_some_and(|(a, b)| {
                a.url == b.url
                    && a.identity.server_id == b.identity.server_id
                    && a.identity.user_id == b.identity.user_id
            })
        });
        self.source = source;
        self.source_menu
            .update(cx, |menu, cx| menu.close(window, cx));
        self.select_tab(None, window, cx);
        self.set_home_page(0, cx);
        self.refresh_server(cx);
    }

    pub(super) fn refresh_server(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.source.clone() {
            self.refresh_connection(id, cx);
        }
    }

    pub(in crate::studio) fn keep_conflict_versions(
        &mut self,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        if self.remote.read(cx).busy {
            return;
        }
        let Some(connection) = self
            .remote
            .read(cx)
            .link(path)
            .map(|link| link.connection.clone())
        else {
            return;
        };
        self.remote
            .update(cx, |remote, cx| remote.fork_conflict(path, cx));
        self.refresh_connection(connection, cx);
    }

    fn refresh_connection(&mut self, id: String, cx: &mut Context<Self>) {
        if self
            .remote
            .read(cx)
            .connection(&id)
            .is_some_and(|c| !c.authenticated)
        {
            return;
        }
        let mut open = std::collections::BTreeSet::new();
        for handle in cx
            .windows()
            .into_iter()
            .filter_map(|w| w.downcast::<Studio>())
        {
            if handle.window_id().as_u64() == self.window_id {
                open.extend(self.tabs.iter().map(|tab| tab.file.path.clone()));
            } else {
                let _ = handle.update(cx, |studio, _, _| {
                    open.extend(studio.tabs.iter().map(|tab| tab.file.path.clone()));
                });
            }
        }
        self.remote.update(cx, |remote, cx| {
            remote.retry(cx);
            remote.refresh(id, open, cx);
        });
    }

    pub(crate) fn open_document_paths(cx: &mut gpui::App) -> std::collections::BTreeSet<PathBuf> {
        let mut open = std::collections::BTreeSet::new();
        for handle in cx
            .windows()
            .into_iter()
            .filter_map(|w| w.downcast::<Studio>())
        {
            let _ = handle.update(cx, |studio, _, _| {
                open.extend(studio.tabs.iter().map(|tab| tab.file.path.clone()))
            });
        }
        open
    }

    pub(super) fn remote_status(&self, cx: &gpui::App) -> Option<String> {
        let remote = self.remote.read(cx);
        let link = self
            .tabs
            .iter()
            .find(|t| Some(t.token) == self.active)
            .and_then(|t| remote.link(&t.file.path));
        if let Some(link) = link {
            if link.conflict {
                return Some(t("server-conflict").into());
            }
            if let Some(error) = &link.error {
                return Some(format!("{} · {error}", t("server-local-copy")));
            }
            return None;
        }
        if self.active.is_none()
            && self.source.is_some()
            && let Some(error) = &remote.error
        {
            return Some(error.clone());
        }
        None
    }

    pub(super) fn active_remote_conflict(&self, cx: &gpui::App) -> Option<PathBuf> {
        self.tabs
            .iter()
            .find(|tab| Some(tab.token) == self.active)
            .filter(|tab| {
                self.remote
                    .read(cx)
                    .link(&tab.file.path)
                    .is_some_and(|link| link.conflict)
            })
            .map(|tab| tab.file.path.clone())
    }

    pub(super) fn open_servers(
        &mut self,
        url: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = url;
        let mut panel = Panel {
            id: uuid::Uuid::new_v4(),
            view: View::Servers,
            selected: None,
            url: cx.new(|cx| TextInput::new(cx).placeholder("https://design.example.com")),
            name: cx.new(|cx| TextInput::new(cx).placeholder(t("server-name-placeholder"))),
            username: cx.new(|cx| TextInput::new(cx).placeholder(t("server-username"))),
            password: cx.new(|cx| {
                TextInput::new(cx)
                    .password()
                    .placeholder(t("server-password"))
            }),
            new_password: cx.new(|cx| {
                TextInput::new(cx)
                    .password()
                    .placeholder(t("account-new-password"))
            }),
            confirm_password: cx.new(|cx| {
                TextInput::new(cx)
                    .password()
                    .placeholder(t("account-confirm-password"))
            }),
            account: None,
            sessions: Vec::new(),
            success: None,
            resume: None,
            team_name: cx.new(|cx| TextInput::new(cx).placeholder(t("space-team-name"))),
            registration: None,
            mode: "login",
            busy: false,
            error: None,
            #[cfg(any(target_family = "wasm", test))]
            motion: Default::default(),
            _subscriptions: Vec::new(),
        };
        panel._subscriptions.push(cx.subscribe_in(
            &panel.name,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Submit(_)) {
                    if this
                        .servers
                        .as_ref()
                        .is_some_and(|p| p.view == View::Rename)
                    {
                        this.save_server_name(window, cx);
                    } else {
                        this.server_address_submit(window, cx);
                    }
                }
            },
        ));
        panel._subscriptions.push(cx.subscribe_in(
            &panel.url,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Submit(_)) {
                    this.server_address_submit(window, cx);
                }
            },
        ));
        for input in [
            &panel.username,
            &panel.password,
            &panel.team_name,
            &panel.new_password,
            &panel.confirm_password,
        ] {
            panel._subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Submit(_)) {
                        if this
                            .servers
                            .as_ref()
                            .is_some_and(|p| p.view == View::Settings)
                        {
                            this.save_account_password(window, cx);
                        } else {
                            this.server_login(window, cx);
                        }
                    }
                },
            ));
        }
        panel.name.focus_handle(cx).focus(window, cx);
        self.servers = Some(panel);
        if let Some(url) = selected {
            self.select_server(url, window, cx);
        }
        cx.notify();
    }

    fn server_address_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &self.servers else { return };
        if panel.view != View::Servers || panel.busy {
            return;
        }
        let url = panel.url.read(cx).value().to_string();
        self.select_server(url, window, cx);
    }

    fn select_server(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.servers else {
            return;
        };
        if panel.busy {
            return;
        }
        let client = match Client::new(&url) {
            Ok(client) => client,
            Err(error) => {
                panel.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let request = panel.id;
        let name = self
            .remote
            .read(cx)
            .servers()
            .get(&client.url)
            .cloned()
            .unwrap_or_else(|| panel.name.read(cx).value().trim().to_owned());
        if name.is_empty() {
            panel.name.focus_handle(cx).focus(window, cx);
            return;
        }
        panel.error = None;
        panel.success = None;
        panel
            .new_password
            .update(cx, |input, cx| input.set_value("", cx));
        panel
            .confirm_password
            .update(cx, |input, cx| input.set_value("", cx));
        panel.busy = true;
        cx.spawn_in(window, async move |this, cx| {
            let result = client
                .json::<rovar_api::ServerInfo>("GET", "info", None)
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.servers.as_ref().is_some_and(|p| p.id == request) {
                    return;
                }
                let result = result.and_then(|info| {
                    anyhow::ensure!(
                        info.api_version == rovar_api::VERSION,
                        "Unsupported server API version"
                    );
                    this.remote.update(cx, |remote, cx| {
                        remote.save_server(client.url.clone(), name, cx)
                    })?;
                    Ok(info)
                });
                let panel = this.servers.as_mut().unwrap();
                panel.busy = false;
                match result {
                    Ok(info) => {
                        panel.selected = Some(client.url);
                        panel.registration = Some(info.registration);
                        panel.view = View::Accounts;
                        panel.url.update(cx, |input, cx| input.set_value("", cx));
                        panel.name.update(cx, |input, cx| input.set_value("", cx));
                        this.focus.focus(window, cx);
                    }
                    Err(error) => panel.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn server_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.servers else {
            return;
        };
        if panel.busy {
            return;
        }
        panel.view = view;
        panel.success = None;
        panel.resume = None;
        panel
            .new_password
            .update(cx, |input, cx| input.set_value("", cx));
        panel
            .confirm_password
            .update(cx, |input, cx| input.set_value("", cx));
        panel.mode = "login";
        panel.error = None;
        panel
            .password
            .update(cx, |input, cx| input.set_value("", cx));
        panel
            .username
            .update(cx, |input, cx| input.set_value("", cx));
        panel
            .team_name
            .update(cx, |input, cx| input.set_value("", cx));
        match view {
            View::Servers => {
                panel.selected = None;
                panel.registration = None;
                panel.name.focus_handle(cx).focus(window, cx);
            }
            View::Rename => panel.name.focus_handle(cx).focus(window, cx),
            View::Login => panel.username.focus_handle(cx).focus(window, cx),
            View::Accounts | View::Settings => self.focus.focus(window, cx),
        }
        cx.notify();
    }

    fn rename_server(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.servers.as_ref().is_none_or(|p| p.busy) {
            return;
        }
        let name = self.remote.read(cx).server_name(&url).to_owned();
        self.server_view(View::Rename, window, cx);
        let panel = self.servers.as_mut().unwrap();
        panel.selected = Some(url);
        panel.name.update(cx, |input, cx| input.set_value(name, cx));
    }

    fn save_server_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &self.servers else { return };
        if panel.busy || panel.view != View::Rename {
            return;
        }
        let Some(url) = panel.selected.clone() else {
            return;
        };
        let name = panel.name.read(cx).value().to_string();
        match self
            .remote
            .update(cx, |remote, cx| remote.save_server(url, name, cx))
        {
            Ok(()) => {
                self.server_view(View::Servers, window, cx);
                self.servers
                    .as_ref()
                    .unwrap()
                    .name
                    .update(cx, |input, cx| input.set_value("", cx));
            }
            Err(error) => self.servers.as_mut().unwrap().error = Some(error.to_string()),
        }
        cx.notify();
    }

    fn remove_server(&mut self, url: String, cx: &mut Context<Self>) {
        let open = cx
            .windows()
            .into_iter()
            .filter_map(|w| w.downcast::<Studio>())
            .any(|handle| {
                if handle.window_id().as_u64() == self.window_id {
                    self.tabs.iter().any(|tab| {
                        self.remote
                            .read(cx)
                            .link(&tab.file.path)
                            .and_then(|l| self.remote.read(cx).connection(&l.connection))
                            .is_some_and(|c| c.url == url)
                    })
                } else {
                    handle
                        .update(cx, |studio, _, cx| {
                            studio.tabs.iter().any(|tab| {
                                studio
                                    .remote
                                    .read(cx)
                                    .link(&tab.file.path)
                                    .and_then(|l| studio.remote.read(cx).connection(&l.connection))
                                    .is_some_and(|c| c.url == url)
                            })
                        })
                        .unwrap_or(true)
                }
            });
        let result = if open {
            Err(anyhow::anyhow!(t("server-remove-hint")))
        } else {
            self.remote
                .update(cx, |remote, cx| remote.remove_server(&url, cx))
        };
        if let Some(panel) = &mut self.servers {
            panel.error = result.err().map(|e| e.to_string());
        }
        cx.notify();
    }
}

pub(super) fn button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px(px(10.))
        .py(px(7.))
        .rounded(px(7.))
        .flex()
        .items_center()
        .gap(px(8.))
        .cursor_pointer()
        .hover(|s| s.bg(rgba(0xb4a2ee22)))
        .child(label.into())
}
