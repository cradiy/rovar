use super::*;
use gpui::TestAppContext;
use std::io::{BufRead, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn call(
    path: &std::path::Path,
    request: serde_json::Value,
    cx: &mut TestAppContext,
) -> serde_json::Value {
    let path = path.to_owned();
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut stream = transport::connect(&path).unwrap();
        serde_json::to_writer(&mut stream, &request).unwrap();
        stream.write_all(b"\n").unwrap();
        let mut line = String::new();
        std::io::BufReader::new(stream)
            .read_line(&mut line)
            .unwrap();
        send.send(serde_json::from_str::<serde_json::Value>(&line).unwrap())
            .unwrap();
    });
    let start = std::time::Instant::now();
    let response = loop {
        cx.run_until_parked();
        if let Ok(value) = receive.try_recv() {
            break value;
        }
        assert!(start.elapsed().as_secs() < 8, "Endpoint response timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    worker.join().unwrap();
    response
}

#[gpui::test]
fn endpoint_routes_only_to_the_named_active_editor(cx: &mut TestAppContext) {
    cx.update(uic::init);
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| start(root.path(), cx).unwrap());
    let endpoints = transport::discover(root.path()).unwrap();
    assert_eq!(endpoints.as_array().unwrap().len(), 1);
    let path = PathBuf::from(endpoints[0]["endpoint"].as_str().unwrap());
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let (other, _requests) = transport::listen(root.path()).unwrap();
    assert_eq!(
        transport::discover(root.path())
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
    drop(other);
    // An unclean exit must not make discovery return an unreachable endpoint.
    let stale = tempfile::Builder::new()
        .prefix("rovar-live-")
        .tempdir_in(root.path())
        .unwrap();
    #[cfg(unix)]
    {
        std::fs::set_permissions(stale.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        drop(std::os::unix::net::UnixListener::bind(stale.path().join("editor.sock")).unwrap());
    }
    std::fs::write(
        stale.path().join("session.json"),
        serde_json::to_vec(&json!({"pid":1,"endpoint":r"\\.\pipe\rovar-missing"})).unwrap(),
    )
    .unwrap();
    assert_eq!(transport::discover(root.path()).unwrap(), endpoints);
    let handle = cx.open_window(gpui::size(px(1000.), px(800.)), |window, cx| {
        Studio::new(root.path().into(), window, cx)
    });
    handle
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    cx.run_until_parked();
    let described = call(
        &path,
        json!({"request_id":"describe","method":"session.describe"}),
        cx,
    );
    let session = &described["result"]["sessions"][0];
    assert!(session["session_id"].is_string(), "{described}");
    let edit = json!({"request_id":"create","method":"document.apply", "session_id":session["session_id"],
        "document_id":session["document_id"],"params":{"page_id":session["page_id"],"base_revision":session["revision"],
            "operations":[{"op":"create","ref":"$rect","type":"rectangle","properties":{
                "rect":{"x":20,"y":30,"width":40,"height":50}}},
                {"op":"import","ref":"$svg","rect":{"x":100,"y":30,"width":40,"height":50},
                    "source":{"type":"svg","source":{"type":"data","svg":"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"50\"><rect width=\"40\" height=\"50\"/></svg>"}}}]}});
    let committed = call(&path, edit.clone(), cx);
    assert!(
        committed["result"]["created"]["$rect"].is_string(),
        "{committed}"
    );
    assert_eq!(call(&path, edit.clone(), cx), committed);
    assert!(committed["result"]["created"]["$svg"].is_string());
    handle
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    assert_eq!(call(&path, edit, cx)["error"]["code"], "SESSION_CLOSED");
    let schema = call(
        &path,
        json!({"request_id":"schema","method":"schema.get","params":{"method":"document.apply"}}),
        cx,
    );
    assert!(schema["result"]["properties"]["operations"].is_object());
    handle
        .update(cx, |studio, window, cx| {
            studio.select_tab(Some(1), window, cx);
            studio.active_editor().unwrap().update(cx, |editor, cx| {
                editor.undo_redo(false, window, cx);
                let (bytes, _) = editor
                    .snapshot_document(&studio.tabs[0].document_id, cx)
                    .unwrap();
                assert!(
                    crate::document::Document::decode(&bytes).unwrap().pages[0]
                        .shapes
                        .is_empty()
                );
            });
        })
        .unwrap();
    cx.update(|cx| {
        cx.remove_global::<transport::Server>();
    });
    #[cfg(unix)]
    {
        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
    }
    assert!(transport::connect(&path).is_err());
    assert_eq!(transport::discover(root.path()).unwrap(), json!([]));
}
