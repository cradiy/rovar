use super::*;
use gpui::TestAppContext;
use std::collections::BTreeSet;
mod pages;
mod structure;

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
    match this.agent_request(r, None, w, cx)? {
        p::Reply::PrepareImports(sources) => {
            let imports = crate::agent::imports::prepare(sources)?;
            this.agent_request(r, Some(&imports), w, cx)?.finish()
        }
        reply => reply.finish(),
    }
}

#[gpui::test]
fn imports_are_revision_guarded_atomic_and_owned_by_the_document(cx: &mut TestAppContext) {
    use base64::Engine as _;
    let directory = tempfile::tempdir().unwrap();
    let image_path = directory.path().join("photo.png");
    image::RgbaImage::from_pixel(8, 6, image::Rgba([20, 80, 120, 255]))
        .save(&image_path)
        .unwrap();
    let image_data =
        base64::engine::general_purpose::STANDARD.encode(std::fs::read(&image_path).unwrap());
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"6\"><rect width=\"8\" height=\"6\" fill=\"red\"/></svg>";
    let svg_path = directory.path().join("icon.svg");
    std::fs::write(&svg_path, svg).unwrap();
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |this, window, cx| {
        let import = |reference: &str, source: Value| json!({"op":"import","ref":reference,
            "parent":"$frame","rect":{"x":120,"y":230,"width":80,"height":60},"source":source});
        let operations = json!([
            {"op":"create","ref":"$frame","type":"frame","properties":{"rect":{"x":100,"y":200,"width":600,"height":600}}},
            import("$image_file", json!({"type":"image","source":{"type":"path","path":image_path}})),
            import("$image_data", json!({"type":"image","source":{"type":"data","base64":image_data}})),
            import("$svg_file", json!({"type":"svg","source":{"type":"path","path":svg_path}})),
            import("$svg_data", json!({"type":"svg","source":{"type":"data","svg":svg},"editable":true})),
            {"op":"set_interaction","id":"$image_data","trigger":"click","action":{"type":"back"}}
        ]);
        let stale = apply(this, operations.clone());
        let p::Reply::PrepareImports(sources) = this.agent_request(&stale, None, window, cx).unwrap() else { panic!("Expected background import"); };
        let prepared = crate::agent::imports::prepare(sources).unwrap();
        let edit = apply(this, json!([{"op":"create","ref":"$rect","type":"rectangle",
            "properties":{"rect":{"x":0,"y":0,"width":10,"height":10}}}]));
        execute(this, &edit, window, cx).unwrap();
        assert_eq!(this.agent_request(&stale, Some(&prepared), window, cx).err().unwrap().code, "REVISION_CONFLICT");
        let before = this.snapshot_page(cx).0;
        let revision = this.document_revision();
        let mut invalid_ops = operations.clone();
        invalid_ops.as_array_mut().unwrap().push(json!({"op":"set_interaction","id":"$image_data",
            "trigger":"click","action":{"type":"navigate","target":"$image_file"}}));
        let invalid = apply(this, invalid_ops);
        assert_eq!(execute(this, &invalid, window, cx).unwrap_err().operation, Some(6));
        assert!(this.snapshot_page(cx).0 == before);
        assert_eq!(this.document_revision(), revision);

        let valid = apply(this, operations);
        let result = execute(this, &valid, window, cx).unwrap();
        let nodes = result["nodes"].as_array().unwrap();
        assert_eq!(nodes.iter().filter(|n| n["type"] == "image").count(), 2);
        assert!(nodes.iter().any(|n| n["type"] == "bezier"));
        assert!(nodes.iter().any(|n| n["type"] == "svg" && n["asset"]["svg_editable"] == true));
        std::fs::remove_file(&image_path).unwrap();
        std::fs::remove_file(&svg_path).unwrap();
        // Acknowledged retries do not reload now-missing files or add undo entries.
        assert_eq!(execute(this, &valid, window, cx).unwrap(), result);
        this.undo_redo(false, window, cx);
        assert!(this.snapshot_page(cx).0 == before);
        this.undo_redo(true, window, cx);
        let (document, assets) = this.snapshot_document(valid.document_id.as_deref().unwrap(), cx).unwrap();
        let destination = directory.path().join("imported.rovar");
        crate::document::save(&destination, &document, &assets, &[], window.text_system()).unwrap();
        let loaded = crate::document::load(&destination).unwrap().into_document().unwrap();
        assert_eq!(loaded.pages[0].assets.len(), 3);
        for shape in &loaded.pages[0].shapes {
            if let Some(asset) = &shape.asset { asset.ensure_decoded().unwrap(); }
        }
        let original = this.snapshot_page(cx).0;
        let image_uid = result["created"]["$image_data"].as_str().unwrap();
        let svg_uid = result["created"]["$svg_data"].as_str().unwrap();
        let image_id = model::lookup(&original, image_uid, &BTreeMap::new()).unwrap();
        let svg_id = model::lookup(&original, svg_uid, &BTreeMap::new()).unwrap();
        let path = original.shapes.iter().find(|s| s.kind == ShapeKind::Bezier).unwrap();
        let path_id = path.id;
        let update = apply(this, json!([
            {"op":"update","id":image_uid,"properties":{"name":"Cover","rect":{"x":300,"y":350,"width":160,"height":120},"opacity":0.6,"rotation":15}},
            {"op":"update","id":svg_uid,"properties":{"rect":{"x":400,"y":500,"width":160,"height":120}}}
        ]));
        execute(this, &update, window, cx).unwrap();
        let edited = this.snapshot_page(cx).0;
        let image = edited.shapes.iter().find(|s| s.id == image_id).unwrap();
        let old_image = original.shapes.iter().find(|s| s.id == image_id).unwrap();
        assert_eq!(image.uid, old_image.uid);
        assert!(std::sync::Arc::ptr_eq(image.asset.as_ref().unwrap(), old_image.asset.as_ref().unwrap()));
        assert!(edited.hierarchy.interactions == original.hierarchy.interactions);
        assert_eq!((image.rect.x, image.rect.y, image.rect.width, image.rect.height), (200.,150.,160.,120.));
        let new_path = edited.shapes.iter().find(|s| s.id == path_id).unwrap();
        assert_eq!(new_path.uid, path.uid);
        assert!(new_path.nodes == path.nodes);
        assert_eq!((new_path.rect.x, new_path.rect.y, new_path.rect.width, new_path.rect.height), (300.,300.,160.,120.));
        let bounds = crate::scene::components::bounds(&edited, svg_id).unwrap();
        assert_eq!((bounds.x,bounds.y,bounds.width,bounds.height), (400.,500.,160.,120.));
        this.undo_redo(false, window, cx);
        assert!(this.snapshot_page(cx).0 == original);
        this.undo_redo(true, window, cx);
        this.shapes.iter_mut().find(|s| s.id == path_id).unwrap().layer.locked = true;
        let locked_before = this.snapshot_page(cx).0;
        let blocked = apply(this, json!([
            {"op":"update","id":image_uid,"properties":{"name":"Should roll back"}},
            {"op":"update","id":svg_uid,"properties":{"rect":{"x":0,"y":0,"width":80,"height":60}}}
        ]));
        assert_eq!(execute(this, &blocked, window, cx).unwrap_err().code, "LOCKED");
        assert!(this.snapshot_page(cx).0 == locked_before);
        this.shapes.iter_mut().find(|s| s.id == path_id).unwrap().layer.locked = false;
        let delete = apply(this, json!([{"op":"delete","id":image_uid},{"op":"delete","id":svg_uid}]));
        execute(this, &delete, window, cx).unwrap();
        assert!(!this.shapes.iter().any(|s| s.id == image_id || s.id == path_id));
        assert!(!this.hierarchy.interactions.contains_key(&image_id));
        this.undo_redo(false, window, cx);
        assert!(this.snapshot_page(cx).0 == edited);
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(8, 6, image::Rgba([240, 20, 40, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let source = json!({"type":"image","source":{"type":"data",
            "base64":base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())}});
        let replace = apply(this, json!([{"op":"replace_source","id":image_uid,"source":source}]));
        let replaced = execute(this, &replace, window, cx).unwrap();
        assert_eq!(replaced["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(replaced["nodes"][0]["id"], image_uid);
        let current = this.snapshot_page(cx).0;
        let old = edited.shapes.iter().find(|s| s.id == image_id).unwrap();
        let new = current.shapes.iter().find(|s| s.id == image_id).unwrap();
        assert!(old.layer == new.layer && old.rect == new.rect && old.name == new.name);
        assert!(current.hierarchy == edited.hierarchy);
        assert_ne!(old.asset.as_ref().unwrap().hash, new.asset.as_ref().unwrap().hash);
        assert_eq!(replaced["nodes"][0]["asset"]["hash"], new.asset.as_ref().unwrap().hash);
        let same = apply(this, replace.params["operations"].clone());
        assert_eq!(execute(this, &same, window, cx).unwrap()["changed"], false);
        let wrong = apply(this, json!([
            {"op":"update","id":image_uid,"properties":{"name":"Must roll back"}},
            {"op":"replace_source","id":image_uid,"source":{"type":"svg","source":{"type":"data","svg":svg}}}
        ]));
        assert_eq!(execute(this, &wrong, window, cx).unwrap_err().operation, Some(1));
        assert!(this.snapshot_page(cx).0 == current);
        this.undo_redo(false, window, cx);
        assert!(this.snapshot_page(cx).0 == edited);
        this.undo_redo(true, window, cx);
        assert!(this.snapshot_page(cx).0 == current);
    }).unwrap();
}

#[gpui::test]
fn interactions_resolve_targets_preserve_other_triggers_and_undo_atomically(
    cx: &mut TestAppContext,
) {
    use crate::scene::presentation::{Action, HoverExit, Transition};
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |this, window, cx| {
        let setup = apply(this, json!([
            {"op":"create","ref":"$frame","type":"frame","properties":{"rect":{"x":0,"y":0,"width":320,"height":200}}},
            {"op":"create","ref":"$button","type":"rectangle","parent":"$frame","properties":{"rect":{"x":10,"y":10,"width":40,"height":40}}},
            {"op":"set_start_frame","target":"$frame"},
            {"op":"set_interaction","id":"$button","trigger":"hover","action":{"type":"navigate","target":"$frame"},
                "hover_exit":{"type":"restore"},"transition":{"type":"dissolve","duration_ms":200}},
            {"op":"set_interaction","id":"$button","trigger":"click","action":{"type":"back"}}
        ]));
        let result = execute(this, &setup, window, cx).unwrap();
        let frame = this.boards[0].id;
        let button = this.shapes[0].id;
        let uid = result["created"]["$button"].clone();
        assert_eq!(result["start_frame_id"], result["created"]["$frame"]);
        let node = result["nodes"].as_array().unwrap().iter().find(|n| n["id"] == uid).unwrap();
        assert_eq!(node["interactions"]["hover"]["action"]["target"], result["created"]["$frame"]);
        let original = this.snapshot_page(cx).0;
        let revision = this.document_revision();
        let undo_len = this.history.borrow().undo_len();
        let invalid = apply(this, json!([
            {"op":"set_interaction","id":uid,"trigger":"click","action":{"type":"close_overlay"}},
            {"op":"set_interaction","id":uid,"trigger":"hover","action":{"type":"navigate","target":uid}}
        ]));
        assert_eq!(execute(this, &invalid, window, cx).unwrap_err().operation, Some(1));
        assert!(this.snapshot_page(cx).0 == original);
        assert_eq!(this.document_revision(), revision);
        assert_eq!(this.history.borrow().undo_len(), undo_len);
        let clear = apply(this, json!([{"op":"set_interaction","id":uid,"trigger":"click","action":{"type":"none"}}]));
        execute(this, &clear, window, cx).unwrap();
        let interaction = this.hierarchy.interactions[&button].clone();
        assert_eq!(interaction.click, None);
        assert_eq!(interaction.hover, Some(Action::Navigate { target: frame }));
        assert_eq!(interaction.hover_exit, HoverExit::Restore);
        assert_eq!(interaction.hover_transition, Transition::Dissolve { duration_ms: 200 });
        this.undo_redo(false, window, cx);
        assert!(this.snapshot_page(cx).0 == original);

        let property = apply(this, json!([{"op":"set_interaction","id":uid,"trigger":"click",
            "action":{"type":"set_property","target":uid,"property":{"property":"visible","value":false}}}]));
        let changed = execute(this, &property, window, cx).unwrap();
        let node = changed["nodes"].as_array().unwrap().iter().find(|n| n["id"] == uid).unwrap();
        assert_eq!(node["interactions"]["click"]["action"]["target"], uid);
        assert!(!this.shapes[0].layer.hidden);
        let before = this.snapshot_page(cx).0;
        let unsupported = apply(this, json!([
            {"op":"set_interaction","id":uid,"trigger":"click","action":{"type":"set_property","target":uid,"property":{"property":"opacity","value":0.5}}},
            {"op":"set_interaction","id":uid,"trigger":"hover","action":{"type":"set_property","target":uid,"property":{"property":"content","value":"Wrong target"}}}
        ]));
        assert_eq!(execute(this, &unsupported, window, cx).unwrap_err().operation, Some(1));
        assert!(this.snapshot_page(cx).0 == before);

        // Media nodes allow basic edits while unsupported playback commands remain rejected.
        this.shapes[0].kind = ShapeKind::Audio;
        this.shapes[0].asset = Some(std::sync::Arc::new(crate::asset::FileAsset {
            path: "audio.mp3".into(),
            source: crate::asset::Source::file(tempfile::NamedTempFile::new().unwrap().into_temp_path()),
            hash: "0".repeat(64), width: 40, height: 40,
            content: std::sync::OnceLock::from(Err("No decoder in authoring test".into())),
            video: false,
        }));
        let media = apply(this, json!([{"op":"set_interaction","id":uid,"trigger":"click",
            "action":{"type":"media","target":uid,"command":{"type":"toggle"}}}]));
        execute(this, &media, window, cx).unwrap();
        let before = this.snapshot_page(cx).0;
        let rate = apply(this, json!([{"op":"set_interaction","id":uid,"trigger":"click",
            "action":{"type":"media","target":uid,"command":{"type":"rate","percent":150}}}]));
        assert!(execute(this, &rate, window, cx).is_err());
        assert!(this.snapshot_page(cx).0 == before);
        let geometry = apply(this, json!([{"op":"update","id":uid,"properties":{"opacity":0.5}}]));
        execute(this, &geometry, window, cx).unwrap();
        assert_eq!(this.shapes[0].layer.opacity, 0.5);
        this.shapes[0].layer.locked = true;
        let locked = apply(this, json!([{"op":"set_interaction","id":uid,"trigger":"click","action":{"type":"back"}}]));
        assert_eq!(execute(this, &locked, window, cx).unwrap_err().code, "LOCKED");
        this.shapes[0].layer.locked = false;

        // Variant choices come from the source component's own set, not arbitrary UUIDs.
        this.set_selection(BTreeSet::from([frame]), cx);
        this.create_component(window, cx);
        let component = this.hierarchy.components[&frame].component.clone();
        let variant = uuid::Uuid::new_v4().to_string();
        let definition = this.components.definitions[&component].clone();
        this.components.definitions.insert(variant.clone(), definition);
        this.components.sets.insert(uuid::Uuid::new_v4().to_string(), crate::scene::components::variants::ComponentSet {
            name: "Button".into(), variants: BTreeMap::from([(component, "Default".into()), (variant.clone(), "Active".into())]),
        });
        let read = request(this, "nodes.get", json!({"page_id":this.pages.active,"revision":this.document_revision().to_string(),"ids":[uid]}));
        let nodes = execute(this, &read, window, cx).unwrap();
        assert_eq!(nodes["nodes"][0]["available_variants"][0]["id"], variant);
        let change = apply(this, json!([{"op":"set_interaction","id":uid,"trigger":"hover",
            "action":{"type":"change_variant","target":variant},"hover_exit":{"type":"restore"}}]));
        execute(this, &change, window, cx).unwrap();
        let before = this.snapshot_page(cx).0;
        let invalid = apply(this, json!([{"op":"set_interaction","id":uid,"trigger":"hover",
            "action":{"type":"change_variant","target":uuid::Uuid::new_v4()}}]));
        assert!(execute(this, &invalid, window, cx).is_err());
        assert!(this.snapshot_page(cx).0 == before);
    }).unwrap();
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
fn removing_group_layout_and_setting_rect_transforms_children(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |this, window, cx| {
        let create = apply(this, json!([
            {"op":"create","ref":"$group","type":"group","properties":{
                "rect":{"x":100,"y":200,"width":200,"height":150},
                "layout":{"direction":"vertical","gap":10,"padding":[10,10,10,10],"main_align":"start","cross_align":"start"}}},
            {"op":"create","ref":"$child","type":"rectangle","parent":"$group","properties":{
                "rect":{"x":0,"y":0,"width":100,"height":40}}}
        ]));
        let created = execute(this, &create, window, cx).unwrap();
        let before = this.snapshot_page(cx).0;
        let update = apply(this, json!([{"op":"update","id":created["created"]["$group"],
            "properties":{"remove_layout":true,"rect":{"x":300,"y":400,"width":250,"height":80}}}]));
        execute(this, &update, window, cx).unwrap();
        assert!(this.hierarchy.layouts.is_empty());
        assert_eq!(this.shapes[0].rect, Rect { x: 300., y: 400., width: 250., height: 80. });
        this.undo_redo(false, window, cx);
        assert!(this.snapshot_page(cx).0 == before);
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
                "rect":{"x":0,"y":0,"width":100,"height":40},"content":"登录","font_size":16,
                "font_weight":700,"italic":true,"underline":true,"strikethrough":true}}
        ]));
        let result=execute(this,&create,window,cx).unwrap();
        assert_eq!(this.texts[0].rect.x,110.);
        assert_eq!(this.texts[0].rect.y,210.);
        assert_eq!(this.snapshot_page(cx).0.texts[0].content,"登录");
        let read=request(this,"nodes.get",json!({"page_id":this.pages.active,
            "revision":result["revision"]}));
        let nodes=execute(this,&read,window,cx).unwrap();
        let text=nodes["nodes"].as_array().unwrap().iter()
            .find(|node| node["properties"]["content"]=="登录").unwrap();
        for key in ["italic","underline","strikethrough"] {
            assert_eq!(text["properties"][key],true);
        }
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
