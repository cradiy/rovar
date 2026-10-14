use super::*;
use gpui::TestAppContext;

fn request(this: &Workspace, method: &str, params: Value) -> Request {
    Request {
        request_id: uuid::Uuid::new_v4().to_string(),
        session_id: Some(this.agent.session.clone()),
        document_id: Some("adad2458-d362-4053-9bb9-074715156c47".into()),
        method: method.into(),
        params,
    }
}
fn apply(this: &Workspace, ops: Value) -> Request {
    request(
        this,
        "document.apply",
        json!({"page_id":this.pages.active,
        "base_revision":this.document_revision().to_string(),"operations":ops}),
    )
}
fn execute(
    this: &mut Workspace,
    r: &Request,
    w: &mut Window,
    cx: &mut Context<Workspace>,
) -> Result<Value> {
    this.agent_request(r, w, cx)?.finish()
}

#[gpui::test]
fn batches_rollback_retry_and_share_the_editors_undo_history(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|this,window,cx| {
        let frame=json!({"op":"create","ref":"$frame","type":"frame","parent":null,
            "properties":{"name":"Card","rect":{"x":100,"y":200,"width":240,"height":160},"fill":"#FFFFFFFF"}});
        let shape=json!({"op":"create","ref":"$shape","type":"rectangle","parent":"$frame",
            "properties":{"rect":{"x":120,"y":220,"width":60,"height":40},"fill":"#FF0000FF"}});
        let bad=apply(this,json!([frame,shape,{"op":"update","id":"$shape","properties":{"radius":-1}}]));
        let original=this.snapshot_page(cx).0;
        let revision=this.document_revision();
        assert_eq!(execute(this,&bad,window,cx).unwrap_err().operation,Some(2));
        assert!(this.snapshot_page(cx).0==original);
        assert_eq!(this.document_revision(),revision);
        assert!(!this.history.borrow().can_undo());

        let good=apply(this,json!([frame,shape]));
        let result=execute(this,&good,window,cx).unwrap();
        assert_eq!(this.shapes[0].rect.x,20.);
        assert_eq!(this.shapes[0].rect.y,20.);
        assert_eq!(this.history.borrow().undo_len(),1);
        assert_eq!(execute(this,&good,window,cx).unwrap(),result);
        let mut changed_body=good.clone();
        changed_body.params["operations"][0]["properties"]["name"]=json!("Different");
        assert_eq!(execute(this,&changed_body,window,cx).unwrap_err().code,"REQUEST_ID_REUSED");
        let mut stale=good.clone();
        stale.request_id="stale".into();
        assert_eq!(execute(this,&stale,window,cx).unwrap_err().code,"REVISION_CONFLICT");

        let mut read=request(this,"nodes.get",json!({"page_id":this.pages.active,
            "revision":result["revision"],"limit":1}));
        let first=execute(this,&read,window,cx).unwrap();
        assert_eq!(first["total"],2);
        assert_eq!(first["nodes"].as_array().unwrap().len(),1);
        read.params["offset"]=first["next_offset"].clone();
        let second=execute(this,&read,window,cx).unwrap();
        assert!(second["next_offset"].is_null());
        assert_eq!(second["nodes"][0]["properties"]["rect"]["x"],120.);

        let capture=request(this,"canvas.capture",json!({"page_id":this.pages.active,
            "revision":this.document_revision().to_string(),"node_id":result["created"]["$frame"]}));
        let image=execute(this,&capture,window,cx).unwrap();
        use base64::Engine as _;
        let bytes=base64::engine::general_purpose::STANDARD.decode(image["data_base64"].as_str().unwrap()).unwrap();
        let pixels=image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(pixels.dimensions(),(240,160));
        assert_eq!(pixels.get_pixel(30,30).0,[255,0,0,255]);
        assert_eq!(image["canvas_rect"]["x"],100.);
        assert_eq!(image["revision"],result["revision"]);

        this.undo_redo(false,window,cx);
        assert!(this.boards.is_empty() && this.shapes.is_empty());
        // A receipt acknowledges a previous commit; it must not redo an undone request.
        assert_eq!(execute(this,&good,window,cx).unwrap(),result);
        assert!(this.boards.is_empty());
        this.undo_redo(true,window,cx);
        assert_eq!(this.boards[0].uid.to_string(),result["created"]["$frame"].as_str().unwrap());
        assert_eq!(this.shapes.len(),1);
    }).unwrap();
}

