use super::*;

fn edit_pages(this: &Workspace, operations: Value) -> Request {
    request(
        this,
        "pages.apply",
        json!({"base_revision":this.document_revision().to_string(),"operations":operations}),
    )
}

#[gpui::test]
fn page_batches_preserve_identity_rollback_and_undo_as_one_edit(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |w, window, cx| {
        let original_id = w.pages.active.clone();
        let setup = apply(w, json!([
            {"op":"create","ref":"$shape","type":"rectangle","properties":{"rect":{"x":20,"y":30,"width":80,"height":40}}},
            {"op":"create_component","id":"$shape","ref":"$component","name":"Card"}
        ]));
        execute(w, &setup, window, cx).unwrap();
        let original = w.snapshot_page(cx).0;
        let before_revision = w.document_revision();
        let edit = edit_pages(w, json!([
            {"op":"duplicate","id":original_id,"ref":"$copy","name":"Copy"},
            {"op":"create","ref":"$empty","name":"Empty","before":"$copy"},
            {"op":"rename","id":"$copy","name":"Components"},
            {"op":"reorder","id":"$copy","before":original_id}
        ]));
        let result = execute(w, &edit, window, cx).unwrap();
        let copy = result["created"]["$copy"].as_str().unwrap();
        assert_eq!(w.pages.active, original_id);
        assert_eq!(w.document_revision(), before_revision + 1);
        assert_eq!(w.pages.entries[0].page.id, copy);
        let duplicate = w.agent_page(copy, cx);
        assert_eq!(duplicate.shapes[0].rect, original.shapes[0].rect);
        assert_ne!(duplicate.shapes[0].uid, original.shapes[0].uid);
        assert!(duplicate.hierarchy.components.values().all(|binding| !binding.master));
        assert_eq!(execute(w, &edit, window, cx).unwrap(), result);
        assert_eq!(w.document_revision(), before_revision + 1);

        let invalid = edit_pages(w, json!([
            {"op":"rename","id":original_id,"name":"Should roll back"},
            {"op":"reorder","id":copy,"before":"missing"}
        ]));
        let snapshot = w.agent_pages(cx);
        let error = execute(w, &invalid, window, cx).unwrap_err();
        assert_eq!(error.code, "PAGE_NOT_FOUND");
        assert_eq!(error.operation, Some(1));
        assert!(w.agent_pages(cx) == snapshot);
        w.undo_redo(false, window, cx);
        assert_eq!(w.pages.entries.len(), 1);
        assert!(w.snapshot_page(cx).0 == original);
        assert_eq!(execute(w, &edit, window, cx).unwrap(), result);
        assert_eq!(w.pages.entries.len(), 1, "retries acknowledge history without reapplying it");
        w.undo_redo(true, window, cx);
        assert!(w.agent_pages(cx) == snapshot);

        let delete_all = edit_pages(w, json!(w.pages.entries.iter().map(|state|
            json!({"op":"delete","id":state.page.id})).collect::<Vec<_>>()));
        assert_eq!(execute(w, &delete_all, window, cx).unwrap_err().code, "INVALID_ARGUMENT");
        assert!(w.agent_pages(cx) == snapshot);
        let remove_active = edit_pages(w, json!([{"op":"delete","id":original_id}]));
        execute(w, &remove_active, window, cx).unwrap();
        assert_ne!(w.pages.active, original_id);
        w.undo_redo(false, window, cx);
        assert_eq!(w.pages.active, original_id);
        assert!(w.agent_pages(cx) == snapshot);
    }).unwrap();
}

