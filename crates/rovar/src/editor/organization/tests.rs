use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{EntityInputHandler, TestAppContext, VisualTestContext, WindowHandle};

fn fixture(cx: &mut TestAppContext) -> WindowHandle<Workspace> {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 100.);
            this.view.zoom = 1.;
            for id in 1..=3 {
                this.shapes.push(Shape::new(
                    id,
                    None,
                    ShapeKind::Rectangle,
                    Rect {
                        x: 30. * id as f32,
                        y: 30. * id as f32,
                        width: 150.,
                        height: 100.,
                    },
                ));
            }
            let text = this.make_text(
                4,
                None,
                Rect {
                    x: 50.,
                    y: 250.,
                    width: 240.,
                    height: 70.,
                },
                window,
                cx,
            );
            text.editor.update(cx, |e, cx| {
                e.replace_text_in_range(None, "保留文字内容", window, cx)
            });
            this.texts.push(text);
            this.next_id = 5;
            this.select(None, cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    window
}
fn pick(visual: &mut VisualTestContext, x: f32, y: f32) {
    let p = visual.update(|window, cx| {
        let root = window.root::<Workspace>().unwrap().unwrap();
        let this = root.read(cx);
        let p = this.view.screen(point(x, y));
        this.bounds.get().origin + point(px(p.x), px(p.y))
    });
    visual.simulate_click(p, Default::default());
    draw(visual);
}

#[gpui::test]
fn group_gaps_select_drag_and_respect_layer_order_and_flags(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, cx| {
            this.view.zoom = 0.8;
            this.set_selection(BTreeSet::from([1, 4]), cx);
            this.group_selection(cx);
            this.select(None, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    pick(&mut visual, 80., 220.);
    let (start, before) = window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([5]));
            let p = this.view.screen(point(80., 220.));
            (
                this.bounds.get().origin + point(px(p.x), px(p.y)),
                this.history.borrow().undo_len(),
            )
        })
        .unwrap();
    let end = start + point(px(32.), px(16.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 70.);
            assert_eq!(this.texts[0].rect.x, 90.);
            assert_eq!(this.shapes[0].rect.y, 50.);
            assert_eq!(this.texts[0].rect.y, 270.);
            assert_eq!(this.shapes[1].rect.x, 60.);
            assert_eq!(this.history.borrow().undo_len(), before + 1);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[0].rect.x, 30.);
            assert_eq!(this.texts[0].rect.x, 50.);
            // Put an unrelated object beneath the group's empty area.
            this.shapes[2].rect.x = 70.;
            this.shapes[2].rect.y = 200.;
            this.select(None, cx);
        })
        .unwrap();
    draw(&mut visual);
    pick(&mut visual, 80., 220.);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.selection_ids(), BTreeSet::from([5]));
            this.hierarchy.order.retain(|id| *id != 3);
            this.hierarchy.order.push(3);
            this.select(None, cx);
        })
        .unwrap();
    draw(&mut visual);
    pick(&mut visual, 80., 220.);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.selected_shape, Some(3));
            this.hierarchy.order.retain(|id| *id != 3);
            this.hierarchy.order.insert(0, 3);
            this.hierarchy.groups.get_mut(&5).unwrap().layer.locked = true;
            this.select(None, cx);
        })
        .unwrap();
    draw(&mut visual);
    pick(&mut visual, 80., 220.);
    assert!(visual.debug_bounds("canvas-group-5").is_none());
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.selected_shape, Some(3));
            let group = this.hierarchy.groups.get_mut(&5).unwrap();
            group.layer.locked = false;
            group.layer.hidden = true;
            this.select(None, cx);
        })
        .unwrap();
    draw(&mut visual);
    pick(&mut visual, 80., 220.);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(3));
        })
        .unwrap();
    assert!(visual.debug_bounds("canvas-group-5").is_none());
}

