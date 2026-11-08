use super::*;
use crate::scene::presentation::{Action, ScrollAxis};

#[gpui::test]
fn prototype_scroll_settings_and_targets_roundtrip_duplicate_and_roll_back_atomically(
    cx: &mut TestAppContext,
) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx, |w, window, cx| {
        let setup = apply(w, json!([
            {"op":"create","ref":"$frame","type":"frame","properties":{"rect":{"x":0,"y":0,"width":200,"height":200}}},
            {"op":"create","ref":"$item","type":"rectangle","parent":"$frame","properties":{"rect":{"x":20,"y":400,"width":40,"height":40}}},
            {"op":"create","ref":"$button","type":"rectangle","parent":"$frame","properties":{"rect":{"x":120,"y":10,"width":40,"height":40}}},
            {"op":"set_prototype","id":"$frame","scroll":"both"},
            {"op":"set_prototype","id":"$button","fixed":true},
            {"op":"set_interaction","id":"$button","trigger":"click","action":{"type":"scroll_to","target":"$item"}},
            {"op":"set_interaction","id":"$button","trigger":"click","action":{"type":"none"}},
            {"op":"set_interaction","id":"$button","trigger":"click","action":{"type":"multiple","actions":[
                {"type":"scroll_to","target":"$item"},
                {"type":"set_property","target":"$item","property":{"property":"opacity","value":0.7}}
            ]}}
        ]));
        let result = execute(w, &setup, window, cx).unwrap();
        let frame = result["created"]["$frame"].as_str().unwrap();
        let button = result["created"]["$button"].as_str().unwrap();
        let original = w.snapshot_page(cx).0;
        let node = result["nodes"].as_array().unwrap().iter().find(|n| n["id"] == button).unwrap();
        assert_eq!(node["interactions"]["fixed"], true);
        assert_eq!(node["interactions"]["click"]["action"]["actions"][0]["target"], result["created"]["$item"]);
        assert_eq!(node["interactions"]["click"]["action"]["actions"][1]["target"], result["created"]["$item"]);
        let invalid = apply(w, json!([
            {"op":"set_prototype","id":frame,"scroll":"horizontal"},
            {"op":"set_prototype","id":button,"scroll":"vertical"}
        ]));
        assert_eq!(execute(w, &invalid, window, cx).unwrap_err().operation, Some(1));
        assert!(w.snapshot_page(cx).0 == original);
        let invalid_actions = apply(w, json!([{"op":"set_interaction","id":button,"trigger":"click","action":{"type":"multiple","actions":[
            {"type":"set_property","target":frame,"property":{"property":"opacity","value":0.4}},
            {"type":"set_property","target":frame,"property":{"property":"width","value":-10}}
        ]}}]));
        assert!(execute(w, &invalid_actions, window, cx).is_err());
        assert!(w.snapshot_page(cx).0 == original);
        let duplicate = apply(w, json!([{"op":"duplicate","id":frame,"ref":"$copy","offset":{"x":300,"y":0}}]));
        let copied = execute(w, &duplicate, window, cx).unwrap();
        let page = w.snapshot_page(cx).0;
        let root = model::lookup(&page, copied["created"]["$copy"].as_str().unwrap(), &BTreeMap::new()).unwrap();
        assert_eq!(page.hierarchy.interactions[&root].scroll, ScrollAxis::Both);
        let ids = crate::scene::components::subtree(&page, root);
        let action = ids.iter().filter_map(|id| page.hierarchy.interactions.get(id))
            .find(|i| i.fixed).unwrap();
        let steps = action.click.as_ref().unwrap().steps();
        assert_eq!(steps.len(), 2);
        let Action::ScrollTo { target } = steps[0] else { panic!("Missing scroll target"); };
        assert!(ids.contains(&target));
        assert!(matches!(steps[1], Action::SetProperty { target: property_target, .. } if property_target == target));
        let (json, sources) = w.snapshot_document(duplicate.document_id.as_deref().unwrap(), cx).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("prototype.rovar");
        crate::document::save_as(&path, &json, &sources, cx.text_system()).unwrap();
        let loaded = crate::document::load(&path).unwrap().into_document().unwrap();
        assert_eq!(loaded.pages[0].hierarchy.interactions, page.hierarchy.interactions);
        w.undo_redo(false, window, cx);
        assert!(w.snapshot_page(cx).0 == original);
        w.undo_redo(true, window, cx);
        assert!(w.snapshot_page(cx).0 == page);
    }).unwrap();
}
