use super::*;
use crate::scene::components;

fn setup(w: &Workspace) -> Request {
    apply(
        w,
        json!([
            {"op":"create","ref":"$master","type":"group","properties":{"name":"Button","rect":{"x":0,"y":0,"width":120,"height":60}}},
            {"op":"create","ref":"$background","type":"rectangle","parent":"$master","properties":{"name":"Background","rect":{"x":0,"y":0,"width":120,"height":60},"fill":"#CCCCCCFF"}},
            {"op":"create","ref":"$label","type":"text","parent":"$master","properties":{"name":"Label","rect":{"x":10,"y":10,"width":100,"height":40},"content":"Default"}},
            {"op":"create_component","id":"$master","ref":"$default","name":"Button"},
            {"op":"create_variant_set","ref":"$set","name":"Button","variants":[{"component":"$default","name":"Default"}]},
            {"op":"add_variant","source":"$default","ref":"$active_master","component_ref":"$active","name":"Active","position":{"x":180,"y":0}},
            {"op":"instantiate_component","component":"$default","ref":"$instance","position":{"x":360,"y":80}},
        {"op":"set_interaction","id":"$master","trigger":"hover","action":{"type":"change_variant","target":"$active"},"hover_exit":{"type":"change_variant","target":"$active"}}
        ]),
    )
}

fn id(page: &crate::document::Page, value: &Value) -> usize {
    model::lookup(page, value.as_str().unwrap(), &BTreeMap::new()).unwrap()
}

#[gpui::test]
fn switching_variants_preserves_instance_identity_overrides_interactions_and_undo(
    cx: &mut TestAppContext,
) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|w,window,cx|{
        let setup=setup(w);
        let result=execute(w,&setup,window,cx).unwrap();
        let nodes=&result["created"];
        let defs=&result["created_components"];
        let page=w.snapshot_page(cx).0;
        let source_label=id(&page,&nodes["$label"]);
        let source_background=id(&page,&nodes["$background"]);
        let active=id(&page,&nodes["$active_master"]);
        let instance=id(&page,&nodes["$instance"]);
        let alt_label=page.hierarchy.components[&active].nodes[&source_label];
        let alt_background=page.hierarchy.components[&active].nodes[&source_background];
        let label=page.hierarchy.components[&instance].nodes[&source_label];
        let background=page.hierarchy.components[&instance].nodes[&source_background];
        let uuids=page.node_ids();
        let edit=apply(w,json!([
            {"op":"update","id":uuids[&alt_background],"properties":{"fill":"#5566DDFF"}},
            {"op":"update","id":uuids[&alt_label],"properties":{"content":"Active"}},
            {"op":"update","id":uuids[&label],"properties":{"content":"Custom label"}},
            {"op":"set_interaction","id":nodes["$instance"],"trigger":"click","action":{"type":"back"}}
        ]));
        execute(w,&edit,window,cx).unwrap();
        let before=w.snapshot_page(cx).0;
        let switch=apply(w,json!([{"op":"switch_variant","id":nodes["$instance"],"component":defs["$active"]}]));
        execute(w,&switch,window,cx).unwrap();
        let after=w.snapshot_page(cx).0;
        assert_eq!(after.node_ids()[&instance],uuids[&instance]);
        assert_eq!(after.node_ids()[&label],uuids[&label]);
        assert_eq!(after.texts.iter().find(|t|t.id==label).unwrap().content,"Custom label");
        assert_eq!(after.shapes.iter().find(|s|s.id==background).unwrap().color,gpui::rgb(0x5566dd));
        assert!(components::bounds(&before,instance)==components::bounds(&after,instance));
        assert_eq!(after.hierarchy.interactions[&instance].click,Some(crate::scene::presentation::Action::Back));
        assert_eq!(after.hierarchy.components[&instance].component,defs["$active"].as_str().unwrap());
        let noop=apply(w,switch.params["operations"].clone());
        assert_eq!(execute(w,&noop,window,cx).unwrap()["changed"],false);
        w.undo_redo(false,window,cx);
        assert!(w.snapshot_page(cx).0==before);
        w.undo_redo(true,window,cx);
        assert!(w.snapshot_page(cx).0==after);
        let list=request(w,"components.list",json!({}));
        let listed=execute(w,&list,window,cx).unwrap();
        assert_eq!(listed["sets"][0]["variants"].as_array().unwrap().len(),2);
        assert!(listed["components"].as_array().unwrap().iter().any(|c|c["id"]==defs["$active"] && c["roots"].as_array().unwrap().len()==2));
        let (bytes,sources)=w.snapshot_document(setup.document_id.as_ref().unwrap(),cx).unwrap();
        let dir=tempfile::tempdir().unwrap(); let path=dir.path().join("variants.rovar");
        crate::document::save_as(&path,&bytes,&sources,cx.text_system()).unwrap();
        let loaded=crate::document::load(&path).unwrap().into_document().unwrap();
        assert_eq!(loaded.component_sets,w.components.sets);
        assert_eq!(loaded.pages[0].hierarchy.components[&instance].component,defs["$active"].as_str().unwrap());
        w.shapes.iter_mut().find(|s|s.id==background).unwrap().layer.locked=true;
        let protected=w.snapshot_page(cx).0;
        let blocked=apply(w,json!([{"op":"switch_variant","id":nodes["$instance"],"component":defs["$default"]}]));
        assert_eq!(execute(w,&blocked,window,cx).unwrap_err().code,"LOCKED");
        assert!(w.snapshot_page(cx).0==protected);
    }).unwrap();
}

