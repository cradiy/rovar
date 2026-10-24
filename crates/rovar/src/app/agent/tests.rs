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
fn endpoint_discovers_instances_and_manages_scoped_document_tabs(cx: &mut TestAppContext) {
    cx.update(uic::init);
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| start(root.path(), cx).unwrap());
    let endpoints = transport::discover(root.path()).unwrap();
    assert_eq!(endpoints.as_array().unwrap().len(), 1);
    assert_eq!(
        endpoints[0]["executable"],
        json!(std::env::current_exe().unwrap())
    );
    let registrations = std::fs::read_dir(root.path())
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let registration = registrations[0].path();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(registration.join("session.json")).unwrap()
        )
        .unwrap(),
        endpoints[0]
    );
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
    let listed = call(&path, json!({"request_id":"tabs","method":"tabs.list"}), cx);
    let window_id = listed["result"]["windows"][0]["window_id"].clone();
    assert_eq!(listed["result"]["windows"][0]["tabs"], json!([]));
    let create = json!({"request_id":"new-tab","method":"tabs.create","params":{"window_id":window_id,"title":"Agent design"}});
    let created = call(&path, create.clone(), cx);
    assert_eq!(
        created["result"]["tab"]["title"], "Agent design",
        "{created}"
    );
    assert_eq!(call(&path, create.clone(), cx), created);
    let mut reused = create.clone();
    reused["params"]["title"] = json!("Different title");
    assert_eq!(
        call(&path, reused, cx)["error"]["code"],
        "REQUEST_ID_REUSED"
    );
    cx.run_until_parked();
    let described = call(
        &path,
        json!({"request_id":"describe","method":"session.describe"}),
        cx,
    );
    let session = &described["result"]["sessions"][0];
    assert!(session["session_id"].is_string(), "{described}");
    assert_eq!(session["window_id"], window_id);
    assert_eq!(session["tab_id"], created["result"]["tab_id"]);
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
    let mut second = create;
    second["request_id"] = json!("second-tab");
    let second = call(&path, second, cx);
    assert_ne!(second["result"]["tab_id"], created["result"]["tab_id"]);
    assert_eq!(call(&path, edit, cx)["error"]["code"], "SESSION_CLOSED");
    let schema = call(
        &path,
        json!({"request_id":"schema","method":"schema.get","params":{"method":"document.apply"}}),
        cx,
    );
    assert!(schema["result"]["properties"]["operations"].is_object());
    let activate = json!({"request_id":"activate-first","method":"tabs.activate","params":{
        "window_id":window_id,"tab_id":created["result"]["tab_id"]}});
    assert_eq!(call(&path, activate, cx)["result"]["tab"]["active"], true);
    handle
        .update(cx, |studio, window, cx| {
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
    let close = json!({"request_id":"close-first","method":"tabs.close","params":{
        "window_id":window_id,"tab_id":created["result"]["tab_id"]}});
    let closed = call(&path, close.clone(), cx);
    assert_eq!(closed["result"]["status"], "accepted", "{closed}");
    assert_eq!(call(&path, close, cx), closed);
    cx.run_until_parked();
    handle
        .update(cx, |studio, _, _| {
            assert_eq!(studio.tabs.len(), 1);
            assert_eq!(
                studio.tabs[0].token.to_string(),
                second["result"]["tab_id"].as_str().unwrap()
            );
        })
        .unwrap();
    let saved_path = PathBuf::from(created["result"]["tab"]["path"].as_str().unwrap());
    let saved = crate::document::load(&saved_path)
        .unwrap()
        .into_document()
        .unwrap();
    assert!(
        saved.pages[0].shapes.is_empty(),
        "Close must save the undo before removing the tab"
    );
    let opened = call(
        &path,
        json!({"request_id":"reopen","method":"tabs.open","params":{
        "window_id":window_id,"path":saved_path}}),
        cx,
    );
    assert_eq!(opened["result"]["status"], "accepted", "{opened}");
    cx.run_until_parked();
    let listed = call(
        &path,
        json!({"request_id":"after-open","method":"tabs.list"}),
        cx,
    );
    assert_eq!(
        listed["result"]["windows"][0]["tabs"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        listed["result"]["windows"][0]["active_tab_id"],
        opened["result"]["tab_id"]
    );
    let other_window = cx.open_window(gpui::size(px(1000.), px(800.)), |window, cx| {
        Studio::new(root.path().join("other-window"), window, cx)
    });
    let already_open = call(
        &path,
        json!({"request_id":"open-existing-in-other-window","method":"tabs.open","params":{
            "window_id":other_window.window_id().as_u64().to_string(),"path":saved_path}}),
        cx,
    );
    assert_eq!(already_open["result"]["window_id"], window_id);
    assert_eq!(already_open["result"]["tab_id"], opened["result"]["tab_id"]);
    other_window
        .update(cx, |studio, _, _| assert!(studio.tabs.is_empty()))
        .unwrap();
    let save = call(
        &path,
        json!({"request_id":"save-inactive","method":"tabs.save","params":{
            "window_id":window_id,"tab_id":second["result"]["tab_id"]}}),
        cx,
    );
    assert_eq!(save["result"]["status"], "accepted", "{save}");
    handle
        .update(cx, |studio, _, _| {
            assert_eq!(
                studio.active.map(|id| id.to_string()),
                opened["result"]["tab_id"].as_str().map(str::to_owned)
            );
        })
        .unwrap();
    let invalid = root.path().join("invalid.rovar");
    std::fs::write(&invalid, b"invalid document").unwrap();
    let failed = call(
        &path,
        json!({"request_id":"invalid-file","method":"tabs.open","params":{
        "window_id":window_id,"path":invalid}}),
        cx,
    );
    cx.run_until_parked();
    let listed = call(
        &path,
        json!({"request_id":"after-error","method":"tabs.list"}),
        cx,
    );
    assert_eq!(
        listed["result"]["windows"][0]["open_errors"][0]["tab_id"],
        failed["result"]["tab_id"]
    );
    assert!(listed["result"]["windows"][0]["open_errors"][0]["message"].is_string());
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
    assert!(!registration.exists());
}