#[gpui::test]
fn grouped_members_require_separate_double_clicks_to_select_then_edit(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, cx| {
            this.set_selection(BTreeSet::from([1, 4]), cx);
            this.group_selection(cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    for (id, x, y) in [(1, 40., 40.), (4, 80., 280.)] {
        window
            .update(&mut visual.cx, |this, _, cx| {
                this.set_selection(BTreeSet::from([5]), cx);
            })
            .unwrap();
        draw(&mut visual);
        let position = window
            .update(&mut visual.cx, |this, _, _| {
                let p = this.view.screen(point(x, y));
                this.bounds.get().origin + point(px(p.x), px(p.y))
            })
            .unwrap();
        for editing in [false, true] {
            visual.simulate_click(position, Default::default());
            draw(&mut visual);
            visual.simulate_event(gpui::MouseDownEvent {
                position,
                button: MouseButton::Left,
                click_count: 2,
                modifiers: Default::default(),
                first_mouse: false,
            });
            visual.simulate_mouse_up(position, MouseButton::Left, Default::default());
            draw(&mut visual);
            window
                .update(&mut visual.cx, |this, _, cx| {
                    assert_eq!(this.selection_ids(), BTreeSet::from([id]));
                    if id == 1 {
                        assert_eq!(this.vector_edit, editing.then_some(id));
                    } else {
                        assert_eq!(this.texts[0].editor.read(cx).editing, editing);
                    }
                    assert_eq!(this.layer_parent(id), Some(5));
                })
                .unwrap();
        }
        visual.simulate_keystrokes("escape");
        draw(&mut visual);
    }
}

#[gpui::test]
fn sibling_sort_changes_canvas_hit_order_and_drag_is_cancellable(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    pick(&mut visual, 110., 110.);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(3))
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-[");
    draw(&mut visual);
    pick(&mut visual, 110., 110.);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(2))
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    pick(&mut visual, 110., 110.);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(3))
        })
        .unwrap();
    click(&mut visual, "toggle-layers");
    let start = visual.debug_bounds("layer-1").unwrap().center();
    let target = visual.debug_bounds("layer-3").unwrap();
    let end = target.origin + point(px(40.), px(3.));
    let before = window
        .update(&mut visual.cx, |this, _, _| {
            this.history.borrow().undo_len()
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.layer_drag.as_ref().unwrap().target, Some((3, true)))
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.history.borrow().undo_len(), before)
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    pick(&mut visual, 110., 110.);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(1));
            assert_eq!(this.ordered_children(None), vec![2, 3, 1, 4]);
        })
        .unwrap();
}

#[gpui::test]
fn inline_names_commit_cancel_and_preserve_text_content(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    click(&mut visual, "toggle-layers");
    click(&mut visual, "layer-4");
    visual.simulate_keystrokes("f2");
    draw(&mut visual);
    assert!(visual.debug_bounds("layer-rename").is_some());
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("说明文字");
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.layer_name(4, cx), "说明文字");
            assert_eq!(this.texts[0].editor.read(cx).content, "保留文字内容");
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.layer_name(4, cx), "保留文字内容")
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    click(&mut visual, "layer-1");
    visual.simulate_keystrokes("f2");
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("不保存");
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].name, "Rectangle 1")
        })
        .unwrap();
    visual.simulate_keystrokes("f2");
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("卡片");
    click(&mut visual, "layer-2");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].name, "卡片")
        })
        .unwrap();
    click(&mut visual, "layer-1");
    visual.simulate_keystrokes("f2");
    draw(&mut visual);
    visual.simulate_input("取消改名");
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[0].name, "卡片");
            assert_eq!(this.layer_name(4, cx), "说明文字");
            assert_eq!(this.sidebar.renaming, None);
        })
        .unwrap();
}

#[gpui::test]
fn nested_groups_move_once_and_reparent_as_a_unit(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.set_selection(BTreeSet::from([1, 2]), cx)
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-g");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.selection_ids(), BTreeSet::from([5]));
            assert_eq!(this.layer_parent(1), Some(5));
            this.set_selection(BTreeSet::from([5, 4]), cx);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-g");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.selection_ids(), BTreeSet::from([6]));
            this.add_artboard(
                Rect {
                    x: 400.,
                    y: 0.,
                    width: 500.,
                    height: 500.,
                },
                cx,
            );
            this.set_selection(BTreeSet::from([6]), cx);
        })
        .unwrap();
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            let before = this.history.borrow().undo_len();
            this.batch_before = this.before_geometry();
            this.move_selection(point(450., 20.));
            this.finish_selection_move(cx);
            assert_eq!(this.shapes[0].rect.x, 80.);
            assert_eq!(this.shapes[0].board, Some(7));
            assert_eq!(this.shapes[1].rect.x, 110.);
            assert_eq!(this.texts[0].rect.x, 100.);
            assert_eq!(this.hierarchy.groups[&5].board, Some(7));
            assert_eq!(this.hierarchy.groups[&6].board, Some(7));
            assert_eq!(this.history.borrow().undo_len(), before + 1);
            this.replay_history(false, window, cx);
            assert_eq!(this.shapes[0].rect.x, 30.);
            assert_eq!(this.shapes[0].board, None);
            assert_eq!(this.selection_ids(), BTreeSet::from([6]));
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-shift-g");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.hierarchy.groups.contains_key(&6));
            assert_eq!(this.selection_ids(), BTreeSet::from([4, 5]));
            assert_eq!(this.layer_parent(1), Some(5));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([6]))
        })
        .unwrap();
}

