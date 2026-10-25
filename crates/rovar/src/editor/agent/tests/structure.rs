use super::*;
use crate::scene::components;

fn create(reference: &str, kind: &str, parent: Option<&str>, x: f32, y: f32) -> Value {
    json!({"op":"create","ref":reference,"type":kind,"parent":parent,
        "properties":{"rect":{"x":x,"y":y,"width":100,"height":100}}})
}

#[gpui::test]
fn duplicate_and_reparent_preserve_geometry_links_identity_and_atomic_undo(
    cx: &mut TestAppContext,
) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|w,window,cx|{
        let request=apply(w,json!([
            create("$left","frame",None,100.,200.),create("$right","frame",None,600.,400.),
            create("$group","group",Some("$left"),120.,220.),
            create("$a","rectangle",Some("$group"),120.,220.),
            create("$b","ellipse",Some("$group"),150.,240.),
            {"op":"set_interaction","id":"$a","trigger":"click","action":{"type":"navigate","target":"$right"}},
            {"op":"set_interaction","id":"$b","trigger":"click","action":{"type":"set_property","target":"$a","property":{"property":"opacity","value":0.5}}}
        ]));
        let result=execute(w,&request,window,cx).unwrap();
        // Existing vector kinds can be copied even when creation uses Bezier paths.
        w.shapes.iter_mut().find(|s|s.kind==ShapeKind::Ellipse).unwrap().kind=ShapeKind::Star;
        let before=w.snapshot_page(cx).0;
        let ids=&result["created"];
        let copy=apply(w,json!([
            {"op":"duplicate","id":ids["$group"],"ref":"$copy","offset":{"x":25,"y":15}},
            {"op":"reparent","id":"$copy","parent":ids["$right"]}
        ]));
        let result=execute(w,&copy,window,cx).unwrap();
        assert_eq!(execute(w,&copy,window,cx).unwrap(),result);
        let page=w.snapshot_page(cx).0;
        let root=model::lookup(&page,result["created"]["$copy"].as_str().unwrap(),&BTreeMap::new()).unwrap();
        let copied=components::subtree(&page,root);
        assert_eq!(copied.len(),3);
        let original=model::lookup(&before,ids["$group"].as_str().unwrap(),&BTreeMap::new()).unwrap();
        let a=components::bounds(&before,original).unwrap();
        let b=components::bounds(&page,root).unwrap();
        assert_eq!((b.x,b.y),(a.x+25.,a.y+15.));
        let right=model::lookup(&page,ids["$right"].as_str().unwrap(),&BTreeMap::new()).unwrap();
        assert_eq!(components::parent(&page,root),Some(right));
        for shape in page.shapes.iter().filter(|s|copied.contains(&s.id)) {
            assert!(!before.node_ids().values().any(|uid|*uid==shape.uid));
            match page.hierarchy.interactions[&shape.id].click.as_ref().unwrap() {
                crate::scene::presentation::Action::Navigate { target } => assert_eq!(*target,right),
                crate::scene::presentation::Action::SetProperty { target,.. } => assert!(copied.contains(target)),
                _=>panic!("unexpected action"),
            }
        }
        let invalid=apply(w,json!([
            {"op":"update","id":ids["$a"],"properties":{"opacity":0.2}},
            {"op":"reparent","id":ids["$group"],"parent":ids["$b"]}
        ]));
        assert_eq!(execute(w,&invalid,window,cx).unwrap_err().operation,Some(1));
        assert!(w.snapshot_page(cx).0==page);
        w.undo_redo(false,window,cx);
        assert!(w.snapshot_page(cx).0==before);
        w.undo_redo(true,window,cx);
        assert!(w.snapshot_page(cx).0==page);
    }).unwrap();
}

#[gpui::test]
fn paths_masks_and_booleans_are_editable_but_invalid_structures_roll_back(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|w,window,cx|{
        let path=json!({"closed":true,"nodes":[
            {"anchor":{"x":0,"y":0},"outgoing":{"x":0.4,"y":-0.2}},
            {"anchor":{"x":1,"y":0}}, {"anchor":{"x":1,"y":1}}, {"anchor":{"x":0,"y":1}}
        ]});
        let setup=apply(w,json!([
            create("$group","group",None,0.,0.),
            {"op":"create","ref":"$path","type":"bezier","parent":"$group","properties":{"rect":{"x":10,"y":10,"width":100,"height":100},"path":path,"fill":"#AABBCCFF"}},
            create("$rect","rectangle",Some("$group"),50.,50.),
            {"op":"set_mask","id":"$group","source":"$path"}
        ]));
        let result=execute(w,&setup,window,cx).unwrap();
        let ids=&result["created"];
        let before=w.snapshot_page(cx).0;
        assert!(result["nodes"].as_array().unwrap().iter().any(|n|n["mask_source_id"]==ids["$path"]));
        let mut changed=path.clone();
        changed["nodes"][0]["outgoing"]=json!({"x":0.7,"y":-0.3});
        let edit=apply(w,json!([
            {"op":"update","id":ids["$path"],"properties":{"path":changed}},
            {"op":"set_mask","id":ids["$group"],"source":null},
            {"op":"set_boolean","id":ids["$group"],"operation":"subtract"}
        ]));
        execute(w,&edit,window,cx).unwrap();
        let after=w.snapshot_page(cx).0;
        let group=model::lookup(&after,ids["$group"].as_str().unwrap(),&BTreeMap::new()).unwrap();
        assert!(crate::scene::boolean::Cache::default().get(&after.hierarchy,&after.shapes,group).is_some());
        let mut open=changed;
        open["closed"]=json!(false);
        let invalid=apply(w,json!([{"op":"update","id":ids["$path"],"properties":{"path":open}}]));
        assert!(execute(w,&invalid,window,cx).is_err());
        assert!(w.snapshot_page(cx).0==after);
        w.undo_redo(false,window,cx);
        assert!(w.snapshot_page(cx).0==before);
        w.undo_redo(true,window,cx);
        let locked=w.shapes[0].id;
        w.shapes[0].layer.locked=true;
        let invalid=apply(w,json!([{"op":"duplicate","id":ids["$group"],"ref":"$copy"}]));
        assert_eq!(execute(w,&invalid,window,cx).unwrap_err().code,"LOCKED");
        assert_eq!(w.shapes[0].id,locked);
    }).unwrap();
}

