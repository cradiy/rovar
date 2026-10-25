use super::*;
use protocol::{CreateTab, OpenTab, TabTarget};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Receipts(HashMap<String, (Request, Value)>);
impl gpui::Global for Receipts {}

pub(super) fn blocked(studio: &Studio) -> bool {
    studio.closing
        || studio.comparison.is_some()
        || studio.strip.drag.is_some()
        || studio.dragging.is_some()
        || !studio.open_errors.is_empty()
        || studio.preferences.is_some()
        || studio.renaming.is_some()
        || studio.deleting_document.is_some()
        || studio.server_action.is_some()
        || studio.signing_out
}

fn describe_tab(
    tab: &Tab,
    active: Option<usize>,
    blocked: bool,
    window: &Window,
    cx: &gpui::App,
) -> Value {
    let editor = tab.editor.as_ref().map(|editor| editor.read(cx));
    json!({
        "tab_id": tab.token.to_string(), "document_id": tab.document_id,
        "title": tab.file.title, "path": tab.file.path, "active": active == Some(tab.token),
        "loaded": tab.editor.is_some(), "loading": tab.loading,
        "checking_remote": tab.checking_remote, "saving": tab.saving,
        "closing": tab.close_after_save, "error": tab.error,
        "ready": !blocked && !tab.loading && !tab.checking_remote && !tab.close_after_save
            && editor.is_some_and(|editor| !editor.agent_busy(window, cx)),
        "session": editor.map(|editor| editor.agent_scope(&tab.document_id)),
        "dirty": tab.editor.as_ref().is_some_and(|editor| {
            tab.needs_upgrade || tab.saved_revision != Some(editor.read(cx).document_revision())
        }),
    })
}

fn list(cx: &mut gpui::App) -> Value {
    let mut windows = Vec::new();
    for handle in cx
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<Studio>())
    {
        let _ = handle.update(cx, |studio, window, cx| {
            windows.push(json!({
                "window_id": studio.window_id.to_string(),
                "source": studio.source, "busy": blocked(studio),
                "active_tab_id": studio.active.map(|token| token.to_string()),
                "tabs": studio.tabs.iter().map(|tab| describe_tab(tab, studio.active, blocked(studio), window, cx)).collect::<Vec<_>>(),
                "open_errors": studio.open_errors.iter().map(|error| json!({
                    "tab_id": error.token.to_string(), "title": error.title, "message": error.detail,
                })).collect::<Vec<_>>(),
            }));
        });
    }
    windows.sort_by(|a, b| a["window_id"].as_str().cmp(&b["window_id"].as_str()));
    json!({"windows": windows, "remaining_tab_receipts": protocol::MAX_RECEIPTS - cx.global::<Receipts>().0.len()})
}

pub(super) fn dispatch(request: &Request, cx: &mut gpui::App) -> protocol::Result<Value> {
    if request.session_id.is_some() || request.document_id.is_some() {
        return Err(Error::invalid(
            "Tab methods use window_id/tab_id in params, without document/session scope",
        ));
    }
    if request.method == "tabs.list" {
        let _: protocol::Empty = protocol::parse(request.params.clone())?;
        return Ok(list(cx));
    }
    // Retried lifecycle requests must not create a second document or affect a new active tab.
    if let Some((previous, value)) = cx.global::<Receipts>().0.get(&request.request_id) {
        return if previous == request {
            Ok(value.clone())
        } else {
            Err(Error::new(
                "REQUEST_ID_REUSED",
                "Use a new request_id for a different tab operation",
            ))
        };
    }
    if cx.global::<Receipts>().0.len() >= protocol::MAX_RECEIPTS {
        return Err(Error::new(
            "RECEIPT_LIMIT",
            "Tab operation receipt limit reached for this process",
        ));
    }
    if serde_json::to_vec(request)
        .map_err(|e| Error::invalid(e.to_string()))?
        .len()
        > 16 * 1024
    {
        return Err(Error::invalid("Tab requests must not exceed 16 KiB"));
    }
    let value = execute(request, cx)?;
    cx.global_mut::<Receipts>()
        .0
        .insert(request.request_id.clone(), (request.clone(), value.clone()));
    Ok(value)
}