#[gpui::test]
fn inactive_page_edits_and_captures_leave_live_content_selection_and_view_intact(
    cx: &mut TestAppContext,
) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |w, window, cx| {
        let active = w.pages.active.clone();
        let create_page = edit_pages(w, json!([{"op":"create","ref":"$background","name":"Background"}]));
        let result = execute(w, &create_page, window, cx).unwrap();
        let other = result["created"]["$background"].as_str().unwrap().to_owned();
        // Simulate a normal UI edit that has not yet been parked in pages.entries.
        let setup = apply(w, json!([{ "op":"create","ref":"$live","type":"rectangle",
            "properties":{"rect":{"x":5,"y":6,"width":30,"height":40}}}]));
        execute(w, &setup, window, cx).unwrap();
        w.shapes[0].rect.x += 1.;
        let shape = w.shapes[0].id;
        w.set_selection([shape].into(), cx);
        w.view.zoom = 0.75;
        w.view.pan = point(123., 456.);
        let live = w.snapshot_page(cx).0;
        let view = w.view_state();
        let selection = w.selection_ids();
        let edit = request(w, "document.apply", json!({"page_id":other,
            "base_revision":w.document_revision().to_string(),"operations":[
                {"op":"create","ref":"$frame","type":"frame","properties":{"rect":{"x":10,"y":20,"width":180,"height":180},"fill":"#FFFFFFFF"}},
                {"op":"create","ref":"$rect","type":"rectangle","parent":"$frame","properties":{"rect":{"x":30,"y":50,"width":80,"height":40},"fill":"#7456AAFF","rotation":25}},
                {"op":"create","ref":"$text","type":"text","parent":"$frame","properties":{"rect":{"x":20,"y":120,"width":150,"height":30},"content":"Background"}}
            ]}));
        let edited = execute(w, &edit, window, cx).unwrap();
        let node = edited["created"]["$frame"].clone();
        assert_eq!(w.pages.active, active);
        assert!(w.snapshot_page(cx).0 == live);
        assert_eq!(w.view_state(), view);
        assert_eq!(w.selection_ids(), selection);
        let read = request(w, "nodes.get", json!({"page_id":other,"revision":w.document_revision().to_string()}));
        assert_eq!(execute(w, &read, window, cx).unwrap()["total"], 3);
        let capture = request(w, "canvas.capture", json!({"page_id":other,"revision":w.document_revision().to_string(),"node_id":node}));
        let image = execute(w, &capture, window, cx).unwrap();
        assert_eq!(w.pages.active, active);
        assert_eq!(w.selection_ids(), selection);
        assert_eq!(w.view_state(), view);
        let focus = request(w, "selection.set", json!({"page_id":other,"revision":w.document_revision().to_string(),"ids":[node]}));
        assert_eq!(execute(w, &focus, window, cx).unwrap_err().code, "PAGE_NOT_ACTIVE");
        let revision = w.document_revision();
        let activate = request(w, "pages.activate", json!({"page_id":other,"revision":revision.to_string()}));
        execute(w, &activate, window, cx).unwrap();
        assert_eq!(w.document_revision(), revision);
        assert_eq!(execute(w, &capture, window, cx).unwrap(), image);
        let back = request(w, "pages.activate", json!({"page_id":active,"revision":revision.to_string()}));
        execute(w, &back, window, cx).unwrap();
        assert_eq!(w.view_state(), view);
        assert_eq!(w.selection_ids(), selection);
        assert!(w.snapshot_page(cx).0 == live);
        execute(w, &activate, window, cx).unwrap();
        assert_eq!(w.pages.active, active, "an activation retry must not switch again");
        w.undo_redo(false, window, cx);
        assert!(w.agent_page(&other, cx).node_ids().is_empty());
        assert!(w.snapshot_page(cx).0 == live);
        w.undo_redo(true, window, cx);
        assert_eq!(w.agent_page(&other, cx).node_ids().len(), 3);
    }).unwrap();
}

