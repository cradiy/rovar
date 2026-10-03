use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{EntityInputHandler, TestAppContext, VisualTestContext};

#[gpui::test]
fn component_generated_nodes_have_stable_identities_across_independent_refreshes(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            let component = main_component(this, window, cx);
            this.insert_document_component(&component, false, Some(point(400., 100.)), window, cx);
            let instance = *this.selection_ids().first().unwrap();
            let original = this.snapshot_page(cx).0.node_ids();
            let id = this.next_id;
            this.next_id += 1;
            let mut node = Shape::new(id, Some(1), ShapeKind::Ellipse, rect(20., 20., 20., 20.));
            node.name = "New child".into();
            this.shapes.push(node);
            let mut a = vec![this.snapshot_page(cx).0];
            let mut b = a.clone();
            b[0].next_id += 20;
            let mut definitions_a = this.components.definitions.clone();
            let mut definitions_b = definitions_a.clone();
            model::synchronize(&mut a, &mut definitions_a).unwrap();
            model::synchronize(&mut b, &mut definitions_b).unwrap();
            let added = |page: &crate::document::Page| {
                page.shapes
                    .iter()
                    .find(|n| n.board == Some(instance) && n.name == "New child")
                    .unwrap()
                    .clone()
            };
            assert_ne!(added(&a[0]).id, added(&b[0]).id);
            assert_eq!(
                added(&a[0]).uid,
                added(&b[0]).uid,
                "Component propagation cannot invent different identities on each device"
            );
            for page in [&a[0], &b[0]] {
                page.validate().unwrap();
                for (id, uid) in &original {
                    assert_eq!(page.node_ids()[id], *uid);
                }
            }
            let before = a[0].node_ids();
            model::synchronize(&mut a, &mut definitions_a).unwrap();
            assert_eq!(a[0].node_ids(), before);
        })
        .unwrap();
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn main_component(
    this: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> String {
    this.add_artboard(rect(100., 100., 200., 150.), cx);
    this.shapes.push(Shape::new(
        2,
        Some(1),
        ShapeKind::Rectangle,
        rect(10., 10., 50., 30.),
    ));
    let text = this.make_text(3, Some(1), rect(10., 60., 120., 30.), window, cx);
    text.editor.update(cx, |editor, cx| {
        editor.replace_text_in_range(None, "Button", window, cx)
    });
    this.texts.push(text);
    this.next_id = 4;
    this.set_selection(BTreeSet::from([1]), cx);
    this.create_component(window, cx);
    this.sync_components(window, cx);
    this.components.definitions.keys().next().unwrap().clone()
}

#[gpui::test]
fn main_changes_propagate_across_pages_preserving_text_overrides_and_undo(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            let id = main_component(this, window, cx);
            let first = this.pages.active.clone();
            this.add_page(None, window, cx);
            let second = this.pages.active.clone();
            this.insert_document_component(&id, false, Some(point(400., 200.)), window, cx);
            let root = *this.selection_ids().first().unwrap();
            let shape = this.hierarchy.components[&root].nodes[&2];
            let text = this.hierarchy.components[&root].nodes[&3];
            this.insert_document_component(&id, false, Some(point(700., 200.)), window, cx);
            let other = *this.selection_ids().first().unwrap();
            let other_text = this.hierarchy.components[&other].nodes[&3];
            this.texts
                .iter()
                .find(|t| t.id == text)
                .unwrap()
                .editor
                .update(cx, |editor, cx| {
                    editor.replace_text_in_range(Some(0..6), "Local label", window, cx);
                });
            this.switch_page(&first, window, cx);
            let before = Change::Shape {
                id: 2,
                index: 0,
                value: Some(this.shapes[0].clone()),
            };
            this.shapes.iter_mut().find(|s| s.id == 2).unwrap().color = rgb(0x8866ff);
            this.history.borrow_mut().record(vec![before], None);
            this.texts[0].editor.update(cx, |editor, cx| {
                editor.replace_text_in_range(Some(0..6), "Primary action", window, cx);
            });
            this.sync_components(window, cx);
            this.switch_page(&second, window, cx);
            assert_eq!(
                this.shapes.iter().find(|s| s.id == shape).unwrap().color,
                rgb(0x8866ff)
            );
            assert_eq!(
                this.texts
                    .iter()
                    .find(|t| t.id == text)
                    .unwrap()
                    .editor
                    .read(cx)
                    .document_text(
                        text,
                        uuid::Uuid::new_v4(),
                        Some(root),
                        rect(0., 0., 1., 1.),
                        Default::default()
                    )
                    .content,
                "Local label"
            );
            assert_eq!(this.boards[0].rect.x, 400.);
            assert_eq!(
                this.snapshot_page(cx)
                    .0
                    .texts
                    .iter()
                    .find(|t| t.id == other_text)
                    .unwrap()
                    .content,
                "Primary action"
            );
            this.reset_component(root, window, cx);
            let content = |this: &Workspace, cx: &gpui::App| {
                this.snapshot_page(cx).0.texts[0].content.clone()
            };
            assert_eq!(content(this, cx), "Primary action");
            this.undo_redo(false, window, cx);
            this.sync_components(window, cx);
            assert_eq!(content(this, cx), "Local label");
            this.detach_component(root, cx);
            assert!(!this.hierarchy.components.contains_key(&root));
            this.undo_redo(false, window, cx);
            this.sync_components(window, cx);
            assert!(this.hierarchy.components.contains_key(&root));
            let (json, _) = this
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            Document::decode(&json).unwrap();
        })
        .unwrap();
}

