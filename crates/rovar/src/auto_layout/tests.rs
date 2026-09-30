use super::*;
use crate::{
    artboard::Artboard,
    shape::{Shape, ShapeKind},
    text::styles::StyledText,
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
        crate::layer::LayerGroup {
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
        },
    );
    page.hierarchy.sizing.insert(
        1,
        Sizing {
            width: Mode::Hug,
            height: Mode::Hug,
            absolute: false,
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
        },
    );
    page.hierarchy.sizing.insert(
        2,
        Sizing {
            width: Mode::Hug,
            height: Mode::Hug,
            absolute: false,
        },
    );
    resolve(&mut page, &system).unwrap();
    let short = page.boards[0].rect.width;
    for align in [
        gpui::TextAlign::Center,
        gpui::TextAlign::Right,
        gpui::TextAlign::Left,
    ] {
        page.texts[0]
            .styles
            .apply(0..5, &crate::text::styles::StyleChange::Align(align), true);
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