#[gpui::test]
fn variant_metadata_is_undoable_and_invalid_batches_roll_back(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|w,window,cx|{
        let setup=setup(w);
        let result=execute(w,&setup,window,cx).unwrap();
        let set=result["created_variant_sets"]["$set"].clone();
        let defs=&result["created_components"];
        let before=w.snapshot_page(cx).0;
        let original_sets=w.components.sets.clone();
        let rename=apply(w,json!([
            {"op":"rename_variant_set","id":set,"name":"Controls"},
            {"op":"rename_variant","component":defs["$active"],"name":"Selected"}
        ]));
        let renamed=execute(w,&rename,window,cx).unwrap();
        assert_eq!(renamed["changed"],true);
        assert_eq!(renamed["changed_pages"],json!([]));
        assert!(w.snapshot_page(cx).0==before);
        assert_eq!(execute(w,&rename,window,cx).unwrap(),renamed);
        w.undo_redo(false,window,cx);
        assert_eq!(w.components.sets,original_sets);
        w.undo_redo(true,window,cx);
        let sets=w.components.sets.clone();
        let definitions=w.components.definitions.clone();
        let revision=w.document_revision();
        for invalid in [
            json!({"op":"rename_variant","component":defs["$active"],"name":"Default"}),
            json!({"op":"create_variant_set","ref":"$other","name":"Other","variants":[{"component":defs["$default"],"name":"Default"}]}),
            json!({"op":"add_variant","source":defs["$default"],"ref":"$duplicate","component_ref":"$duplicate","name":"Another","position":{"x":0,"y":200}}),
            json!({"op":"switch_variant","id":result["created"]["$master"],"component":defs["$active"]})
        ] {
            let edit=apply(w,json!([{"op":"rename_variant_set","id":set,"name":"Must roll back"},invalid]));
            assert_eq!(execute(w,&edit,window,cx).unwrap_err().operation,Some(1));
            assert_eq!(w.document_revision(),revision);
            assert!(w.snapshot_page(cx).0==before);
            assert_eq!(w.components.sets,sets);
            assert!(w.components.definitions==definitions);
        }
    }).unwrap();
}

#[gpui::test]
fn background_variant_switches_preserve_view_and_locked_masters_block_metadata_edits(
    cx: &mut TestAppContext,
) {
    let handle = crate::editor::tests::open(cx);
    handle
        .update(cx, |w, window, cx| {
            let setup = setup(w);
            let result = execute(w, &setup, window, cx).unwrap();
            let defs = &result["created_components"];
            let mut other = crate::document::Page::empty("Other".into());
            let base = defs["$default"].as_str().unwrap().to_owned();
            let root = super::super::structure::instantiate(
                &mut other,
                &w.components.definitions[&base],
                base,
                None,
                p::Point { x: 0., y: 0. },
            )
            .unwrap();
            let uid = other.node_ids()[&root];
            let other_id = other.id.clone();
            w.pages
                .entries
                .push(crate::editor::pages::PageState::new(other));
            let active = w.pages.active.clone();
            let view = w.agent_view();
            let selection = w.selection_ids();
            let mut switch = apply(
                w,
                json!([{"op":"switch_variant","id":uid,"component":defs["$active"]}]),
            );
            switch.params["page_id"] = json!(other_id);
            execute(w, &switch, window, cx).unwrap();
            assert_eq!(w.pages.active, active);
            assert_eq!(w.agent_view(), view);
            assert_eq!(w.selection_ids(), selection);
            assert_eq!(
                w.agent_page(&other_id, cx).hierarchy.components[&root].component,
                defs["$active"].as_str().unwrap()
            );
            let original = w.snapshot_page(cx).0;
            let master = id(&original, &result["created"]["$master"]);
            w.hierarchy.groups.get_mut(&master).unwrap().layer.locked = true;
            let sets = w.components.sets.clone();
            let mut rename = apply(
                w,
                json!([{"op":"rename_variant","component":defs["$default"],"name":"Locked edit"}]),
            );
            rename.params["page_id"] = json!(other_id);
            assert_eq!(execute(w, &rename, window, cx).unwrap_err().code, "LOCKED");
            assert_eq!(w.components.sets, sets);
        })
        .unwrap();
}