#[gpui::test]
fn page_mutations_check_revision_busy_state_and_receipt_capacity_before_changes(
    cx: &mut TestAppContext,
) {
    let handle = crate::editor::tests::open(cx);
    handle
        .update(cx, |w, window, cx| {
            let edit = edit_pages(w, json!([{"op":"create","ref":"$page","name":"Other"}]));
            let result = execute(w, &edit, window, cx).unwrap();
            let other = result["created"]["$page"].clone();
            let original = w.pages.active.clone();
            let stale = request(w, "pages.activate", json!({"page_id":other,"revision":"0"}));
            assert_eq!(
                execute(w, &stale, window, cx).unwrap_err().code,
                "REVISION_CONFLICT"
            );
            let activate = request(
                w,
                "pages.activate",
                json!({"page_id":other,"revision":w.document_revision().to_string()}),
            );
            w.history.borrow_mut().begin_preview();
            assert_eq!(execute(w, &activate, window, cx).unwrap_err().code, "BUSY");
            w.history.borrow_mut().end_preview(false);
            w.agent.receipt_bytes = p::MAX_RECEIPT_BYTES;
            assert_eq!(
                execute(w, &activate, window, cx).unwrap_err().code,
                "LIMIT_REACHED"
            );
            assert_eq!(w.pages.active, original);
            let rename = edit_pages(
                w,
                json!([{"op":"rename","id":original,"name":"Should not change"}]),
            );
            let before = w.agent_pages(cx);
            let revision = w.document_revision();
            assert_eq!(
                execute(w, &rename, window, cx).unwrap_err().code,
                "LIMIT_REACHED"
            );
            assert!(w.agent_pages(cx) == before);
            assert_eq!(w.document_revision(), revision);
            assert_eq!(execute(w, &edit, window, cx).unwrap(), result);
            let mut collision = activate;
            collision.request_id = edit.request_id;
            assert_eq!(
                execute(w, &collision, window, cx).unwrap_err().code,
                "REQUEST_ID_REUSED"
            );
        })
        .unwrap();
}

#[gpui::test]
fn inactive_master_edits_refresh_visible_instances_without_switching_pages(
    cx: &mut TestAppContext,
) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |w, window, cx| {
        let master_page = w.pages.active.clone();
        let setup = apply(w, json!([
            {"op":"create","ref":"$master","type":"rectangle","properties":{"rect":{"x":10,"y":20,"width":80,"height":40}}},
            {"op":"create_component","id":"$master","ref":"$component","name":"Card"}
        ]));
        let result = execute(w, &setup, window, cx).unwrap();
        let master = result["created"]["$master"].clone();
        let duplicate = edit_pages(w, json!([{"op":"duplicate","id":master_page,"ref":"$copy","name":"Instances"}]));
        let copied = execute(w, &duplicate, window, cx).unwrap();
        let instance_page = copied["created"]["$copy"].as_str().unwrap();
        let activate = request(w, "pages.activate", json!({"page_id":instance_page,"revision":w.document_revision().to_string()}));
        execute(w, &activate, window, cx).unwrap();
        let id = w.shapes[0].id;
        w.set_selection([id].into(), cx);
        w.view.pan = point(100., 200.);
        let view = w.view_state();
        let before = w.agent_pages(cx);
        let edit = request(w, "document.apply", json!({"page_id":master_page,
            "base_revision":w.document_revision().to_string(),"operations":[
                {"op":"update","id":master,"properties":{"fill":"#AB3456FF","radius":12}}
            ]}));
        let result = execute(w, &edit, window, cx).unwrap();
        assert_eq!(result["changed_pages"].as_array().unwrap().len(), 2);
        assert_eq!(w.pages.active, instance_page);
        assert_eq!(w.shapes[0].color, gpui::rgb(0xab3456));
        assert_eq!(w.shapes[0].radius, 12.);
        assert_eq!(w.selection_ids(), [id].into());
        assert_eq!(w.view_state(), view);
        w.undo_redo(false, window, cx);
        assert_eq!(w.pages.active, instance_page);
        assert!(w.agent_pages(cx) == before);
    }).unwrap();
}