fn execute(request: &Request, cx: &mut gpui::App) -> protocol::Result<Value> {
    enum Operation {
        Create(Option<String>),
        Open(PathBuf),
        Target(String),
    }
    let (mut window_id, operation) = match request.method.as_str() {
        "tabs.create" => {
            let params: CreateTab = protocol::parse(request.params.clone())?;
            if params
                .title
                .as_ref()
                .is_some_and(|t| t.trim().is_empty() || t.len() > 256)
            {
                return Err(Error::invalid(
                    "title must contain 1 to 256 bytes and not be blank",
                ));
            }
            (params.window_id, Operation::Create(params.title))
        }
        "tabs.open" => {
            let params: OpenTab = protocol::parse(request.params.clone())?;
            if !params.path.is_absolute()
                || !params
                    .path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("rovar"))
            {
                return Err(Error::invalid("path must be an absolute .rovar file path"));
            }
            let path = std::fs::canonicalize(params.path)
                .map_err(|e| Error::new("OPEN_FAILED", e.to_string()))?;
            if !path.is_file() {
                return Err(Error::invalid("path must name a file"));
            }
            (params.window_id, Operation::Open(path))
        }
        "tabs.activate" | "tabs.close" | "tabs.save" => {
            let params: TabTarget = protocol::parse(request.params.clone())?;
            (params.window_id, Operation::Target(params.tab_id))
        }
        _ => return Err(Error::new("UNSUPPORTED", "Unknown tab method")),
    };
    let handles = cx
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<Studio>())
        .collect::<Vec<_>>();
    let find_window = |id: &str| {
        handles
            .iter()
            .find(|h| h.window_id().as_u64().to_string() == id)
            .copied()
    };
    let mut handle = find_window(&window_id)
        .ok_or_else(|| Error::new("WINDOW_CLOSED", "Window no longer exists; call tabs.list"))?;
    // Match UI behavior: opening an already-open file activates its owning window.
    if let Operation::Open(path) = &operation {
        for candidate in &handles {
            let found = candidate
                .update(cx, |studio, _, _| {
                    studio.tabs.iter().any(|tab| &tab.file.path == path)
                })
                .unwrap_or(false);
            if found {
                handle = *candidate;
                window_id = handle.window_id().as_u64().to_string();
                break;
            }
        }
    }
    handle.update(cx, |studio, window, cx| {
        if blocked(studio) {
            return Err(Error::new("BUSY", "Window has a pending dialog, drag, comparison, or close"));
        }
        let token = match operation {
            Operation::Create(title) => {
                studio.new_named_document(title, window, cx);
                studio.active.ok_or_else(|| Error::new("BUSY", "Could not create document"))?
            }
            Operation::Open(path) => {
                if studio.tabs.iter().any(|tab| tab.file.path == path && tab.close_after_save) {
                    return Err(Error::new("BUSY", "The existing tab is closing"));
                }
                studio.open_path(path, window, cx);
                studio.active.ok_or_else(|| Error::new("OPEN_FAILED", "Could not open document"))?
            }
            Operation::Target(id) => {
                let tab = studio.tabs.iter_mut().find(|tab| tab.token.to_string() == id)
                    .ok_or_else(|| Error::new("TAB_CLOSED", "Tab no longer exists; call tabs.list"))?;
                let token = tab.token;
                if tab.close_after_save || tab.loading || tab.checking_remote {
                    return Err(Error::new("BUSY", "Tab is loading, checking remote state, or closing"));
                }
                match request.method.as_str() {
                    "tabs.activate" => studio.select_tab(Some(token), window, cx),
                    "tabs.close" => studio.close_tab(token, window, cx),
                    "tabs.save" => {
                        let editor = tab.editor.clone().ok_or_else(|| Error::new("BUSY", "Activate the tab to load it before saving"))?;
                        editor.update(cx, |editor, cx| editor.suspend(window, cx));
                        tab.save_requested = true;
                        studio.save_tab(token, window, cx);
                    }
                    _ => unreachable!(),
                }
                token
            }
        };
        if matches!(request.method.as_str(), "tabs.create" | "tabs.open" | "tabs.activate") {
            window.activate_window();
        }
        Ok(json!({"window_id":window_id,"tab_id":token.to_string(),"status":"accepted",
            "tab":studio.tabs.iter().find(|tab| tab.token == token).map(|tab| describe_tab(tab, studio.active, blocked(studio), window, cx))}))
    }).map_err(|e| Error::new("WINDOW_CLOSED", e.to_string()))?
}