#[gpui::test]
fn layout_edits_protect_locked_descendants_and_preserve_text(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|this,window,cx| {
        let layout=json!({"direction":"vertical","gap":10,"padding":[10,10,10,10],"main_align":"start","cross_align":"start"});
        let create=apply(this,json!([
            {"op":"create","ref":"$group","type":"group","properties":{
                "rect":{"x":100,"y":200,"width":200,"height":150},"layout":layout}},
            {"op":"create","ref":"$text","type":"text","parent":"$group","properties":{
                "rect":{"x":0,"y":0,"width":100,"height":40},"content":"登录","font_size":16}}
        ]));
        let result=execute(this,&create,window,cx).unwrap();
        assert_eq!(this.texts[0].rect.x,110.);
        assert_eq!(this.texts[0].rect.y,210.);
        assert_eq!(this.snapshot_page(cx).0.texts[0].content,"登录");
        let capture=request(this,"canvas.capture",json!({"page_id":this.pages.active,
            "revision":result["revision"],"node_id":result["created"]["$text"]}));
        let image=execute(this,&capture,window,cx).unwrap();
        use base64::Engine as _;
        let bytes=base64::engine::general_purpose::STANDARD.decode(image["data_base64"].as_str().unwrap()).unwrap();
        let pixels=image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert!(pixels.pixels().any(|pixel| pixel.0[3]>100),"Capture must include text glyphs");
        this.texts[0].layer.locked=true;
        this.history.borrow_mut().mark_changed();
        let before=this.snapshot_page(cx).0;
        let revision=this.document_revision();
        let mut moved_layout=layout.clone();
        moved_layout["padding"]=json!([30,30,30,30]);
        let update=apply(this,json!([{"op":"update","id":result["created"]["$group"],
            "properties":{"layout":moved_layout}}]));
        assert_eq!(execute(this,&update,window,cx).unwrap_err().code,"LOCKED");
        assert!(this.snapshot_page(cx).0==before);
        assert_eq!(this.document_revision(),revision);
        let delete=apply(this,json!([{"op":"delete","id":result["created"]["$group"]}]));
        assert_eq!(execute(this,&delete,window,cx).unwrap_err().code,"LOCKED");
        assert_eq!(this.history.borrow().undo_len(),1);
        this.texts[0].editor.update(cx,|text,_|text.editing=true);
        assert_eq!(execute(this,&delete,window,cx).unwrap_err().code,"BUSY");
    }).unwrap();
}

#[gpui::test]
fn selection_and_framing_preserve_document_and_reject_invalid_targets(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |this, window, cx| {
        this.sidebar.collapsed = false;
        this.bounds.set(Bounds::new(point(px(0.), px(0.)), size(px(1280.), px(800.))));
        let create = apply(this, json!([
            {"op":"create","ref":"$a","type":"rectangle","properties":{
                "rect":{"x":2000,"y":1000,"width":1000,"height":600},"rotation":30}},
            {"op":"create","ref":"$b","type":"rectangle","properties":{
                "rect":{"x":-5000,"y":-5000,"width":100,"height":100},"hidden":true}}
        ]));
        let created = execute(this, &create, window, cx).unwrap();
        let before = this.snapshot_page(cx).0;
        let revision = this.document_revision();
        let undo = this.history.borrow().undo_len();
        let params = json!({"page_id":this.pages.active,"revision":revision.to_string(),"ids":[created["created"]["$a"]]});
        let select = request(this, "selection.set", params.clone());
        let selected = execute(this, &select, window, cx).unwrap();
        assert_eq!(selected["selection"], params["ids"]);
        let original_view = this.view;
        for (id, code) in [(created["created"]["$b"].clone(), "NOT_SELECTABLE"), (json!(uuid::Uuid::new_v4()), "NODE_NOT_FOUND")] {
            let mut bad = select.clone();
            bad.params["ids"].as_array_mut().unwrap().push(id);
            assert_eq!(execute(this, &bad, window, cx).unwrap_err().code, code);
            assert_eq!(this.selection_ids().len(), 1);
            assert_eq!(this.view, original_view);
        }
        let mut stale = select.clone();
        stale.params["revision"] = json!("0");
        stale.params["ids"] = json!([]);
        assert_eq!(execute(this, &stale, window, cx).unwrap_err().code, "REVISION_CONFLICT");
        let focus = request(this, "canvas.focus", params.clone());
        execute(this, &focus, window, cx).unwrap();
        assert_eq!(this.view.zoom, original_view.zoom);
        assert_ne!(this.view.pan, original_view.pan);
        let fit = request(this, "canvas.fit", params.clone());
        let fitted = execute(this, &fit, window, cx).unwrap();
        let bounds = this.world_bounds(this.shapes[0].id).unwrap();
        let tl = this.view.screen(point(bounds.x, bounds.y));
        let br = this.view.screen(point(bounds.x + bounds.width, bounds.y + bounds.height));
        let (left, right) = this.canvas_insets();
        assert!(tl.x >= left && br.x <= 1280. - right);
        assert!(tl.y >= 24. && br.y <= 728.);
        assert_eq!(fitted["selection"], selected["selection"]);
        let fitted_view = this.view;
        let mut all = fit.clone();
        all.params["ids"] = json!([]);
        execute(this, &all, window, cx).unwrap();
        assert_eq!(this.view, fitted_view, "Fit all ignores hidden content");
        let mut hidden = focus.clone();
        hidden.params["ids"] = json!([created["created"]["$b"]]);
        assert_eq!(execute(this, &hidden, window, cx).unwrap_err().code, "NOT_VISIBLE");
        let mut clear = select;
        clear.params["ids"] = json!([]);
        execute(this, &clear, window, cx).unwrap();
        assert!(this.selection_ids().is_empty());
        assert_eq!(this.view, fitted_view);
        assert!(this.snapshot_page(cx).0 == before);
        assert_eq!(this.document_revision(), revision);
        assert_eq!(this.history.borrow().undo_len(), undo);
    }).unwrap();
}