#[gpui::test]
fn structural_changes_reset_copy_and_main_undo_keep_valid_bindings(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            let id = main_component(this, window, cx);
            this.insert_document_component(&id, false, Some(point(500., 100.)), window, cx);
            let root = *this.selection_ids().first().unwrap();
            let child = this.hierarchy.components[&root].nodes[&2];
            this.set_selection(BTreeSet::from([child]), cx);
            this.delete_selected(cx);
            this.set_selection(BTreeSet::from([root]), cx);
            this.duplicate_selection(window, cx);
            this.sync_components(window, cx);
            let copy = *this.selection_ids().first().unwrap();
            assert_ne!(copy, root);
            let added = this.next_id;
            this.next_id += 1;
            this.shapes.push(Shape::new(
                added,
                Some(root),
                ShapeKind::Ellipse,
                rect(40., 40., 20., 20.),
            ));
            this.reset_component(root, window, cx);
            assert!(this.shapes.iter().all(|s| s.id != added));
            assert!(this.shapes.iter().any(|s| s.id == child));
            let source_before = this.page_edit(std::slice::from_ref(&this.pages.active), cx);
            let new_id = this.next_id;
            this.next_id += 1;
            this.shapes.push(Shape::new(
                new_id,
                Some(1),
                ShapeKind::Ellipse,
                rect(70., 10., 30., 30.),
            ));
            this.hierarchy.order = vec![1, new_id, 2, 3, root, child, copy];
            this.record_page_edit(source_before);
            this.sync_components(window, cx);
            assert_eq!(
                this.shapes.iter().filter(|s| s.board == Some(root)).count(),
                2
            );
            assert_eq!(
                this.shapes.iter().filter(|s| s.board == Some(copy)).count(),
                1,
                "deleted child stays overridden on copy"
            );
            this.undo_redo(false, window, cx);
            this.sync_components(window, cx);
            assert_eq!(
                this.shapes.iter().filter(|s| s.board == Some(root)).count(),
                1
            );
            let (json, _) = this
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            Document::decode(&json).unwrap();
        })
        .unwrap();
}

#[gpui::test]
fn document_assets_insert_on_drop_or_double_click_but_not_single_click(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, _, cx| {
            this.add_artboard(rect(0., 0., 100., 100.), cx);
            this.sidebar.resources = true;
            this.sidebar.collapsed = false;
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    click(&mut visual, "create-document-component");
    draw(&mut visual);
    let id = window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.components.definitions.len(), 1);
            this.components.definitions.keys().next().unwrap().clone()
        })
        .unwrap();
    let selector: &'static str = Box::leak(format!("document-component-{id}").into_boxed_str());
    click(&mut visual, selector);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards.len(), 1);
            assert_eq!(
                this.hierarchy
                    .components
                    .values()
                    .filter(|b| b.master)
                    .count(),
                1
            );
        })
        .unwrap();
    let card = visual.debug_bounds(selector).unwrap();
    visual.simulate_mouse_down(card.center(), MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        card.center() + point(px(12.), px(0.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_move(
        point(px(600.), px(340.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_up(
        point(px(600.), px(340.)),
        MouseButton::Left,
        Default::default(),
    );
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards.len(), 2);
            let root = *this.selection_ids().first().unwrap();
            let rect = this.world_rect(root).unwrap();
            assert_eq!(
                (rect.x + rect.width / 2., rect.y + rect.height / 2.),
                (600., 340.)
            );
        })
        .unwrap();
    visual.simulate_event(gpui::MouseDownEvent {
        position: card.center(),
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    visual.simulate_event(gpui::MouseUpEvent {
        position: card.center(),
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards.len(), 3)
        })
        .unwrap();
    click(&mut visual, "assets-local");
    draw(&mut visual);
    assert!(visual.debug_bounds("create-document-component").is_none());
}

#[gpui::test]
fn create_from_selection_accepts_single_layers_and_hides_for_existing_components(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.sidebar.resources = true;
            this.sidebar.collapsed = false;
            this.shapes.push(Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                rect(400., 100., 100., 100.),
            ));
            let text = this.make_text(2, None, rect(500., 300., 100., 30.), window, cx);
            text.editor.update(cx, |editor, cx| {
                editor.replace_text_in_range(None, "Label", window, cx)
            });
            this.texts.push(text);
            this.next_id = 3;
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    assert!(visual.debug_bounds("create-document-component").is_none());
    for id in [1, 2] {
        window
            .update(&mut visual.cx, |this, _, cx| {
                this.set_selection(BTreeSet::from([id]), cx)
            })
            .unwrap();
        draw(&mut visual);
        click(&mut visual, "create-document-component");
        draw(&mut visual);
        assert!(visual.debug_bounds("create-document-component").is_none());
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert!(this.hierarchy.components[&id].master);
                assert!(this.hierarchy.groups.is_empty());
                let (json, _) = this
                    .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                    .unwrap();
                Document::decode(&json).unwrap();
            })
            .unwrap();
    }
}