#[gpui::test]
fn group_copy_delete_and_flags_preserve_members_and_names(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.hierarchy.names.insert(4, "标题".into());
            this.set_selection(BTreeSet::from([1, 4]), cx);
            this.group_selection(cx);
            this.shapes[0].layer.locked = true;
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-d");
    draw(&mut visual);
    let group = window
        .update(&mut visual.cx, |this, _, cx| {
            let id = *this.selection_ids().first().unwrap();
            assert!(id > 5);
            assert_eq!(this.hierarchy.groups.len(), 2);
            assert_eq!(this.shapes.len(), 4);
            assert_eq!(this.texts.len(), 2);
            let children = this.ordered_children(Some(id));
            assert_eq!(children.len(), 2);
            let copied_text = this.texts.last().unwrap();
            assert_eq!(this.layer_name(copied_text.id, cx), "标题");
            assert_eq!(this.layer_parent(copied_text.id), Some(id));
            assert!(this.shapes.last().unwrap().layer.locked);
            id
        })
        .unwrap();
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 3);
            assert_eq!(this.texts.len(), 1);
            assert!(!this.hierarchy.groups.contains_key(&group));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([group]))
        })
        .unwrap();
    click(&mut visual, "toggle-layers");
    let point = visual.debug_bounds("layer-visibility-8").unwrap().center();
    visual.simulate_click(point, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.hierarchy.groups[&group].layer.hidden);
            assert!(this.selection_ids().is_empty());
            for id in this.ordered_children(Some(group)) {
                assert!(!this.paint_order().contains(&id));
            }
        })
        .unwrap();
    visual.simulate_click(point, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes.last().unwrap().layer.locked)
        })
        .unwrap();
}

#[gpui::test]
fn group_pointer_selects_whole_and_properties_edit_members_without_panicking(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.set_selection(BTreeSet::from([1, 4]), cx);
            this.group_selection(cx);
            this.select(None, cx);
        })
        .unwrap();
    draw(&mut visual);
    pick(&mut visual, 40., 40.);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([5]))
        })
        .unwrap();
    let start = visual.debug_bounds("property-drag-1").unwrap().center();
    let end = start + point(px(20.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 50.);
            assert_eq!(this.texts[0].rect.x, 70.);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([5]))
        })
        .unwrap();
    click(&mut visual, "toggle-layers");
    click(&mut visual, "layer-1");
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    click(&mut visual, "layer-4");
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.hierarchy.groups.contains_key(&5))
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.hierarchy.groups.contains_key(&5));
            assert_eq!(this.layer_parent(4), Some(5));
        })
        .unwrap();
}

#[gpui::test]
fn groups_containing_boards_move_and_copy_children_in_their_own_coordinates(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(
                Rect {
                    x: 300.,
                    y: 0.,
                    width: 400.,
                    height: 300.,
                },
                cx,
            );
            let board = this.selected.unwrap();
            this.shapes[0].board = Some(board);
            this.set_selection(BTreeSet::from([board, 2]), cx);
            this.group_selection(cx);
            let group = *this.selection_ids().first().unwrap();
            this.batch_before = this.before_geometry();
            this.move_selection(point(100., 50.));
            this.finish_selection_move(cx);
            assert_eq!(this.boards[0].rect.x, 400.);
            assert_eq!(this.shapes[0].rect.x, 30.);
            assert_eq!(this.world_rect(1).unwrap().x, 430.);
            assert_eq!(this.shapes[1].rect.x, 160.);
            assert_eq!(this.hierarchy.groups[&group].board, None);
            this.duplicate_selection(window, cx);
            let copy = *this.selection_ids().first().unwrap();
            let members = this.descendants(&BTreeSet::from([copy]));
            let copied_board = this
                .boards
                .iter()
                .find(|b| members.contains(&b.id))
                .unwrap();
            let copied_child = this
                .shapes
                .iter()
                .find(|s| s.board == Some(copied_board.id))
                .unwrap();
            assert_eq!(copied_board.rect.x, 420.);
            assert_eq!(copied_child.rect.x, 30.);
            assert_eq!(this.world_rect(copied_child.id).unwrap().x, 450.);
            let copied_root = this
                .shapes
                .iter()
                .find(|s| members.contains(&s.id) && s.board.is_none())
                .unwrap();
            assert_eq!(copied_root.rect.x, 180.);
            this.replay_history(false, window, cx);
            assert!(!this.hierarchy.groups.contains_key(&copy));
            this.replay_history(false, window, cx);
            assert_eq!(this.boards[0].rect.x, 300.);
            assert_eq!(this.world_rect(1).unwrap().x, 330.);
            assert_eq!(this.selection_ids(), BTreeSet::from([group]));
        })
        .unwrap();
}
