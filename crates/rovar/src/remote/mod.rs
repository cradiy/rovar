mod baseline;
mod cache;
mod cleanup;
mod colors;
mod conflict;
mod delta;
mod directory;
use directory::Directory;
mod media;
mod merge;
#[cfg(all(test, not(target_family = "wasm")))]
pub(crate) mod tests;
mod transport;
mod upload;
pub(crate) use transport::{Client, HttpError};

use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use gpui::{App, AppContext, Context, Entity, Global};
use rovar_api::{Identity, Kind, Object, Save, Snapshot};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// A server account scoped to one workspace; its ID also namespaces local caches.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Connection {
    pub id: String,
    pub url: String,
    pub identity: Identity,
    pub space: rovar_api::Space,
    #[serde(skip)]
    pub token: String,
    #[serde(skip)]
    pub authenticated: bool,
    #[serde(skip)]
    pub generation: u64,
}
impl Connection {
    pub fn client(&self) -> Client {
        Client {
            url: self.url.clone(),
            token: self.token.clone(),
        }
    }
    pub fn space_label(&self) -> String {
        if self.space.kind == "personal" {
            crate::i18n::t("space-personal").into()
        } else {
            self.space.name.clone()
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Link {
    pub connection: String,
    pub object: Object,
    pub dirty: bool,
    pub digest: String,
    #[serde(default)]
    pub baseline: Option<String>,
    pub conflict: bool,
    #[serde(skip)]
    pub error: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Catalog {
    servers: BTreeMap<String, String>,
    connections: Vec<Connection>,
    links: BTreeMap<PathBuf, Link>,
    #[serde(default)]
    directories: BTreeMap<String, Directory>,
}

#[derive(Serialize, Deserialize)]
struct PendingSave {
    #[serde(flatten)]
    input: Save,
    /// Freeze the exact patch before the first request; retries never re-diff
    /// against a cache that may have been edited or refreshed in the meantime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    delta: Option<String>,
}

pub(crate) struct Remote {
    root: PathBuf,
    catalog: Catalog,
    pub busy: bool,
    pub error: Option<String>,
    libraries_changed: BTreeSet<String>,
    retry_at: web_time::Instant,
    cleanup_at: Option<web_time::Instant>,
    baseline_cleanup: cleanup::Cleanup,
    reconnect_at: BTreeMap<String, web_time::Instant>,
    refresh_at: BTreeMap<String, web_time::Instant>,
    auth_generation: u64,
    merge_pending: BTreeSet<PathBuf>,
    local_changes: BTreeMap<PathBuf, u64>,
}
struct SharedRemote(Entity<Remote>);
impl Global for SharedRemote {}

#[cfg(target_family = "wasm")]
pub(crate) fn browser_url() -> String {
    web_sys::window()
        .unwrap()
        .location()
        .origin()
        .unwrap_or_default()
}

pub(crate) fn digest(bytes: &[u8], title: &str, deleted: bool) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hash.update(title.as_bytes());
    hash.update([u8::from(deleted)]);
    hex::encode(hash.finalize())
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap();
    rovar_storage::fs::create_dir_all(parent)?;
    let mut temp = rovar_storage::tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    rovar_format::sync_parent(path)?;
    Ok(())
}

impl Remote {
    pub(crate) fn download_failed(&self, link: &Link) -> bool {
        self.catalog
            .directories
            .get(&link.connection)
            .is_some_and(|directory| directory.failed(&link.object.id))
    }

    pub(crate) fn download_pending(&self, link: &Link) -> bool {
        self.catalog
            .directories
            .get(&link.connection)
            .is_some_and(|directory| directory.pending.contains_key(&link.object.id))
    }

    pub fn shared(root: &Path, cx: &mut App) -> Entity<Self> {
        if let Some(shared) = cx.try_global::<SharedRemote>() {
            return shared.0.clone();
        }
        let path = root.join("servers.json");
        let (catalog, error) = match rovar_storage::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(catalog) => (catalog, None),
                Err(error) => (
                    Catalog::default(),
                    Some(format!("Cannot read servers.json: {error}")),
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (Catalog::default(), None)
            }
            Err(error) => (Catalog::default(), Some(error.to_string())),
        };
        let merge_pending = catalog
            .links
            .iter()
            .filter(|(_, link)| link.conflict)
            .map(|(path, _)| path.clone())
            .collect();
        let remote = cx.new(|_| Self {
            root: root.into(),
            catalog,
            busy: false,
            error,
            libraries_changed: BTreeSet::new(),
            retry_at: web_time::Instant::now(),
            cleanup_at: None,
            baseline_cleanup: cleanup::Cleanup::default(),
            reconnect_at: BTreeMap::new(),
            refresh_at: BTreeMap::new(),
            auth_generation: 0,
            merge_pending,
            local_changes: BTreeMap::new(),
        });
        cx.set_global(SharedRemote(remote.clone()));
        remote.update(cx, |remote, cx| {
            remote.recover_incoming(cx, |_, _, _| {});
        });
        remote
    }
    pub fn connections(&self) -> &[Connection] {
        &self.catalog.connections
    }
    pub fn servers(&self) -> &BTreeMap<String, String> {
        &self.catalog.servers
    }
    pub fn server_name(&self, url: &str) -> &str {
        self.catalog
            .servers
            .get(url)
            .map(String::as_str)
            .unwrap_or("Rovar")
    }
    pub fn save_server(&mut self, url: String, name: String, cx: &mut Context<Self>) -> Result<()> {
        let name = name.trim();
        ensure!(
            !name.is_empty() && name.chars().count() <= 80,
            "Use a server name of 1–80 characters"
        );
        let previous = self.catalog.servers.insert(url.clone(), name.to_owned());
        if !self.persist() {
            if let Some(previous) = previous {
                self.catalog.servers.insert(url, previous);
            } else {
                self.catalog.servers.remove(&url);
            }
            anyhow::bail!(self.error.clone().unwrap_or_default());
        }
        self.error = None;
        cx.notify();
        Ok(())
    }
    /// Show one account per server, regardless of its team memberships.
    pub fn accounts(&self, url: &str) -> Vec<Connection> {
        let mut accounts = BTreeMap::new();
        for connection in self.catalog.connections.iter().filter(|c| c.url == url) {
            let key = (&connection.identity.server_id, &connection.identity.user_id);
            if !accounts.contains_key(&key) || connection.space.kind == "personal" {
                accounts.insert(key, connection.clone());
            }
        }
        let mut accounts = accounts.into_values().collect::<Vec<_>>();
        accounts.sort_by(|a, b| a.identity.username.cmp(&b.identity.username));
        accounts
    }
    pub fn existing(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<SharedRemote>()
            .map(|shared| shared.0.clone())
    }
    pub fn take_library_updates(&mut self) -> BTreeSet<String> {
        std::mem::take(&mut self.libraries_changed)
    }
    pub fn links(&self) -> &BTreeMap<PathBuf, Link> {
        &self.catalog.links
    }
    pub fn link(&self, path: &Path) -> Option<&Link> {
        self.catalog.links.get(path)
    }
    pub fn library_root(&self, connection: &str) -> PathBuf {
        self.root.join("servers").join(connection)
    }
    pub fn connection(&self, id: &str) -> Option<&Connection> {
        self.catalog.connections.iter().find(|c| c.id == id)
    }
    fn persist(&mut self) -> bool {
        if let Err(error) = write_atomic(
            &self.root.join("servers.json"),
            &serde_json::to_vec(&self.catalog).unwrap(),
        ) {
            self.error = Some(error.to_string());
            return false;
        }
        true
    }
    pub fn connect(
        &mut self,
        url: String,
        identity: Identity,
        token: String,
        cx: &mut Context<Self>,
    ) -> Result<String> {
        ensure!(
            identity.api_version == rovar_api::VERSION,
            "Unsupported server API version"
        );
        let mut first = None;
        // Refreshing workspace metadata must not invalidate in-flight work for
        // the same authenticated session. Logout, new tokens and role changes do.
        let sessions: BTreeMap<_, _> = self
            .catalog
            .connections
            .iter()
            .filter(|c| {
                c.authenticated
                    && c.url == url
                    && c.token == token
                    && c.identity.server_id == identity.server_id
                    && c.identity.user_id == identity.user_id
            })
            .map(|c| (c.space.id.clone(), (c.space.role.clone(), c.generation)))
            .collect();
        self.auth_generation += 1;
        for connection in &mut self.catalog.connections {
            if connection.url == url && connection.identity.user_id == identity.user_id {
                connection.authenticated = false;
                connection.token.clear();
            }
        }
        for space in &identity.spaces {
            let id = self
                .catalog
                .connections
                .iter()
                .find(|c| {
                    c.url == url
                        && c.identity.server_id == identity.server_id
                        && c.identity.user_id == identity.user_id
                        && c.space.id == space.id
                })
                .map(|c| c.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            self.catalog.connections.retain(|c| c.id != id);
            self.catalog.connections.push(Connection {
                id: id.clone(),
                url: url.clone(),
                identity: identity.clone(),
                space: space.clone(),
                token: token.clone(),
                authenticated: true,
                generation: sessions
                    .get(&space.id)
                    .filter(|(role, _)| role == &space.role)
                    .map_or(self.auth_generation, |(_, generation)| *generation),
            });
            self.refresh_at
                .entry(id.clone())
                .or_insert_with(web_time::Instant::now);
            if first.is_none() || space.kind == "personal" {
                first = Some(id);
            }
        }
        let id = first.ok_or_else(|| anyhow::anyhow!("Account has no workspace"))?;
        self.catalog
            .servers
            .entry(url)
            .or_insert_with(|| "Rovar".into());
        self.error = None;
        self.persist();
        cx.notify();
        Ok(id)
    }
    pub fn sign_out(&mut self, id: &str, cx: &mut Context<Self>) {
        self.auth_generation += 1;
        if let Some(account) = self.connection(id).cloned() {
            for c in &mut self.catalog.connections {
                if c.url == account.url && c.identity.user_id == account.identity.user_id {
                    c.token.clear();
                    c.authenticated = false;
                    c.generation = self.auth_generation;
                }
            }
        }
        cx.notify();
    }
    #[cfg(not(target_family = "wasm"))]
    pub fn restore_credentials(&mut self, id: &str, token: String, cx: &mut Context<Self>) {
        if let Some(connection) = self
            .catalog
            .connections
            .iter_mut()
            .find(|c| c.id == id && !c.authenticated)
        {
            connection.token = token;
            self.reconnect_at.remove(id);
            cx.notify();
        }
    }

    fn reconnect(&mut self, cx: &mut Context<Self>) -> bool {
        let candidate = self
            .catalog
            .connections
            .iter()
            .find(|c| {
                !c.authenticated
                    && !c.token.is_empty()
                    && self.catalog.servers.contains_key(&c.url)
                    && self
                        .reconnect_at
                        .get(&c.id)
                        .is_none_or(|at| at.elapsed().as_secs() >= 30)
            })
            .cloned();
        let Some(connection) = candidate else {
            return false;
        };
        self.reconnect_at
            .insert(connection.id.clone(), web_time::Instant::now());
        self.busy = true;
        cx.spawn(async move |this, cx| {
            let result = connection
                .client()
                .json::<Identity>("GET", "session", None)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if this.connection(&connection.id).is_none_or(|c| {
                    c.token != connection.token || c.generation != connection.generation
                }) {
                    return;
                }
                match result {
                    Ok(identity)
                        if identity.server_id == connection.identity.server_id
                            && identity.user_id == connection.identity.user_id =>
                    {
                        let _ = this.connect(connection.url, identity, connection.token, cx);
                        this.retry(cx);
                    }
                    Ok(_) => this.sign_out(&connection.id, cx),
                    Err(error)
                        if error
                            .downcast_ref::<HttpError>()
                            .is_some_and(|e| e.status == 401) =>
                    {
                        this.sign_out(&connection.id, cx)
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        true
    }

    pub fn remove_server(&mut self, url: &str, cx: &mut Context<Self>) -> Result<()> {
        ensure!(
            !self.busy
                && !self
                    .catalog
                    .connections
                    .iter()
                    .any(|c| c.url == url && c.authenticated),
            "{}",
            crate::i18n::t("server-remove-hint")
        );
        ensure!(
            !self
                .catalog
                .links
                .values()
                .any(|l| l.dirty && self.connection(&l.connection).is_some_and(|c| c.url == url)),
            "{}",
            crate::i18n::t("server-remove-hint")
        );
        let previous = self.catalog.servers.remove(url);
        if !self.persist() {
            if let Some(name) = previous {
                self.catalog.servers.insert(url.into(), name);
            }
            anyhow::bail!(self.error.clone().unwrap_or_default());
        }
        // Keep document caches and their identity mapping for a future reconnect.
        cx.notify();
        Ok(())
    }
    pub fn track(
        &mut self,
        path: PathBuf,
        connection: String,
        title: String,
        kind: Kind,
        cx: &mut Context<Self>,
    ) {
        self.catalog.links.entry(path).or_insert_with(|| Link {
            connection,
            object: Object {
                id: uuid::Uuid::new_v4().to_string(),
                kind,
                title,
                revision: 0,
                created: crate::platform::now(),
                modified: crate::platform::now(),
                deleted: false,
            },
            dirty: true,
            digest: String::new(),
            baseline: None,
            conflict: false,
            error: None,
        });
        self.persist();
        cx.notify();
    }
    pub fn changed(
        &mut self,
        path: &Path,
        title: Option<String>,
        deleted: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(link) = self.catalog.links.get_mut(path) {
            if let Some(title) = title {
                link.object.title = title;
            }
            link.object.deleted = deleted;
            link.dirty = true;
            *self.local_changes.entry(path.to_owned()).or_default() += 1;
            if self
                .catalog
                .links
                .get(path)
                .is_some_and(|link| link.conflict)
            {
                self.merge_pending.insert(path.to_path_buf());
            }
            self.persist();
            cx.notify();
        }
    }
    pub fn retry(&mut self, cx: &mut Context<Self>) {
        self.reconnect_at.clear();
        for directory in self.catalog.directories.values_mut() {
            directory.retry_now();
        }
        self.merge_pending.extend(
            self.catalog
                .links
                .iter()
                .filter(|(_, link)| link.conflict)
                .map(|(path, _)| path.clone()),
        );
        for link in self.catalog.links.values_mut() {
            link.error = None;
        }
        self.error = None;
        cx.notify();
    }

    /// A single shared worker owns uploads across all application windows.
    pub fn sync(&mut self, cx: &mut Context<Self>) {
        if self
            .cleanup_at
            .is_none_or(|last| last.elapsed().as_secs() >= 3600)
        {
            self.cleanup_at = Some(web_time::Instant::now());
            let downloads = self.root.join("downloads");
            cx.background_executor()
                .spawn(async move {
                    if let Err(error) = Client::cleanup_downloads(&downloads) {
                        eprintln!("Download cache cleanup failed: {error}");
                    }
                })
                .detach();
        }
        if self.busy {
            return;
        }
        self.cleanup_baselines(cx);
        if self.recover_incoming(cx, |result, this, cx| {
            if result.is_ok() {
                this.sync(cx);
            }
        }) {
            return;
        }
        if self.reconnect(cx) {
            return;
        }
        let paths: Vec<_> = self
            .catalog
            .links
            .iter()
            .filter(|(_, link)| link.dirty || link.baseline.is_none())
            .map(|(path, _)| path.clone())
            .collect();
        self.reconcile_baselines(paths, cx, Self::sync_ready);
    }

    fn sync_ready(&mut self, cx: &mut Context<Self>) {
        if self.retry_at.elapsed().as_secs() >= 30 {
            self.retry_at = web_time::Instant::now();
            for link in self
                .catalog
                .links
                .values_mut()
                .filter(|link| !link.conflict)
            {
                link.error = None;
            }
        }
        let next = self.catalog.links.iter().find_map(|(path, link)| {
            if (!link.dirty && !rovar_storage::exists(self.pending_path(link)))
                || link.conflict
                || link.error.is_some()
            {
                return None;
            }
            if !link.object.deleted && !rovar_storage::exists(path) {
                return None;
            }
            let connection = self.connection(&link.connection)?;
            if !connection.authenticated {
                return None;
            }
            if !cfg!(target_family = "wasm") && connection.token.is_empty() {
                return None;
            }
            Some((
                path.clone(),
                link.clone(),
                connection.client(),
                connection.identity.clone(),
                connection.space.id.clone(),
                connection.generation,
            ))
        });
        let Some((path, link, client, identity, space, generation)) = next else {
            if let Some(connection) = self
                .catalog
                .connections
                .iter()
                .filter(|c| {
                    c.authenticated && (cfg!(target_family = "wasm") || !c.token.is_empty())
                })
                .filter(|c| {
                    self.refresh_at
                        .get(&c.id)
                        .is_none_or(|last| last.elapsed().as_secs() >= 5)
                })
                .min_by_key(|c| self.refresh_at.get(&c.id).copied())
                .map(|c| c.id.clone())
            {
                self.refresh_at
                    .insert(connection.clone(), web_time::Instant::now());
                // Autosave calls sync while its Studio entity is leased. Wait
                // until that update ends before collecting open document paths.
                let remote = cx.entity().downgrade();
                cx.defer(move |cx| {
                    let open = crate::app::Studio::protected_document_paths(cx);
                    let _ = remote.update(cx, |this, cx| {
                        if this.try_merge(&open, cx) {
                            return;
                        }
                        if this
                            .connection(&connection)
                            .is_some_and(|c| c.authenticated)
                        {
                            this.refresh(connection, open, cx);
                        }
                    });
                });
            }
            return;
        };
        self.busy = true;
        let pending_path = self
            .root
            .join("pending")
            .join(&link.connection)
            .join(format!("{}.json", link.object.id));
        let executor = cx.background_executor().clone();
        let root = self.root.clone();
        cx.spawn(async move |this, cx| {
            let result = async {
                let actual: Identity = client.json("GET", "session", None).await?;
                if actual.server_id != identity.server_id || actual.user_id != identity.user_id {
                    return Err(HttpError::account_changed().into());
                }
                let prepared = {
                    let root = root.clone();
                    let path = path.clone();
                    let link = link.clone();
                    let record = pending_path.clone();
                    executor
                        .spawn(async move { upload::prepare(&root, &path, &link, &record) })
                        .await?
                };
                let pending = prepared.pending;
                let transfer = prepared.media.send(&client, &space).await?;
                let (object, pending) = delta::send(
                    &client,
                    &space,
                    &link.object.id,
                    &pending_path,
                    pending,
                    transfer,
                    &executor,
                )
                .await?;
                executor
                    .spawn(async move { upload::confirm(&root, object, pending) })
                    .await
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if result.is_err()
                    && this
                        .connection(&link.connection)
                        .is_none_or(|c| c.generation != generation)
                {
                    cx.notify();
                    return;
                }
                if this
                    .connection(&link.connection)
                    .is_some_and(|c| c.generation == generation)
                    && result.as_ref().err().is_some_and(|error| {
                        error
                            .downcast_ref::<HttpError>()
                            .is_some_and(|e| e.status == 401)
                    })
                {
                    this.sign_out(&link.connection, cx);
                }
                if let Some(current) = this.catalog.links.get_mut(&path) {
                    match result {
                        Ok(upload::Confirmed {
                            object,
                            digest: sent_digest,
                            baseline,
                        }) => {
                            // Compare the latest local file against this confirmed
                            // baseline in the background after persisting the receipt.
                            current.dirty = true;
                            current.baseline = Some(baseline);
                            current.object.revision = object.revision;
                            current.object.created = object.created;
                            current.object.modified = object.modified;
                            current.digest = sent_digest;
                            current.error = None;
                            // Persist the acknowledged revision before removing the retry record.
                            if this.persist() {
                                let _ = rovar_storage::fs::remove_file(&pending_path);
                            }
                        }
                        Err(error) => {
                            current.conflict = error
                                .downcast_ref::<HttpError>()
                                .is_some_and(HttpError::is_conflict);
                            current.error = Some(error.to_string());
                            this.persist();
                        }
                    }
                }
                if this
                    .catalog
                    .links
                    .get(&path)
                    .is_some_and(|link| link.conflict && link.object.kind == Kind::ColorStyle)
                    && let Err(error) = this.preserve_color_conflict(&path, cx)
                {
                    this.error = Some(error.to_string());
                }
                if this.catalog.links.get(&path).is_some_and(|link| {
                    link.conflict && !link.object.deleted && link.object.kind == Kind::Document
                }) {
                    this.merge_pending.insert(path.clone());
                    let remote = cx.entity().downgrade();
                    cx.defer(move |cx| {
                        let protected = crate::app::Studio::protected_document_paths(cx);
                        let _ = remote.update(cx, |this, cx| {
                            this.try_merge(&protected, cx);
                        });
                    });
                } else {
                    this.reconcile_baselines(vec![path], cx, |_, _| {});
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn fork_conflict(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(link) = self
            .catalog
            .links
            .get_mut(path)
            .filter(|link| link.conflict)
        else {
            return;
        };
        let pending = self
            .root
            .join("pending")
            .join(&link.connection)
            .join(format!("{}.json", link.object.id));
        link.object.id = uuid::Uuid::new_v4().to_string();
        link.object.title =
            crate::i18n::message("page-copy-name", &[("name", link.object.title.clone())]);
        link.object.revision = 0;
        link.object.created = crate::platform::now();
        link.object.deleted = false;
        link.conflict = false;
        link.error = None;
        link.dirty = true;
        if self.persist() {
            let _ = rovar_storage::fs::remove_file(pending);
        }
        cx.notify();
    }

    pub fn refresh(&mut self, connection: String, open: BTreeSet<PathBuf>, cx: &mut Context<Self>) {
        self.refresh_selected(connection, open, None, cx);
    }

    pub(crate) fn refresh_document(
        &mut self,
        path: PathBuf,
        open: BTreeSet<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let Some(link) = self.catalog.links.get(&path) else {
            return;
        };
        self.refresh_selected(
            link.connection.clone(),
            open,
            Some(link.object.id.clone()),
            cx,
        );
    }

    fn refresh_selected(
        &mut self,
        connection: String,
        open: BTreeSet<PathBuf>,
        selected: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let resume_connection = connection.clone();
        let resume_open = open.clone();
        let resume_selected = selected.clone();
        if self.recover_incoming(cx, move |result, this, cx| {
            if result.is_ok() {
                this.refresh_selected(resume_connection, resume_open, resume_selected, cx);
            }
        }) {
            return;
        }
        let paths: Vec<_> = self
            .catalog
            .links
            .iter()
            .filter(|(_, link)| link.connection == connection)
            .map(|(path, _)| path.clone())
            .collect();
        self.reconcile_baselines(paths, cx, move |this, cx| {
            this.refresh_ready(connection, open, selected, cx);
        });
    }

    fn refresh_ready(
        &mut self,
        connection: String,
        open: BTreeSet<PathBuf>,
        selected: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if !self.persist() {
            return;
        }
        // The user may have signed out while the preflight scan was running.
        let Some(server) = self.connection(&connection).filter(|c| c.authenticated) else {
            return;
        };
        let client = server.client();
        let generation = server.generation;
        let identity = server.identity.clone();
        let space = server.space.id.clone();
        let known = self.catalog.links.clone();
        let mut directory = self
            .catalog
            .directories
            .get(&connection)
            .cloned()
            .unwrap_or_default();
        let root = self.root.clone();
        let executor = cx.background_executor().clone();
        self.refresh_at
            .insert(connection.clone(), web_time::Instant::now());
        self.busy = true;
        self.error = None;
        cx.spawn(async move |this, cx| {
            let mut observed_identity = None;
            let result = async {
                let remote_identity: Identity = client.json("GET", "session", None).await?;
                if remote_identity.server_id != identity.server_id
                    || remote_identity.user_id != identity.user_id
                {
                    return Err(HttpError::account_changed().into());
                }
                observed_identity = Some(remote_identity);
                if let Some(id) = &selected {
                    let object: Object = client
                        .json(
                            "GET",
                            &format!("spaces/{space}/objects/{id}/metadata"),
                            None,
                        )
                        .await?;
                    directory.queue(object);
                } else {
                    // One bounded page per refresh keeps large workspaces from
                    // monopolizing the sync worker. Deferred objects survive restart.
                    let page: rovar_api::Changes = client
                        .json(
                            "GET",
                            &format!("spaces/{space}/changes?after={}", directory.cursor),
                            None,
                        )
                        .await?;
                    ensure!(
                        page.cursor >= directory.cursor,
                        "Server sync cursor moved backwards"
                    );
                    directory.cursor = page.cursor;
                    for object in page.objects {
                        directory.queue(object);
                    }
                }
                // Missing local caches must still be repaired when the remote
                // directory itself has not changed.
                for (path, link) in &known {
                    if link.connection == connection
                        && !link.object.deleted
                        && !rovar_storage::exists(path)
                    {
                        directory
                            .pending
                            .entry(link.object.id.clone())
                            .or_insert_with(|| link.object.clone());
                    }
                }
                let mut updates = Vec::new();
                let mut downloaded = 0usize;
                let mut attempted = 0;
                let mut pending = directory.pending.values().cloned().collect::<Vec<_>>();
                // New work precedes retries, including when a large set of
                // failing objects exhausts one refresh's attempt budget.
                pending.sort_by_key(|object| directory.attempts(&object.id));
                for object in pending {
                    // Bound each refresh's transfer and staging work, including
                    // the first scan of a workspace with many large documents.
                    if downloaded >= 16 * 1024 * 1024 || attempted >= 16 {
                        break;
                    }
                    if selected.as_ref().is_some_and(|id| id != &object.id) {
                        continue;
                    }
                    let existing = known
                        .iter()
                        .find(|(_, l)| l.connection == connection && l.object.id == object.id);
                    if let Some((path, link)) = existing {
                        if link.object.revision >= object.revision
                            && (link.object.deleted || rovar_storage::exists(path))
                        {
                            directory.complete(&object.id);
                            continue;
                        }
                        if link.dirty || open.contains(path) {
                            continue;
                        }
                    }
                    if selected.is_none() && !directory.ready(&object.id) {
                        continue;
                    }
                    attempted += 1;
                    let path =
                        existing
                            .map(|(p, _)| p.clone())
                            .unwrap_or_else(|| match object.kind {
                                Kind::Document => root
                                    .join("documents")
                                    .join(format!("{}.rovar", uuid::Uuid::new_v4())),
                                Kind::Component => root
                                    .join("servers")
                                    .join(&connection)
                                    .join("components")
                                    .join(format!("{}.rovar", object.id)),
                                Kind::ColorStyle => root
                                    .join("servers")
                                    .join(&connection)
                                    .join("colors")
                                    .join(format!("{}.json", object.id)),
                            });
                    let base = if let Some((_, link)) = existing.filter(|_| !object.deleted) {
                        let root = root.clone();
                        let link = link.clone();
                        executor
                            .spawn(async move { baseline::read(&root, &link).ok().flatten() })
                            .await
                    } else {
                        None
                    };
                    let result = if object.deleted {
                        Ok((object.clone(), None))
                    } else {
                        delta::receive(
                            &client,
                            &space,
                            &object,
                            base,
                            &path,
                            &root.join("downloads"),
                            &executor,
                        )
                        .await
                        .map(|(object, bytes)| (object, Some(bytes)))
                    };
                    let result = match result {
                        Ok((object, bytes)) => {
                            downloaded += bytes.as_ref().map_or(0, Vec::len);
                            let root = root.clone();
                            let path = path.clone();
                            executor
                                .spawn(async move {
                                    cache::prepare_snapshot(&root, &path, object, bytes)
                                })
                                .await
                        }
                        Err(error) => Err(error),
                    };
                    match result {
                        Ok(prepared) => {
                            updates.push((path, prepared));
                        }
                        // Authentication failures invalidate the entire batch;
                        // object failures must not discard unrelated successes.
                        Err(error)
                            if error
                                .downcast_ref::<HttpError>()
                                .is_some_and(|e| e.status == 401) =>
                        {
                            return Err(error);
                        }
                        Err(error) => directory.fail(&object, &error),
                    }
                }
                Ok::<_, anyhow::Error>((updates, directory))
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if this
                    .connection(&connection)
                    .is_none_or(|c| c.generation != generation)
                {
                    cx.notify();
                    return;
                }
                if let Some(identity) = observed_identity
                    && this.connection(&connection).is_some_and(|current| {
                        current.authenticated && current.token == client.token
                    })
                {
                    let _ = this.connect(client.url.clone(), identity, client.token.clone(), cx);
                }
                match result {
                    Ok((updates, directory)) => {
                        let paths = updates.iter().map(|(path, _)| path.clone()).collect();
                        this.reconcile_baselines(paths, cx, move |this, cx| {
                            if this
                                .connection(&connection)
                                .is_none_or(|c| c.generation != generation)
                            {
                                return;
                            }
                            let previous = this
                                .catalog
                                .directories
                                .insert(connection.clone(), directory);
                            if !this.persist() {
                                if let Some(previous) = previous {
                                    this.catalog
                                        .directories
                                        .insert(connection.clone(), previous);
                                } else {
                                    this.catalog.directories.remove(&connection);
                                }
                                cx.notify();
                                return;
                            }
                            let currently_open = crate::app::Studio::protected_document_paths(cx);
                            for (path, prepared) in updates {
                                if currently_open.contains(&path)
                                    || this.catalog.links.get(&path).is_some_and(|link| link.dirty)
                                {
                                    continue;
                                }
                                let object = prepared.object.clone();
                                let id = object.id.clone();
                                if let Err(error) =
                                    this.install_snapshot(path, connection.clone(), prepared)
                                {
                                    this.catalog
                                        .directories
                                        .get_mut(&connection)
                                        .unwrap()
                                        .fail(&object, &error);
                                    // A journal must be recovered before another
                                    // publication can reuse it. Keep later items queued.
                                    if this.ensure_recovered().is_err() {
                                        break;
                                    }
                                    continue;
                                }
                                this.catalog
                                    .directories
                                    .get_mut(&connection)
                                    .unwrap()
                                    .complete(&id);
                            }
                            if this.error.is_none() {
                                this.error = this.catalog.directories[&connection].error();
                            }
                            this.persist();
                            this.recover_incoming(cx, |_, _, _| {});
                        });
                    }
                    Err(error) => {
                        if error
                            .downcast_ref::<HttpError>()
                            .is_some_and(|e| e.status == 401)
                        {
                            this.sign_out(&connection, cx);
                        }
                        this.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn library_changed(
        &mut self,
        directory: &Path,
        entries: &[crate::document::library::Entry],
        cx: &mut Context<Self>,
    ) {
        let Some(connection) = self
            .connections()
            .iter()
            .find(|c| self.library_root(&c.id).join("components") == directory)
            .map(|c| c.id.clone())
        else {
            return;
        };
        let paths: BTreeSet<_> = entries.iter().map(|entry| entry.path.clone()).collect();
        let mut changed = false;
        for entry in entries.iter().filter(|entry| entry.error.is_none()) {
            *self.local_changes.entry(entry.path.clone()).or_default() += 1;
            let Ok(bytes) = rovar_storage::fs::read(&entry.path) else {
                continue;
            };
            let hash = digest(&bytes, &entry.name, false);
            if let Some(link) = self.catalog.links.get_mut(&entry.path) {
                if link.digest != hash {
                    link.object.title = entry.name.clone();
                    link.dirty = true;
                    changed = true;
                }
            } else {
                self.catalog.links.insert(
                    entry.path.clone(),
                    Link {
                        connection: connection.clone(),
                        object: Object {
                            id: uuid::Uuid::new_v4().to_string(),
                            kind: Kind::Component,
                            title: entry.name.clone(),
                            revision: 0,
                            created: crate::platform::now(),
                            modified: crate::platform::now(),
                            deleted: false,
                        },
                        dirty: true,
                        digest: String::new(),
                        baseline: None,
                        conflict: false,
                        error: None,
                    },
                );
                changed = true;
            }
        }
        for (path, link) in &mut self.catalog.links {
            if link.connection == connection
                && link.object.kind == Kind::Component
                && !link.object.deleted
                && !paths.contains(path)
            {
                link.object.deleted = true;
                link.dirty = true;
                *self.local_changes.entry(path.clone()).or_default() += 1;
                changed = true;
            }
        }
        if changed {
            self.persist();
            cx.notify();
        }
    }
}
