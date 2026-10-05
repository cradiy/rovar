use super::*;
mod grid;
use crate::{
    scene::artboard::Artboard,
    scene::shape::{Shape, ShapeKind},
    scene::text::styles::StyledText,
};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}
fn fixture() -> Page {
    let mut page = Page::empty("Page 1".into());
    page.boards.push(Artboard {
        uid: uuid::Uuid::new_v4(),
        color_style: None,
        id: 1,
        name: "Frame".into(),
        rect: rect(100., 200., 300., 160.),
        layer: Default::default(),
        color: gpui::rgb(0xffffff),
        fill_mode: Default::default(),
        gradient: Default::default(),
        image_fill: Default::default(),
    });
    page.shapes.push(Shape::new(
        2,
        Some(1),
        ShapeKind::Rectangle,
        rect(60., 60., 50., 20.),
    ));
    page.shapes.push(Shape::new(
        3,
        Some(1),
        ShapeKind::Rectangle,
        rect(180., 60., 70., 40.),
    ));
    page.next_id = 4;
    let mut layout = Container::new(page.boards[0].rect);
    layout.padding = [10.; 4];
    layout.gap = 8.;
    page.hierarchy.layouts.insert(1, layout);
    page
}

#[gpui::test]
fn size_limits_bound_fill_and_force_wrapping_at_minimum_width(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = fixture();
    page.hierarchy.sizing.insert(
        2,
        Sizing {
            width: Mode::Fill,
            limits: Limits {
                min_width: Some(80.),
                max_width: Some(100.),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    page.hierarchy.sizing.insert(
        3,
        Sizing {
            width: Mode::Fill,
            limits: Limits {
                min_width: Some(70.),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[0].rect.width, 100.);
    assert_eq!(page.shapes[1].rect.width, 172.);
    page.hierarchy.layouts.get_mut(&1).unwrap().wrap = true;
    page.hierarchy.layouts.get_mut(&1).unwrap().line_gap = 15.;
    page.boards[0].rect.width = 130.;
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[0].rect, rect(10., 10., 100., 20.));
    assert_eq!(page.shapes[1].rect, rect(10., 45., 110., 40.));
    let bytes = serde_json::to_vec(&page).unwrap();
    page = Page::decode(&bytes).unwrap();
    resolve(&mut page, &system).unwrap();
    assert_eq!(serde_json::to_vec(&page).unwrap(), bytes);
    page.hierarchy.sizing.get_mut(&2).unwrap().limits.max_width = Some(79.);
    assert!(page.validate().is_err());
}

#[gpui::test]
fn size_limits_cap_hug_text_and_preserve_center_constraints(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = fixture();
    page.hierarchy.layouts.clear();
    page.shapes.clear();
    let content = "A long label which wraps onto several lines";
    let mut styles = StyledText::default();
    styles.replace(0..0, content.len());
    page.texts.push(crate::document::Text {
        uid: uuid::Uuid::new_v4(),
        id: 2,
        board: Some(1),
        rect: rect(100., 40., 100., 30.),
        layer: Default::default(),
        content: content.into(),
        styles,
    });
    page.hierarchy.sizing.insert(
        2,
        Sizing {
            width: Mode::Hug,
            height: Mode::Hug,
            limits: Limits {
                max_width: Some(100.),
                ..Default::default()
            },
            constraints: Some(Constraints {
                horizontal: Constraint::Center,
                vertical: Constraint::Start,
                rect: page.texts[0].rect,
                parent_size: [300., 160.],
            }),
            ..Default::default()
        },
    );
    resolve(&mut page, &system).unwrap();
    let wrapped = page.texts[0].rect;
    assert!(wrapped.width <= 100.);
    assert_eq!(wrapped.x + wrapped.width / 2., 150.);
    page.hierarchy.sizing.get_mut(&2).unwrap().limits = Limits::default();
    resolve(&mut page, &system).unwrap();
    let unwrapped = page.texts[0].rect;
    assert!(unwrapped.width > wrapped.width);
    assert!(unwrapped.height < wrapped.height);
    assert_eq!(unwrapped.x + unwrapped.width / 2., 150.);
}

#[gpui::test]
fn constraints_resize_restore_and_round_trip_without_drift(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    for (mode, expected) in [
        (Constraint::Start, rect(60., 60., 50., 20.)),
        (Constraint::End, rect(360., 220., 50., 20.)),
        (Constraint::Center, rect(210., 140., 50., 20.)),
        (Constraint::Stretch, rect(60., 60., 350., 180.)),
        (Constraint::Scale, rect(120., 120., 100., 40.)),
    ] {
        let mut page = fixture();
        page.hierarchy.layouts.clear();
        page.hierarchy.sizing.insert(
            2,
            Sizing {
                constraints: Some(Constraints {
                    horizontal: mode,
                    vertical: mode,
                    rect: page.shapes[0].rect,
                    parent_size: [300., 160.],
                }),
                ..Default::default()
            },
        );
        let original = page.shapes[0].rect;
        page.boards[0].rect.width = 600.;
        page.boards[0].rect.height = 320.;
        resolve(&mut page, &system).unwrap();
        assert_eq!(page.shapes[0].rect, expected, "{mode:?}");
        let encoded = serde_json::to_vec(&page).unwrap();
        page = Page::decode(&encoded).unwrap();
        resolve(&mut page, &system).unwrap();
        assert_eq!(serde_json::to_vec(&page).unwrap(), encoded);
        page.boards[0].rect.width = 1.;
        page.boards[0].rect.height = 1.;
        resolve(&mut page, &system).unwrap();
        page.boards[0].rect.width = 300.;
        page.boards[0].rect.height = 160.;
        resolve(&mut page, &system).unwrap();
        assert_eq!(page.shapes[0].rect, original, "restore {mode:?}");
    }
}

#[gpui::test]
fn wrapped_rows_and_columns_use_separate_gaps_and_hug_cross_axis(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = fixture();
    let layout = page.hierarchy.layouts.get_mut(&1).unwrap();
    layout.wrap = true;
    layout.line_gap = 15.;
    page.boards[0].rect.width = 130.;
    page.hierarchy.sizing.insert(
        1,
        Sizing {
            height: Mode::Hug,
            ..Default::default()
        },
    );
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[0].rect, rect(10., 10., 50., 20.));
    assert_eq!(page.shapes[1].rect, rect(10., 45., 70., 40.));
    assert_eq!(page.boards[0].rect.height, 95.);
    page.boards[0].rect.width = 148.;
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[1].rect, rect(68., 10., 70., 40.));
    assert_eq!(page.boards[0].rect.height, 60.);
    page.hierarchy.layouts.get_mut(&1).unwrap().axis = Axis::Vertical;
    page.boards[0].rect.height = 75.;
    page.hierarchy.sizing.insert(
        1,
        Sizing {
            width: Mode::Hug,
            ..Default::default()
        },
    );
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[1].rect, rect(75., 10., 70., 40.));
    assert_eq!(page.boards[0].rect.width, 155.);
}

#[gpui::test]
fn absolute_constraints_inside_padded_layout_do_not_enter_flow(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = fixture();
    page.hierarchy.sizing.insert(
        2,
        Sizing {
            absolute: true,
            constraints: Some(Constraints {
                horizontal: Constraint::End,
                vertical: Constraint::Stretch,
                rect: page.shapes[0].rect,
                parent_size: [300., 160.],
            }),
            ..Default::default()
        },
    );
    page.boards[0].rect.width = 400.;
    page.boards[0].rect.height = 260.;
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[0].rect, rect(160., 60., 50., 120.));
    assert_eq!(page.shapes[1].rect, rect(10., 10., 70., 40.));
}