#[gpui::test]
fn image_overrides_reset_and_unused_definitions_survive_save_and_window_transfer(
    cx: &mut TestAppContext,
) {
    let folder = tempfile::tempdir().unwrap();
    let assets: Vec<_> = [40, 80, 120]
        .into_iter()
        .map(|value| {
            let path = folder.path().join(format!("{value}.png"));
            image::RgbaImage::from_pixel(2, 2, image::Rgba([value, 30, 90, 255]))
                .save(&path)
                .unwrap();
            crate::media::MediaAsset::load(path).unwrap()
        })
        .collect();
    let window = open(cx);
    let path = folder.path().join("components.rovar");
    window
        .update(cx, |this, window, cx| {
            let id = main_component(this, window, cx);
            this.shapes[0].image_fill.asset = Some(assets[0].clone());
            this.shapes[0].fill_mode = crate::scene::artboard::FillMode::Image;
            this.components.revision = None;
            this.sync_components(window, cx);
            this.insert_document_component(&id, false, Some(point(500., 100.)), window, cx);
            let root = *this.selection_ids().first().unwrap();
            let child = this.hierarchy.components[&root].nodes[&2];
            this.shapes
                .iter_mut()
                .find(|s| s.id == child)
                .unwrap()
                .image_fill
                .asset = Some(assets[1].clone());
            this.shapes[0].image_fill.asset = Some(assets[2].clone());
            this.components.revision = None;
            this.sync_components(window, cx);
            assert_eq!(
                this.shapes
                    .iter()
                    .find(|s| s.id == child)
                    .unwrap()
                    .image_fill
                    .asset
                    .as_ref()
                    .unwrap()
                    .hash,
                assets[1].hash
            );
            this.reset_component(root, window, cx);
            assert_eq!(
                this.shapes
                    .iter()
                    .find(|s| s.id == child)
                    .unwrap()
                    .image_fill
                    .asset
                    .as_ref()
                    .unwrap()
                    .hash,
                assets[2].hash
            );
            this.shapes[0].image_fill.asset = None;
            this.shapes[0].fill_mode = crate::scene::artboard::FillMode::Solid;
            this.components.revision = None;
            this.sync_components(window, cx);
            assert!(
                this.shapes
                    .iter()
                    .find(|s| s.id == child)
                    .unwrap()
                    .image_fill
                    .asset
                    .is_none()
            );
            this.shapes[0].image_fill.asset = Some(assets[0].clone());
            this.shapes[0].fill_mode = crate::scene::artboard::FillMode::Image;
            this.components.revision = None;
            this.sync_components(window, cx);
            this.set_selection(BTreeSet::from([1, root]), cx);
            this.delete_selected(cx);
            this.sync_components(window, cx);
            let (json, sources) = this
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            assert!(this.shapes.is_empty());
            assert_eq!(sources.len(), 1);
            crate::document::save_as(&path, &json, &sources, cx.text_system()).unwrap();
        })
        .unwrap();
    for value in [40, 80, 120] {
        std::fs::remove_file(folder.path().join(format!("{value}.png"))).unwrap();
    }
    let loaded = crate::document::load(&path).unwrap();
    window
        .update(cx, |this, window, cx| {
            this.load_document(loaded, window, cx).unwrap()
        })
        .unwrap();
    let transfer = window
        .update(cx, |this, _, cx| {
            this.transfer(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap()
        })
        .unwrap();
    let destination = open(cx);
    destination
        .update(cx, |this, window, cx| {
            this.receive_transfer(transfer, window, cx).unwrap();
            let id = this.components.definitions.keys().next().unwrap().clone();
            this.insert_document_component(&id, false, Some(point(300., 100.)), window, cx);
            assert_eq!(
                this.shapes[0].image_fill.asset.as_ref().unwrap().hash,
                assets[0].hash
            );
            assert!(
                this.shapes[0]
                    .image_fill
                    .asset
                    .as_ref()
                    .unwrap()
                    .source
                    .open()
                    .is_ok()
            );
        })
        .unwrap();
}
