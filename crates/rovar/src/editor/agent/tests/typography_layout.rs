use super::*;

#[gpui::test]
fn unicode_range_edits_preserve_other_styles_expand_paragraphs_and_undo(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|w,window,cx|{
        let setup=apply(w,json!([{"op":"create","type":"text","ref":"$text","properties":{
            "rect":{"x":0,"y":0,"width":320,"height":160},"content":"A中🙂\n第二段","font_size":24,"fill":"#223344FF"}}]));
        let created=execute(w,&setup,window,cx).unwrap();
        let uid=created["created"]["$text"].clone();
        let before=w.snapshot_page(cx).0;
        let edit=apply(w,json!([
            {"op":"edit_text","id":uid,"start":1,"end":3,"style":{"font_weight":700,"italic":true,"paint":{"type":"gradient","stops":[{"position":0,"color":"#FF0000"},{"position":1,"color":"#0000FF"}]}}},
            {"op":"edit_text","id":uid,"start":1,"end":2,"style":{"line_height":2,"text_align":"center"}},
            {"op":"edit_text","id":uid,"start":0,"end":1,"content":"Hello "}
        ]));
        let edited=execute(w,&edit,window,cx).unwrap();
        let page=w.snapshot_page(cx).0;
        let text=&page.texts[0];
        assert_eq!(text.content,"Hello 中🙂\n第二段");
        assert_eq!(text.styles.at(6).weight,700.);
        assert!(text.styles.at(6).italic);
        assert_eq!(text.styles.at(6).fill_mode,FillMode::Linear);
        assert_eq!(text.styles.at(0).weight,400.);
        assert_eq!(text.styles.at(0).line_height,2.);
        assert_eq!(text.styles.at(0).align,gpui::TextAlign::Center);
        let second=text.content.find('第').unwrap();
        assert_eq!(text.styles.at(second).line_height,1.5);
        assert_eq!(text.styles.at(second).align,gpui::TextAlign::Left);
        assert_eq!(edited["nodes"][0]["text"]["length"],12);
        assert!(edited["nodes"][0]["text"]["runs"].as_array().unwrap().iter().any(|r|r["start"]==6 && r["end"]==8 && r["style"]["font_weight"]==700.));
        let unchanged=apply(w,json!([{"op":"update","id":uid,"properties":{"content":text.content}}]));
        assert_eq!(execute(w,&unchanged,window,cx).unwrap()["changed"],false);
        w.undo_redo(false,window,cx);
        assert!(w.snapshot_page(cx).0==before);
        w.undo_redo(true,window,cx);
        assert!(w.snapshot_page(cx).0==page);
        let capture=request(w,"canvas.capture",json!({"page_id":w.pages.active,"revision":w.document_revision().to_string(),"node_id":uid}));
        assert!(execute(w,&capture,window,cx).unwrap()["data_base64"].is_string());
        let revision=w.document_revision();
        for op in [
            json!({"op":"edit_text","id":uid,"start":0,"end":99,"content":"x"}),
            json!({"op":"edit_text","id":uid,"start":0,"end":1,"style":{"line_height":0.1}}),
            json!({"op":"edit_text","id":uid,"start":0,"end":1,"style":{"font_family":"Missing family for protocol regression"}})
        ] {
            let invalid=apply(w,json!([{"op":"edit_text","id":uid,"start":0,"end":1,"content":"x"},op]));
            assert_eq!(execute(w,&invalid,window,cx).unwrap_err().operation,Some(1));
            assert!(w.snapshot_page(cx).0==page);
            assert_eq!(w.document_revision(),revision);
        }
    }).unwrap();
}

fn layout(direction: &str) -> Value {
    json!({"direction":direction,"gap":10,"line_gap":20,"padding":[10,10,10,10],"main_align":"start","cross_align":"start"})
}

