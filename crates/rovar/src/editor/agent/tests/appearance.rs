use super::*;

fn solid(color: &str) -> Value {
    json!({"paint":{"type":"solid","color":color}})
}

#[gpui::test]
fn appearance_stacks_roundtrip_incrementally_and_undo_with_their_effects(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |w, window, cx| {
        let gradient = json!({"type":"gradient","kind":"linear","angle":45,"stops":[
            {"position":0,"color":"#FF0000FF"},{"position":0.5,"color":"#00FF0080","midpoint":0.3},{"position":1,"color":"#0000FFFF"}]});
        let effects = json!([
            {"type":"shadow","kind":"drop","enabled":true,"x":2,"y":4,"blur":8,"spread":1,"color":[0,0,0,0.25]},
            {"type":"shadow","kind":"inner","enabled":true,"x":1,"y":2,"blur":3,"spread":0,"color":[0,0,0,0.2]},
            {"type":"layer_blur","enabled":true,"radius":1},
            {"type":"background_blur","enabled":true,"radius":4},
            {"type":"glow","enabled":true,"radius":5,"edge_width":1,"intensity":1,"threshold":0.5,"color":[1,0,0,0.5]}
        ]);
        let setup = apply(w, json!([{"op":"create","type":"rectangle","ref":"$rect","properties":{
            "rect":{"x":20,"y":20,"width":100,"height":80},"fills":[solid("#FFFFFF"),{"paint":gradient,"opacity":0.7}],
            "strokes":[{"paint":{"type":"solid","color":"#112233FF"},"width":3,"align":"outside"},{"paint":gradient,"width":1}],
            "blend":"multiply","effects":effects}}]));
        let result = execute(w, &setup, window, cx).unwrap();
        let uid = result["created"]["$rect"].clone();
        assert_eq!(w.shapes[0].fill_layers.len(), 1);
        assert_eq!(w.shapes[0].stroke_layers.len(), 1);
        assert_eq!(w.hierarchy.effects[&w.shapes[0].id].len(), 5);
        // A previously removed stop must not reset the editor's allocation watermark.
        let removed = w.shapes[0].fill_layers[0].gradient.add_stop().unwrap();
        w.shapes[0].fill_layers[0].gradient.remove_stop(removed);
        let before = w.snapshot_page(cx).0;
        let read = request(w, "nodes.get", json!({"page_id":w.pages.active,"revision":w.document_revision().to_string(),"ids":[uid]}));
        let nodes = execute(w, &read, window, cx).unwrap();
        let read_properties = &nodes["nodes"][0]["properties"];
        let properties = json!({"fills":read_properties["fills"],"strokes":read_properties["strokes"],
            "effects":read_properties["effects"],"blend":read_properties["blend"]});
        let unchanged = apply(w, json!([{"op":"update","id":uid,"properties":properties}]));
        assert_eq!(execute(w, &unchanged, window, cx).unwrap()["changed"], false);
        assert!(w.snapshot_page(cx).0 == before);
        let mut fills = properties["fills"].clone();
        fills[0]["hidden"] = json!(true);
        fills[1]["opacity"] = json!(0.35);
        let edit = apply(w, json!([{"op":"update","id":uid,"properties":{"fills":fills}}]));
        execute(w, &edit, window, cx).unwrap();
        assert!(w.shapes[0].fill_hidden);
        assert_eq!(w.shapes[0].fill_layers[0].opacity, 0.35);
        assert!(w.shapes[0].stroke == before.shapes[0].stroke);
        assert!(w.hierarchy.effects == before.hierarchy.effects);
        let capture = request(w, "canvas.capture", json!({"page_id":w.pages.active,"revision":w.document_revision().to_string(),"node_id":uid}));
        assert!(execute(w, &capture, window, cx).unwrap()["data_base64"].is_string());
        w.undo_redo(false, window, cx);
        assert!(w.snapshot_page(cx).0 == before);
        w.undo_redo(true, window, cx);
        let clear = apply(w, json!([{"op":"update","id":uid,"properties":{"fills":[],"strokes":[],"effects":[]}}]));
        let cleared = execute(w, &clear, window, cx).unwrap();
        assert_eq!(cleared["nodes"][0]["properties"]["fills"], json!([]));
        assert_eq!(cleared["nodes"][0]["properties"]["strokes"], json!([]));
        assert!(w.hierarchy.effects.is_empty());
    }).unwrap();
}