#[gpui::test]
fn component_edits_sync_other_pages_and_undo_with_overrides_and_bindings(cx: &mut TestAppContext) {
    let handle = crate::editor::tests::open(cx);
    handle.update(cx,|w,window,cx|{
        let setup=apply(w,json!([
            create("$master","rectangle",None,10.,20.),
            {"op":"create_component","id":"$master","ref":"$component","name":"Card"},
            {"op":"instantiate_component","component":"$component","ref":"$instance","position":{"x":300,"y":200}},
            {"op":"duplicate","id":"$master","ref":"$copy","offset":{"x":200,"y":0}}
        ]));
        let result=execute(w,&setup,window,cx).unwrap();
        let ids=&result["created"];
        assert_eq!(w.hierarchy.components.values().filter(|b|b.master).count(),1);
        let page=w.snapshot_page(cx).0;
        let instance=model::lookup(&page,ids["$instance"].as_str().unwrap(),&BTreeMap::new()).unwrap();
        let component=result["created_components"]["$component"].as_str().unwrap().to_string();
        let mut second=crate::document::Page::empty("Instances".into());
        let other=super::super::structure::instantiate(&mut second,&w.components.definitions[&component],component.clone(),None,p::Point { x:50.,y:50. }).unwrap();
        let second_id=second.id.clone();
        w.pages.entries.push(crate::editor::pages::PageState::new(second.clone()));
        let override_request=apply(w,json!([{"op":"update","id":ids["$instance"],"properties":{"fill":"#FF0000FF"}}]));
        execute(w,&override_request,window,cx).unwrap();
        let before=w.snapshot_page(cx).0;
        let request=apply(w,json!([{"op":"update","id":ids["$master"],"properties":{"fill":"#00FF00FF","radius":12}}]));
        let result=execute(w,&request,window,cx).unwrap();
        assert!(result["changed_pages"].as_array().unwrap().contains(&json!(second_id)));
        let page=w.snapshot_page(cx).0;
        let local=page.shapes.iter().find(|s|s.id==instance).unwrap();
        assert_eq!(local.color,gpui::rgb(0xff0000));
        assert_eq!(local.radius,12.);
        let remote=&w.pages.entries.iter().find(|s|s.page.id==second_id).unwrap().page;
        let remote_shape=remote.shapes.iter().find(|s|s.id==other).unwrap();
        assert_eq!(remote_shape.color,gpui::rgb(0x00ff00));
        assert_eq!(remote_shape.radius,12.);
        w.undo_redo(false,window,cx);
        assert!(w.snapshot_page(cx).0==before);
        assert!(w.pages.entries.iter().find(|s|s.page.id==second_id).unwrap().page==second);
        w.undo_redo(true,window,cx);
        let second_state=w.pages.entries.iter_mut().find(|s|s.page.id==second_id).unwrap();
        second_state.page.shapes.iter_mut().find(|s|s.id==other).unwrap().layer.locked=true;
        let protected=w.snapshot_page(cx).0;
        let locked=apply(w,json!([{"op":"update","id":ids["$master"],"properties":{"fill":"#0000FFFF"}}]));
        assert_eq!(execute(w,&locked,window,cx).unwrap_err().code,"LOCKED");
        assert!(w.snapshot_page(cx).0==protected);
        w.pages.entries.iter_mut().find(|s|s.page.id==second_id).unwrap().page.shapes.iter_mut().find(|s|s.id==other).unwrap().layer.locked=false;
        let detach=apply(w,json!([{"op":"detach_component","id":ids["$instance"]}]));
        execute(w,&detach,window,cx).unwrap();
        assert!(!w.hierarchy.components.contains_key(&instance));
        assert_eq!(w.shapes.iter().find(|s|s.id==instance).unwrap().color,gpui::rgb(0xff0000));
        let revision=w.document_revision();
        let before=w.snapshot_page(cx).0;
        let invalid=apply(w,json!([
            {"op":"create_component","id":ids["$instance"],"ref":"$bad","name":"New"},
            {"op":"reparent","id":ids["$master"],"parent":ids["$instance"]}
        ]));
        assert!(execute(w,&invalid,window,cx).is_err());
        assert_eq!(w.document_revision(),revision);
        assert!(w.snapshot_page(cx).0==before);
        assert_eq!(w.components.definitions.len(),1);
        // A reusable definition is still a document change if its main binding
        // is detached in the same atomic batch, leaving the page unchanged.
        let definition_only=apply(w,json!([
            {"op":"create_component","id":ids["$instance"],"ref":"$saved","name":"Independent card"},
            {"op":"detach_component","id":ids["$instance"]}
        ]));
        let result=execute(w,&definition_only,window,cx).unwrap();
        assert_eq!(result["changed"],true);
        assert_eq!(result["changed_pages"],json!([]));
        assert!(w.snapshot_page(cx).0==before);
        assert_eq!(w.components.definitions.len(),2);
        w.undo_redo(false,window,cx);
        assert_eq!(w.components.definitions.len(),1);
    }).unwrap();
}