#[gpui::test]
fn grid_spans_limits_wrapping_and_hug_use_the_editor_layout_engine(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|w,window,cx|{
        let mut grid=layout("grid"); grid["columns"]=json!(3); grid["column_width"]=json!(90);
        let setup=apply(w,json!([
            {"op":"create","type":"frame","ref":"$f","properties":{"rect":{"x":100,"y":100,"width":320,"height":200},"layout":grid,"sizing":{"width":"fixed","height":"hug"}}},
            {"op":"create","type":"rectangle","ref":"$a","parent":"$f","properties":{"rect":{"x":100,"y":100,"width":40,"height":40},"sizing":{"width":"fill","height":"fixed","column_span":2,"max_width":160}}},
            {"op":"create","type":"rectangle","ref":"$b","parent":"$f","properties":{"rect":{"x":100,"y":100,"width":40,"height":40},"sizing":{"width":"fill","height":"fixed","column_span":2}}}
        ]));
        let created=execute(w,&setup,window,cx).unwrap();
        let ids=&created["created"];
        let page=w.snapshot_page(cx).0;
        assert_eq!(page.shapes[0].rect.width,160.);
        assert_eq!(page.shapes[1].rect.width,190.);
        assert_eq!(page.shapes[1].rect.y-page.shapes[0].rect.y,60.);
        assert_eq!(page.boards[0].rect.height,120.);
        let board=created["nodes"].as_array().unwrap().iter().find(|n|n["id"]==ids["$f"]).unwrap();
        let unchanged=apply(w,json!([{"op":"update","id":ids["$f"],"properties":{"layout":board["properties"]["layout"],"sizing":board["properties"]["sizing"]}}]));
        assert_eq!(execute(w,&unchanged,window,cx).unwrap()["changed"],false);
        let mut wrapped=layout("horizontal"); wrapped["wrap"]=json!(true);
        let edit=apply(w,json!([
            {"op":"update","id":ids["$f"],"properties":{"layout":wrapped}},
            {"op":"update","id":ids["$a"],"properties":{"sizing":{"width":"fixed","height":"fixed"}}},
            {"op":"update","id":ids["$b"],"properties":{"sizing":{"width":"fixed","height":"fixed"}}}
        ]));
        execute(w,&edit,window,cx).unwrap();
        let wrapped=w.snapshot_page(cx).0;
        assert_eq!(wrapped.shapes[1].rect.y-wrapped.shapes[0].rect.y,60.);
        w.undo_redo(false,window,cx);
        assert!(w.snapshot_page(cx).0==page);
        let invalid=apply(w,json!([{"op":"update","id":ids["$f"],"properties":{"layout":layout("horizontal"),"sizing":{"width":"hug","height":"hug"}}}]));
        assert!(execute(w,&invalid,window,cx).is_err(),"Hug parent and Fill child form a sizing cycle");
        assert!(w.snapshot_page(cx).0==page);
    }).unwrap();
}

#[gpui::test]
fn constraints_resize_children_and_protect_locked_descendants(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|w,window,cx|{
        let setup=apply(w,json!([
            {"op":"create","type":"frame","ref":"$f","properties":{"rect":{"x":100,"y":100,"width":300,"height":200}}},
            {"op":"create","type":"rectangle","ref":"$child","parent":"$f","properties":{"rect":{"x":120,"y":120,"width":100,"height":40},"constraints":{"horizontal":"stretch","vertical":"end"}}}
        ]));
        let created=execute(w,&setup,window,cx).unwrap();
        let ids=&created["created"];
        let before=w.snapshot_page(cx).0;
        let resize=apply(w,json!([{"op":"update","id":ids["$f"],"properties":{"rect":{"x":100,"y":100,"width":400,"height":300}}}]));
        execute(w,&resize,window,cx).unwrap();
        let page=w.snapshot_page(cx).0;
        assert_eq!((page.shapes[0].rect.width,page.shapes[0].rect.y),(200.,120.));
        let unchanged=apply(w,json!([{"op":"update","id":ids["$child"],"properties":{"constraints":{"horizontal":"stretch","vertical":"end"}}}]));
        assert_eq!(execute(w,&unchanged,window,cx).unwrap()["changed"],false);
        w.undo_redo(false,window,cx);
        assert!(w.snapshot_page(cx).0==before);
        w.shapes[0].layer.locked=true;
        let locked=w.snapshot_page(cx).0;
        let resize=apply(w,resize.params["operations"].clone());
        assert_eq!(execute(w,&resize,window,cx).unwrap_err().code,"LOCKED");
        assert!(w.snapshot_page(cx).0==locked);
        w.shapes[0].layer.locked=false;
        let flow=apply(w,json!([
            {"op":"update","id":ids["$child"],"properties":{"remove_constraints":true}},
            {"op":"update","id":ids["$f"],"properties":{"layout":layout("horizontal")}},
            {"op":"update","id":ids["$child"],"properties":{"sizing":{"width":"fill","height":"fixed"}}}
        ]));
        execute(w,&flow,window,cx).unwrap();
        assert_eq!(w.shapes[0].rect.width,280.);
    }).unwrap();
}
