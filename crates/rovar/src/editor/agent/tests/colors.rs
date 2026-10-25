use super::*;

fn solid(color: &str) -> Value {
    json!({"type":"solid","color":color})
}

#[gpui::test]
fn color_style_bindings_target_layers_and_unicode_ranges_and_delete_without_repainting(
    cx: &mut TestAppContext,
) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|w,window,cx|{
        let setup=apply(w,json!([
            {"op":"create_color_style","ref":"$accent","name":"Accent","paint":solid("#8855BB")},
            {"op":"create","ref":"$r","type":"rectangle","properties":{"rect":{"x":0,"y":0,"width":80,"height":80},"fills":[{"paint":solid("#FFFFFF")},{"paint":solid("#000000"),"opacity":0.4,"hidden":true}],"strokes":[{"paint":solid("#333333"),"width":3}]}},
            {"op":"create","ref":"$t","type":"text","properties":{"rect":{"x":0,"y":100,"width":200,"height":80},"content":"A中🙂B","fill":"#112233FF"}},
            {"op":"bind_color_style","id":"$r","target":{"type":"fill","index":1},"style":"$accent"},
            {"op":"bind_color_style","id":"$r","target":{"type":"stroke","index":0},"style":"$accent"},
            {"op":"bind_color_style","id":"$t","target":{"type":"text","start":1,"end":3},"style":"$accent"}
        ]));
        let result=execute(w,&setup,window,cx).unwrap();
        let style=result["created_color_styles"]["$accent"].clone();
        let ids=&result["created"];
        let page=w.snapshot_page(cx).0;
        assert_eq!(page.shapes[0].color,gpui::rgb(0xffffff));
        assert_eq!(page.shapes[0].fill_layers[0].color_style.as_deref(),style.as_str());
        assert_eq!(page.texts[0].styles.at(1).color_style.as_deref(),style.as_str());
        assert!(page.texts[0].styles.at(0).color_style.is_none());
        let text=result["nodes"].as_array().unwrap().iter().find(|n|n["id"]==ids["$t"]).unwrap();
        assert!(text["text"]["runs"].as_array().unwrap().iter().any(|r|r["start"]==1 && r["end"]==3 && r["color_style"]==style));
        let gradient=json!({"type":"gradient","angle":40,"stops":[{"position":0,"color":"#FF0000"},{"position":1,"color":"#0000FF80"}]});
        let update=apply(w,json!([{"op":"update_color_style","id":style,"paint":gradient}]));
        execute(w,&update,window,cx).unwrap();
        let changed=w.snapshot_page(cx).0;
        assert_eq!(changed.shapes[0].fill_layers[0].fill_mode,FillMode::Linear);
        assert_eq!(changed.shapes[0].fill_layers[0].opacity,0.4);
        assert!(changed.shapes[0].fill_layers[0].hidden);
        assert_eq!(changed.shapes[0].stroke.fill_mode,FillMode::Linear);
        assert_eq!(changed.texts[0].styles.at(1).fill_mode,FillMode::Linear);
        assert_eq!(changed.texts[0].styles.at(0).color,gpui::rgb(0x112233));
        let list=request(w,"colors.list",json!({}));
        let listed=execute(w,&list,window,cx).unwrap();
        let noop=apply(w,json!([{"op":"update_color_style","id":style,"paint":listed["styles"][0]["paint"]}]));
        assert_eq!(execute(w,&noop,window,cx).unwrap()["changed"],false);
        let detach=apply(w,json!([{"op":"bind_color_style","id":ids["$t"],"target":{"type":"text","start":1,"end":3},"style":null}]));
        execute(w,&detach,window,cx).unwrap();
        let detached=w.snapshot_page(cx).0;
        assert!(detached.texts[0].styles.at(1).color_style.is_none());
        assert!(detached.texts[0].styles.at(1).gradient==changed.texts[0].styles.at(1).gradient);
        let delete=apply(w,json!([{"op":"delete_color_style","id":style}]));
        execute(w,&delete,window,cx).unwrap();
        assert!(w.colors.palette.is_empty());
        let deleted=w.snapshot_page(cx).0;
        assert!(deleted.shapes[0].fill_layers[0].color_style.is_none());
        assert!(deleted.shapes[0].fill_layers[0].gradient==changed.shapes[0].fill_layers[0].gradient);
        w.undo_redo(false,window,cx);
        assert!(w.colors.palette.contains_key(style.as_str().unwrap()));
        assert!(w.snapshot_page(cx).0==detached);
        let (bytes,sources)=w.snapshot_document(setup.document_id.as_ref().unwrap(),cx).unwrap();
        let directory=tempfile::tempdir().unwrap();
        let path=directory.path().join("styled.rovar");
        crate::document::save_as(&path,&bytes,&sources,cx.text_system()).unwrap();
        let loaded=crate::document::load(&path).unwrap().into_document().unwrap();
        assert_eq!(loaded.colors,w.colors.palette);
        assert_eq!(loaded.pages[0].shapes[0].fill_layers[0].color_style.as_deref(),style.as_str());
    }).unwrap();
}

