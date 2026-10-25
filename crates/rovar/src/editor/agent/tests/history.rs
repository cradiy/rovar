use super::*;

fn history_request(w: &Workspace, redo: bool) -> Request {
    request(
        w,
        if redo {
            "document.redo"
        } else {
            "document.undo"
        },
        json!({"base_revision":w.document_revision().to_string()}),
    )
}

#[gpui::test]
fn history_requests_are_revision_guarded_and_retries_never_replay_again(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |w, window, cx| {
        let empty = history_request(w, false);
        let empty_result = execute(w, &empty, window, cx).unwrap();
        assert_eq!(empty_result, json!({"revision":"0","changed":false}));
        let setup = apply(w, json!([
            {"op":"create","ref":"$a","type":"rectangle","properties":{"rect":{"x":20,"y":30,"width":40,"height":50}}},
            {"op":"create","ref":"$b","type":"ellipse","properties":{"rect":{"x":80,"y":30,"width":40,"height":50}}}
        ]));
        execute(w, &setup, window, cx).unwrap();
        let before = w.snapshot_page(cx).0;
        let undo = history_request(w, false);
        let revision = w.document_revision();
        let bytes = w.agent.receipt_bytes;
        w.agent.receipt_bytes = p::MAX_RECEIPT_BYTES;
        assert_eq!(execute(w, &undo, window, cx).unwrap_err().code, "LIMIT_REACHED");
        assert_eq!(w.document_revision(), revision);
        assert!(w.snapshot_page(cx).0 == before);
        w.agent.receipt_bytes = bytes;
        w.history.borrow_mut().begin_preview();
        assert_eq!(execute(w, &undo, window, cx).unwrap_err().code, "BUSY");
        w.history.borrow_mut().end_preview(false);
        let mut stale = undo.clone();
        stale.params["base_revision"] = json!("0");
        assert_eq!(execute(w, &stale, window, cx).unwrap_err().code, "REVISION_CONFLICT");
        let undone = execute(w, &undo, window, cx).unwrap();
        assert_eq!(undone["revision"], (revision + 1).to_string());
        assert!(w.shapes.is_empty(), "A whole apply batch is a single undo");
        assert!(w.can_undo_redo(true));
        assert_eq!(execute(w, &undo, window, cx).unwrap(), undone);
        assert_eq!(w.document_revision(), revision + 1);
        let redo = history_request(w, true);
        let redone = execute(w, &redo, window, cx).unwrap();
        assert_eq!(redone["revision"], (revision + 2).to_string());
        assert!(w.snapshot_page(cx).0 == before);
        assert_eq!(execute(w, &undo, window, cx).unwrap(), undone);
        assert_eq!(execute(w, &empty, window, cx).unwrap(), empty_result);
        assert!(w.snapshot_page(cx).0 == before);
        let mut reused = undo;
        reused.params["base_revision"] = json!(w.document_revision().to_string());
        assert_eq!(execute(w, &reused, window, cx).unwrap_err().code, "REQUEST_ID_REUSED");
        let no_redo = history_request(w, true);
        assert_eq!(execute(w, &no_redo, window, cx).unwrap()["changed"], false);
        assert_eq!(w.document_revision(), revision + 2);
    }).unwrap();
}