#[gpui::test]
fn image_paint_resources_survive_source_deletion_storage_and_export(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fill.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]))
        .save(&path)
        .unwrap();
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |w, window, cx| {
        let import = apply(w, json!([{"op":"import","ref":"$image","rect":{"x":0,"y":0,"width":8,"height":8},
            "source":{"type":"image","source":{"type":"path","path":path}}}]));
        let imported = execute(w, &import, window, cx).unwrap();
        let hash = imported["nodes"][0]["asset"]["hash"].clone();
        let setup = apply(w, json!([
            {"op":"create","ref":"$primary","type":"frame","properties":{"rect":{"x":100,"y":0,"width":32,"height":32},"fills":[{"paint":{"type":"image","hash":hash}}]}},
            {"op":"create","ref":"$extra","type":"rectangle","properties":{"rect":{"x":0,"y":0,"width":32,"height":32},"fills":[solid("#0000FFFF"),{"paint":{"type":"image","hash":hash},"opacity":0.5}]}},
            {"op":"delete","id":imported["created"]["$image"]}
        ]));
        let result = execute(w, &setup, window, cx).unwrap();
        let uid = result["created"]["$extra"].clone();
        let primary = result["nodes"].as_array().unwrap().iter().find(|node| node["id"] == result["created"]["$primary"]).unwrap();
        let unchanged = apply(w, json!([{"op":"update","id":result["created"]["$primary"],"properties":{"fills":primary["properties"]["fills"]}}]));
        assert_eq!(execute(w, &unchanged, window, cx).unwrap()["changed"], false);
        let remove_primary = apply(w, json!([{"op":"delete","id":result["created"]["$primary"]}]));
        execute(w, &remove_primary, window, cx).unwrap();
        let (bytes, sources) = w.snapshot_document(setup.document_id.as_ref().unwrap(), cx).unwrap();
        assert_eq!(sources.len(), 1, "Only the additional fill now owns the image");
        let saved = directory.path().join("paint.rovar");
        crate::document::save_as(&saved, &bytes, &sources, cx.text_system()).unwrap();
        let loaded = crate::document::load(&saved).unwrap().into_document().unwrap();
        assert!(loaded.pages[0].shapes[0].fill_layers[0].image_fill.asset.is_some());
        let capture = request(w, "canvas.capture", json!({"page_id":w.pages.active,"revision":w.document_revision().to_string(),"node_id":uid}));
        let captured = execute(w, &capture, window, cx).unwrap();
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD.decode(captured["data_base64"].as_str().unwrap()).unwrap();
        let pixels = image::load_from_memory(&bytes).unwrap().to_rgba8();
        let center = pixels.get_pixel(16, 16).0;
        assert!(center[0] >= 125 && center[0] <= 130 && center[2] >= 125 && center[2] <= 130, "Fills must composite bottom-to-top: {center:?}");
    }).unwrap();
}

#[gpui::test]
fn invalid_appearance_and_locked_targets_roll_back_entire_batches(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |w, window, cx| {
        let setup = apply(w, json!([{"op":"create","ref":"$r","type":"rectangle","properties":{"rect":{"x":0,"y":0,"width":80,"height":80}}}]));
        let result = execute(w, &setup, window, cx).unwrap();
        let uid = result["created"]["$r"].clone();
        let before = w.snapshot_page(cx).0;
        let revision = w.document_revision();
        for invalid in [
            json!({"fills":[{"paint":{"type":"solid","color":"bad"}}]}),
            json!({"fill":"#FFFFFFFF","fills":[]}),
            json!({"fills":[{"paint":{"type":"image","hash":"missing"}}]}),
            json!({"fills":[{"paint":{"type":"gradient","stops":[{"position":1,"color":"#FFF000"},{"position":0,"color":"#000000"}]}}]}),
            json!({"strokes":[{"paint":{"type":"solid","color":"#000000"},"width":-1}]}),
            json!({"strokes":[{"paint":{"type":"points","gradient":crate::scene::point_gradient::PointGradient::default()}}]}),
            json!({"effects":[{"type":"layer_blur","enabled":true,"radius":999}]}),
        ] {
            let edit = apply(w, json!([{"op":"update","id":uid,"properties":{"opacity":0.2}}, {"op":"update","id":uid,"properties":invalid}]));
            assert!(execute(w, &edit, window, cx).is_err());
            assert!(w.snapshot_page(cx).0 == before);
            assert_eq!(w.document_revision(), revision);
        }
        w.shapes[0].layer.locked = true;
        let edit = apply(w, json!([{"op":"update","id":uid,"properties":{"fills":[solid("#FF0000")]}}]));
        assert_eq!(execute(w, &edit, window, cx).unwrap_err().code, "LOCKED");
    }).unwrap();
}

#[gpui::test]
fn closed_paths_accept_point_fills_and_reject_unsupported_effects_atomically(
    cx: &mut TestAppContext,
) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |w, window, cx| {
        let points = json!({"type":"points","gradient":crate::scene::point_gradient::PointGradient::default()});
        let path = json!({"closed":true,"nodes":[{"anchor":{"x":0,"y":0}},
            {"anchor":{"x":1,"y":0}}, {"anchor":{"x":0.5,"y":1}}]});
        let setup = apply(w, json!([{"op":"create","ref":"$path","type":"bezier","properties":{
            "rect":{"x":0,"y":0,"width":100,"height":100},"path":path,"fills":[{"paint":points}]}}]));
        let result = execute(w, &setup, window, cx).unwrap();
        assert_eq!(result["nodes"][0]["properties"]["fills"][0]["paint"], points);
        let uid = result["created"]["$path"].clone();
        let before = w.snapshot_page(cx).0;
        let invalid = apply(w, json!([{"op":"update","id":uid,"properties":{
            "fills":[],"effects":[{"type":"background_blur","enabled":true,"radius":4}]}}]));
        assert!(execute(w, &invalid, window, cx).is_err());
        assert!(w.snapshot_page(cx).0 == before);
        for kind in ["radial", "angular", "diamond"] {
            let edit = apply(w, json!([{"op":"update","id":uid,"properties":{"fills":[{"paint":{
                "type":"gradient","kind":kind,"stops":[{"position":0,"color":"#FF0000"},{"position":1,"color":"#0000FF"}]}}]}}]));
            let result = execute(w, &edit, window, cx).unwrap();
            assert_eq!(result["nodes"][0]["properties"]["fills"][0]["paint"]["kind"], kind);
            let capture = request(w, "canvas.capture", json!({"page_id":w.pages.active,"revision":w.document_revision().to_string(),"node_id":uid}));
            assert!(execute(w, &capture, window, cx).unwrap()["data_base64"].is_string());
        }
    }).unwrap();
}