#[gpui::test]
fn component_instances_keep_internal_constraints_when_resized(cx: &mut gpui::TestAppContext) {
    use crate::scene::components;
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = fixture();
    page.hierarchy.layouts.clear();
    page.hierarchy.sizing.insert(
        2,
        Sizing {
            constraints: Some(Constraints {
                horizontal: Constraint::End,
                vertical: Constraint::Center,
                rect: page.shapes[0].rect,
                parent_size: [300., 160.],
            }),
            ..Default::default()
        },
    );
    let template = components::extract(&page, 1).unwrap();
    let mut instance = components::place(
        &template,
        &BTreeMap::from([(1, 10), (2, 11), (3, 12)]),
        [700., 400.],
        None,
    );
    instance.boards[0].rect.width = 500.;
    instance.boards[0].rect.height = 360.;
    resolve(&mut instance, &system).unwrap();
    assert_eq!(instance.shapes[0].rect, rect(260., 160., 50., 20.));
    instance.validate().unwrap();
    let shape_template = components::extract(&page, 2).unwrap();
    assert!(shape_template.hierarchy.sizing[&2].constraints.is_none());
}

#[gpui::test]
fn centered_hug_text_keeps_its_center_when_content_changes(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = fixture();
    page.hierarchy.layouts.clear();
    page.shapes.clear();
    let mut styles = StyledText::default();
    styles.replace(0..0, 5);
    page.texts.push(crate::document::Text {
        uid: uuid::Uuid::new_v4(),
        id: 2,
        board: Some(1),
        rect: rect(100., 50., 100., 30.),
        layer: Default::default(),
        content: "Hello".into(),
        styles,
    });
    page.hierarchy.sizing.insert(
        2,
        Sizing {
            width: Mode::Hug,
            height: Mode::Hug,
            constraints: Some(Constraints {
                horizontal: Constraint::Center,
                vertical: Constraint::Center,
                rect: page.texts[0].rect,
                parent_size: [300., 160.],
            }),
            ..Default::default()
        },
    );
    resolve(&mut page, &system).unwrap();
    let first = page.texts[0].rect;
    page.texts[0].content = "A much longer label".into();
    let length = page.texts[0].content.len();
    page.texts[0].styles.replace(0..5, length);
    resolve(&mut page, &system).unwrap();
    let after = page.texts[0].rect;
    assert!(after.width > first.width);
    assert_eq!(first.x + first.width / 2., after.x + after.width / 2.);
    assert_eq!(first.y + first.height / 2., after.y + after.height / 2.);
}
#[gpui::test]
fn flex_fixed_fill_absolute_hidden_and_reordering(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = fixture();
    page.hierarchy.layouts.get_mut(&1).unwrap().cross = Align::Center;
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[0].rect, rect(10., 70., 50., 20.));
    assert_eq!(page.shapes[1].rect, rect(68., 60., 70., 40.));
    page.hierarchy.sizing.insert(
        3,
        Sizing {
            width: Mode::Fill,
            height: Mode::Fill,
            absolute: false,
            ..Default::default()
        },
    );
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[1].rect, rect(68., 10., 222., 140.));
    page.hierarchy.order = vec![3, 2];
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[0].rect.x, 240.);
    page.hierarchy.sizing.get_mut(&3).unwrap().absolute = true;
    page.shapes[1].rect = rect(180., 75., 25., 25.);
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[1].rect, rect(180., 75., 25., 25.));
    assert_eq!(page.shapes[0].rect.x, 10.);
    page.hierarchy.sizing.remove(&3);
    page.shapes[0].layer.hidden = true;
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[1].rect.x, 10.);
}
#[gpui::test]
fn nested_hug_frames_are_stable_and_serialize_without_ui_state(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = fixture();
    page.hierarchy.groups.insert(
        4,
        crate::scene::layer::LayerGroup {
            boolean: None,
            uid: uuid::Uuid::new_v4(),
            name: "Stack".into(),
            board: Some(1),
            layer: Default::default(),
        },
    );
    page.hierarchy.parents.insert(2, 4);
    page.hierarchy.parents.insert(3, 4);
    let mut nested = Container::new(rect(0., 0., 100., 100.));
    nested.axis = Axis::Vertical;
    nested.gap = 5.;
    nested.padding = [4.; 4];
    page.hierarchy.layouts.insert(4, nested);
    page.hierarchy.sizing.insert(
        4,
        Sizing {
            width: Mode::Hug,
            height: Mode::Hug,
            absolute: false,
            ..Default::default()
        },
    );
    page.hierarchy.sizing.insert(
        1,
        Sizing {
            width: Mode::Hug,
            height: Mode::Hug,
            absolute: false,
            ..Default::default()
        },
    );
    page.next_id = 5;
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.hierarchy.layouts[&4].frame, rect(10., 10., 78., 73.));
    assert_eq!(page.boards[0].rect, rect(100., 200., 98., 93.));
    assert_eq!(page.shapes[1].rect, rect(14., 39., 70., 40.));
    let first = serde_json::to_vec(&page).unwrap();
    resolve(&mut page, &system).unwrap();
    assert_eq!(first, serde_json::to_vec(&page).unwrap());
    Page::decode(&first).unwrap();
    page.hierarchy.layouts.get_mut(&4).unwrap().gap = f32::NAN;
    assert!(page.validate().is_err());
}
#[gpui::test]
fn hug_text_resizes_its_container_and_wraps_at_fixed_width(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = fixture();
    page.shapes.clear();
    let mut styles = StyledText::default();
    styles.replace(0..0, 5);
    page.texts.push(crate::document::Text {
        uid: uuid::Uuid::new_v4(),
        id: 2,
        board: Some(1),
        rect: rect(0., 0., 80., 20.),
        layer: Default::default(),
        content: "Hello".into(),
        styles,
    });
    page.hierarchy.sizing.insert(
        1,
        Sizing {
            width: Mode::Hug,
            height: Mode::Hug,
            absolute: false,
            ..Default::default()
        },
    );
    page.hierarchy.sizing.insert(
        2,
        Sizing {
            width: Mode::Hug,
            height: Mode::Hug,
            absolute: false,
            ..Default::default()
        },
    );
    resolve(&mut page, &system).unwrap();
    let short = page.boards[0].rect.width;
    for align in [
        gpui::TextAlign::Center,
        gpui::TextAlign::Right,
        gpui::TextAlign::Left,
    ] {
        page.texts[0].styles.apply(
            0..5,
            &crate::scene::text::styles::StyleChange::Align(align),
            true,
        );
        resolve(&mut page, &system).unwrap();
        assert!((page.boards[0].rect.width - short).abs() < 0.1);
    }
    page.texts[0].content = "Hello from a longer button label".into();
    let length = page.texts[0].content.len();
    page.texts[0].styles.replace(0..5, length);
    resolve(&mut page, &system).unwrap();
    assert!(page.boards[0].rect.width > short);
    assert_eq!(page.boards[0].rect.width, page.texts[0].rect.width + 20.);
    let line_height = page.texts[0].rect.height;
    page.hierarchy.sizing.get_mut(&2).unwrap().width = Mode::Fixed;
    page.texts[0].rect.width = 80.;
    resolve(&mut page, &system).unwrap();
    assert!(page.texts[0].rect.height > line_height);
    assert_eq!(page.boards[0].rect.width, 100.);
}
