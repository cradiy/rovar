use super::*;
use crate::agent::{
    protocol::{self, Error, Request},
    transport,
};
use futures_util::StreamExt;
use serde_json::json;
#[cfg(test)]
mod tests;

pub(super) fn start(base: &std::path::Path, cx: &mut gpui::App) -> anyhow::Result<()> {
    let (server, mut requests) = transport::listen(base)?;
    let stop = server.stop.clone();
    cx.set_global(server);
    cx.on_app_quit(|cx| {
        cx.remove_global::<transport::Server>();
        async {}
    })
    .detach();
    cx.spawn(async move |cx| {
        while let Some(pending) = requests.next().await {
            let prepared = cx.update(|cx| {
                if stop.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(Error::new("SESSION_CLOSED", "Editor endpoint has closed"));
                }
                if std::time::Instant::now() >= pending.deadline {
                    return Err(Error::new("BUSY", "Request expired before execution"));
                }
                dispatch(&pending.request, cx)
            });
            let result = match prepared {
                Ok(protocol::Reply::Ready(value)) => Ok(value),
                Ok(capture) => {
                    cx.background_executor()
                        .spawn(async move { capture.finish() })
                        .await
                }
                Err(error) => Err(error),
            };
            let _ = pending
                .reply
                .send(protocol::response(&pending.request, result));
        }
    })
    .detach();
    Ok(())
}

fn dispatch(request: &Request, cx: &mut gpui::App) -> protocol::Result<protocol::Reply> {
    if request.request_id.is_empty() || request.request_id.len() > 128 {
        return Err(Error::invalid("request_id must contain 1 to 128 bytes"));
    }
    if request.method == "schema.get" {
        let params: protocol::Schema = protocol::parse(request.params.clone())?;
        return protocol::schema(params.method.as_deref()).map(protocol::Reply::Ready);
    }
    let mut sessions = Vec::new();
    let mut target = None;
    for handle in cx
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<Studio>())
    {
        let _ = handle.update(cx, |studio, window, cx| {
            // Only active tabs are exposed. Switching tabs never redirects a scoped request.
            if studio.closing || studio.comparison.is_some() {
                return;
            }
            let Some(tab) = studio
                .tabs
                .iter()
                .find(|tab| Some(tab.token) == studio.active)
            else {
                return;
            };
            let Some(editor) = &tab.editor else {
                return;
            };
            if tab.loading || tab.checking_remote {
                return;
            }
            let session =
                editor
                    .read(cx)
                    .agent_session(&tab.document_id, &tab.file.title, window, cx);
            if request.session_id.as_deref() == session["session_id"].as_str()
                && request.document_id.as_deref() == Some(tab.document_id.as_str())
            {
                target = Some((handle, editor.clone()));
            }
            sessions.push(session);
        });
    }
    if request.method == "session.describe" {
        let _: protocol::Empty = protocol::parse(request.params.clone())?;
        return Ok(protocol::Reply::Ready(
            json!({"sessions":sessions,"capabilities":{
                "methods":["session.describe","schema.get","nodes.get","nodes.find","document.apply","canvas.capture","selection.set","canvas.focus","canvas.fit"],
                "create_types":["frame","rectangle","ellipse","text","group"],
                "operations":["create","update","delete"],
                "coordinates":"page","atomic_apply":true,"undo":"one apply per undo entry",
                "max_operations":protocol::MAX_OPERATIONS,"max_read_nodes":protocol::MAX_NODES,
                "max_apply_receipts":protocol::MAX_RECEIPTS,"max_request_bytes":transport::MAX_REQUEST_BYTES,
            "max_receipt_bytes":protocol::MAX_RECEIPT_BYTES,
                "capture_renderer":"export","capture_media":false,
                "scope":"active tabs and their active pages","assets_import":false
            }}),
        ));
    }
    let (handle, editor) = target.ok_or_else(|| Error::new("SESSION_CLOSED",
        "No matching active session; call session.describe and use its document_id and session_id"))?;
    handle
        .update(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| editor.agent_request(request, window, cx))
        })
        .map_err(|e| Error::new("SESSION_CLOSED", e.to_string()))?
}