#[gpui::test]
fn palette_only_changes_are_undoable_and_failed_batches_leave_no_style(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|w,window,cx|{
        let setup=apply(w,json!([{"op":"create_color_style","ref":"$unused","name":"Unused","paint":solid("#123456")} ]));
        let result=execute(w,&setup,window,cx).unwrap();
        assert_eq!(result["changed"],true);
        assert_eq!(result["changed_pages"],json!([]));
        assert_eq!(execute(w,&setup,window,cx).unwrap(),result);
        w.undo_redo(false,window,cx);
        assert!(w.colors.palette.is_empty());
        w.undo_redo(true,window,cx);
        assert_eq!(w.colors.palette.len(),1);
        let before=w.snapshot_page(cx).0;
        let palette=w.colors.palette.clone();
        let revision=w.document_revision();
        let invalid=apply(w,json!([
            {"op":"create_color_style","ref":"$style","name":"Invalid batch","paint":solid("#FF0000")},
            {"op":"create","ref":"$rect","type":"rectangle","properties":{"rect":{"x":0,"y":0,"width":40,"height":40}}},
            {"op":"bind_color_style","id":"$rect","target":{"type":"fill","index":10},"style":"$style"}
        ]));
        assert_eq!(execute(w,&invalid,window,cx).unwrap_err().operation,Some(2));
        assert!(w.snapshot_page(cx).0==before);
        assert_eq!(w.colors.palette,palette);
        assert_eq!(w.document_revision(),revision);
    }).unwrap();
}

#[gpui::test]
fn shared_styles_refresh_component_baselines_and_preserve_overrides_across_pages(
    cx: &mut TestAppContext,
) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|w,window,cx|{
        let setup=apply(w,json!([
            {"op":"create_color_style","ref":"$style","name":"Accent","paint":solid("#8855BB")},
            {"op":"create","ref":"$master","type":"rectangle","properties":{"rect":{"x":0,"y":0,"width":80,"height":80}}},
            {"op":"bind_color_style","id":"$master","target":{"type":"fill","index":0},"style":"$style"},
            {"op":"create_component","id":"$master","ref":"$component","name":"Card"},
            {"op":"instantiate_component","component":"$component","ref":"$override","position":{"x":100,"y":0}},
            {"op":"update","id":"$override","properties":{"fill":"#FF0000FF"}}
        ]));
        let result=execute(w,&setup,window,cx).unwrap();
        let style=result["created_color_styles"]["$style"].clone();
        let component=result["created_components"]["$component"].as_str().unwrap().to_string();
        let mut other=crate::document::Page::empty("Other".into());
        let id=super::super::structure::instantiate(&mut other,&w.components.definitions[&component],component,None,p::Point{x:0.,y:0.}).unwrap();
        let page_id=other.id.clone();
        other.shapes.iter_mut().find(|s|s.id==id).unwrap().layer.locked=true;
        w.pages.entries.push(crate::editor::pages::PageState::new(other));
        let before=w.snapshot_page(cx).0;
        let palette=w.colors.palette.clone();
        let update=apply(w,json!([{"op":"update_color_style","id":style,"paint":solid("#00FF00")} ]));
        assert_eq!(execute(w,&update,window,cx).unwrap_err().code,"LOCKED");
        assert!(w.snapshot_page(cx).0==before);
        assert_eq!(w.colors.palette,palette);
        w.pages.entries.iter_mut().find(|s|s.page.id==page_id).unwrap().page.shapes.iter_mut().find(|s|s.id==id).unwrap().layer.locked=false;
        let before_pages=w.agent_pages(cx);
        let changed=execute(w,&update,window,cx).unwrap();
        assert!(changed["changed_pages"].as_array().unwrap().contains(&json!(page_id)));
        assert_eq!(w.shapes[0].color,gpui::rgb(0x00ff00));
        assert_eq!(w.shapes[1].color,gpui::rgb(0xff0000));
        let other=&w.pages.entries.iter().find(|s|s.page.id==page_id).unwrap().page;
        assert_eq!(other.shapes[0].color,gpui::rgb(0x00ff00));
        let baseline:crate::document::Page=serde_json::from_value(other.hierarchy.components[&id].baseline.clone()).unwrap();
        assert_eq!(baseline.shapes[0].color,gpui::rgb(0x00ff00));
        let resize=apply(w,json!([{"op":"update","id":result["created"]["$master"],"properties":{"radius":12}}]));
        execute(w,&resize,window,cx).unwrap();
        assert_eq!(w.shapes[1].color,gpui::rgb(0xff0000));
        assert_eq!(w.shapes[1].radius,12.);
        w.undo_redo(false,window,cx);
        w.undo_redo(false,window,cx);
        assert!(w.agent_pages(cx)==before_pages);
        assert_eq!(w.colors.palette,palette);
    }).unwrap();
}
