use super::*;
use crate::{scene::artboard::FillMode, scene::shape::ShapeKind};

#[gpui::test]
fn single_page_files_upgrade_on_save_and_keep_their_identity(cx: &mut gpui::TestAppContext) {
    let text_system = cx.update(|cx| cx.text_system().clone());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("single.rovar");
    let original = fixture();
    let page = &original.pages[0];
    let mut writer = rovar_format::Writer::create(&path).unwrap();
    let manifest = serde_json::json!({
        "schema": 1, "id": original.id, "next_id": page.next_id,
        "boards": [], "shapes": [1], "texts": [], "assets": [], "media": {}
    });
    writer
        .put_bytes(
            "document",
            "document",
            &serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    writer
        .put_bytes(
            "hierarchy",
            "json",
            &serde_json::to_vec(&page.hierarchy).unwrap(),
        )
        .unwrap();
    writer
        .put_bytes(
            "shape/1",
            "json",
            &serde_json::to_vec(&page.shapes[0]).unwrap(),
        )
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let bytes = std::fs::read(&path).unwrap();
    let loaded = load(&path).unwrap();
    assert!(loaded.needs_upgrade);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut document = Document::decode(&loaded.json).unwrap();
    assert_eq!(document.id, original.id);
    assert_eq!(document.pages.len(), 1);
    assert_eq!(document.pages[0].name, "Page 1");
    assert_eq!(document.pages[0].shapes, page.shapes);
    document.pages.push(Page::empty("Details".into()));
    save(
        &path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &loaded.json,
        &text_system,
    )
    .unwrap();
    let saved = load(&path).unwrap();
    assert!(!saved.needs_upgrade);
    assert_eq!(saved.json, serde_json::to_vec(&document).unwrap());
    let reader = rovar_format::Reader::open(&path).unwrap();
    assert!(reader.entry("shape/1").is_none());
    let manifest: serde_json::Value =
        serde_json::from_slice(&reader.read("document", 65536).unwrap()).unwrap();
    assert_eq!(manifest["schema"], 3);
}

#[gpui::test]
fn reordering_pages_updates_the_preview_and_preserves_unchanged_blocks(
    cx: &mut gpui::TestAppContext,
) {
    let text_system = cx.update(|cx| cx.text_system().clone());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pages.rovar");
    let mut document = fixture();
    let mut second = document.pages[0].clone();
    second.id = uuid::Uuid::new_v4().to_string();
    second.name = "Details".into();
    second.shapes[0].rect.x += 300.;
    second.shapes[0].color = gpui::rgb(0x3366ff);
    let key = format!("page/{}/shape/1", second.id);
    document.pages.push(second);
    let first = serde_json::to_vec(&document).unwrap();
    save_as(&path, &first, &[], &text_system).unwrap();
    let mut writer = rovar_format::Writer::open(&path).unwrap();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&writer.snapshot().read("document", 65536).unwrap()).unwrap();
    manifest["cover"] = document.pages[1].id.clone().into();
    writer
        .put_bytes(
            "document",
            "document",
            &serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let loaded = load(&path).unwrap();
    assert!(loaded.needs_upgrade);
    assert_eq!(loaded.json, first);
    let before = rovar_format::Reader::open(&path).unwrap();
    let preview = before.read("preview", 1024 * 1024).unwrap();
    document.pages.reverse();
    document.pages[0].shapes[0].name = "Details object".into();
    let next = serde_json::to_vec(&document).unwrap();
    save(&path, &next, &[], &first, &text_system).unwrap();
    assert_eq!(load(&path).unwrap().json, next);
    let after = rovar_format::Reader::open(&path).unwrap();
    assert_ne!(before.entry(&key), after.entry(&key));
    let unchanged = format!("page/{}/shape/1", document.pages[1].id);
    assert_eq!(before.entry(&unchanged), after.entry(&unchanged));
    assert_ne!(preview, after.read("preview", 1024 * 1024).unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&after.read("document", 65536).unwrap()).unwrap();
    assert!(manifest.get("cover").is_none());
    assert!(!load(&path).unwrap().needs_upgrade);
    document
        .pages
        .insert(0, Page::empty("Empty first page".into()));
    save(
        &path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &next,
        &text_system,
    )
    .unwrap();
    assert!(
        rovar_format::Reader::open(&path)
            .unwrap()
            .entry("preview")
            .is_none()
    );
    document.pages[1].id = document.pages[0].id.clone();
    assert!(document.validate().is_err());
    document.pages.clear();
    assert!(document.validate().is_err());
}

fn fixture() -> Document {
    let mut shape = Shape::new(
        1,
        None,
        ShapeKind::Star,
        Rect {
            x: 31.25,
            y: -8.,
            width: 240.,
            height: 120.,
        },
    );
    shape.color = gpui::Rgba {
        r: 0.12345,
        g: 0.56789,
        b: 0.98765,
        a: 0.42123,
    };
    shape.gradient.kind = gpui::GradientKind::Diamond;
    shape.fill_mode = FillMode::Linear;
    Document::single(crate::document::Page {
        name: "Page 1".into(),
        id: uuid::Uuid::new_v4().to_string(),
        boards: vec![],
        shapes: vec![shape],
        texts: vec![],
        hierarchy: Hierarchy::default(),
        next_id: 2,
        assets: vec![],
    })
}

#[gpui::test]
fn file_round_trip_and_failed_save_preserve_the_previous_document(cx: &mut gpui::TestAppContext) {
    let text_system = cx.update(|cx| cx.text_system().clone());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.rovar");
    let mut document = fixture();
    let original = serde_json::to_vec(&document).unwrap();
    save(&path, &original, &[], &[], &text_system).unwrap();
    let decoded = Document::decode(&load(&path).unwrap().json).unwrap();
    assert_eq!(decoded.pages[0].shapes, document.pages[0].shapes);
    document.pages[0].shapes[0].name = "changed".into();
    let bytes = serde_json::to_vec(&document).unwrap();
    let bad_asset = AssetSource {
        hash: "0".repeat(64),
        name: "missing.png".into(),
        path: crate::media::Source::file(tempfile::NamedTempFile::new().unwrap().into_temp_path()),
        size: [1, 1],
    };
    assert!(
        save(
            &path,
            &bytes,
            std::slice::from_ref(&bad_asset),
            &original,
            &text_system
        )
        .is_err()
    );
    assert_eq!(load(&path).unwrap().json, original);
    assert!(save_as(&path, &bytes, &[bad_asset], &text_system).is_err());
    assert_eq!(load(&path).unwrap().json, original);
    document.id = uuid::Uuid::new_v4().to_string();
    assert!(
        save(
            &path,
            &serde_json::to_vec(&document).unwrap(),
            &[],
            &original,
            &text_system
        )
        .is_err()
    );
    assert_eq!(load(&path).unwrap().json, original);
}

#[gpui::test]
fn embedded_assets_survive_source_removal_and_deduplicate(cx: &mut gpui::TestAppContext) {
    let text_system = cx.update(|cx| cx.text_system().clone());
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("photo.png");
    image::RgbaImage::from_pixel(3, 2, image::Rgba([42, 70, 90, 255]))
        .save(&original)
        .unwrap();
    let asset = crate::media::MediaAsset::load(original.clone()).unwrap();
    std::fs::remove_file(&original).unwrap();
    let mut document = fixture();
    document.pages[0].shapes[0].kind = ShapeKind::Image;
    document.pages[0].assets.push(AssetUse {
        object: 1,
        fill: false,
        hash: asset.hash.clone(),
    });
    document.pages[0].assets.push(AssetUse {
        object: 1,
        fill: true,
        hash: asset.hash.clone(),
    });
    let mut second = document.pages[0].clone();
    second.id = uuid::Uuid::new_v4().to_string();
    second.name = "Shared media".into();
    document.pages.push(second);
    let source = AssetSource {
        hash: asset.hash.clone(),
        name: asset.name(),
        path: asset.source.clone(),
        size: [asset.width, asset.height],
    };
    let path = directory.path().join("media.rovar");
    save(
        &path,
        &serde_json::to_vec(&document).unwrap(),
        &[source.clone(), source],
        &[],
        &text_system,
    )
    .unwrap();
    let loaded = load(&path).unwrap();
    let preview = cache_preview(&path, &directory.path().join("previews"))
        .unwrap()
        .unwrap();
    let pixels = image::open(directory.path().join("previews").join(preview))
        .unwrap()
        .into_rgba8();
    assert_eq!(*pixels.get_pixel(280, 168), image::Rgba([42, 70, 90, 255]));
    assert_eq!(loaded.assets.len(), 1);
    let decoded = load(&path).unwrap().into_document().unwrap();
    assert!(Arc::ptr_eq(
        decoded.pages[0].shapes[0].media.as_ref().unwrap(),
        decoded.pages[1].shapes[0].media.as_ref().unwrap(),
    ));
    assert_eq!(
        (
            loaded.assets[&asset.hash].width,
            loaded.assets[&asset.hash].height
        ),
        (3, 2)
    );
    let (import_result, imported_path) =
        import(&path, &directory.path().join("documents"), &text_system).unwrap();
    let imported = load(&imported_path).unwrap();
    assert_ne!(Document::decode(&imported.json).unwrap().id, document.id);
    assert_eq!(
        imported.assets[&asset.hash].source.bytes(),
        asset.source.bytes()
    );
    let reader = rovar_format::Reader::open(&path).unwrap();
    assert_eq!(
        reader.entries().filter(|(_, b)| b.kind == "media").count(),
        1
    );
    assert!(loaded.assets[&asset.hash].is_pending());
    loaded.assets[&asset.hash].ensure_decoded().unwrap();
    let block = reader.entry(&format!("media/{}", asset.hash)).unwrap();
    use std::io::{Seek, SeekFrom, Write};
    let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.seek(SeekFrom::Start(block.offset)).unwrap();
    file.write_all(b"X").unwrap();
    // Opening only reads structure; damage is detected when this media is requested.
    let damaged = load(&path).unwrap();
    assert!(damaged.assets[&asset.hash].ensure_decoded().is_err());
    import_result.assets[&asset.hash].ensure_decoded().unwrap();
    assert_eq!(load(&imported_path).unwrap().assets.len(), 1);
}

#[gpui::test]
fn rich_text_paths_groups_and_external_edit_conflicts_round_trip(cx: &mut gpui::TestAppContext) {
    let text_system = cx.update(|cx| cx.text_system().clone());
    use crate::scene::text::{
        TextStyle,
        styles::{StyleRun, StyledText},
    };
    let mut document = fixture();
    let content = "中文🙂 design".to_owned();
    let style = TextStyle::default();
    let mut accent = style.clone();
    accent.size = 37.;
    accent.color.a = 0.4321;
    document.pages[0].texts.push(Text {
        uid: uuid::Uuid::new_v4(),
        id: 2,
        board: None,
        rect: Rect {
            x: 20.,
            y: 30.,
            width: 320.,
            height: 120.,
        },
        layer: Default::default(),
        content: content.clone(),
        styles: StyledText {
            default: style.clone(),
            runs: vec![
                StyleRun {
                    range: 0..10,
                    style: accent,
                },
                StyleRun {
                    range: 10..content.len(),
                    style,
                },
            ],
        },
    });
    document.pages[0].hierarchy.groups.insert(
        3,
        crate::scene::layer::LayerGroup {
            boolean: None,
            mask: None,
            uid: uuid::Uuid::new_v4(),
            name: "Group".into(),
            board: None,
            layer: Default::default(),
        },
    );
    document.pages[0].hierarchy.parents.extend([(1, 3), (2, 3)]);
    document.pages[0].hierarchy.order = vec![3, 2, 1];
    document.pages[0].next_id = 4;
    let nodes = [
        crate::scene::bezier::Node::corner(gpui::point(12.3, 45.6)),
        crate::scene::bezier::Node::corner(gpui::point(120., 94.)),
    ];
    document.pages[0].shapes[0].kind = ShapeKind::Bezier;
    document.pages[0].shapes[0].set_bezier(&nodes, false);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("text.rovar");
    let first = serde_json::to_vec(&document).unwrap();
    save(&path, &first, &[], &[], &text_system).unwrap();
    let decoded = Document::decode(&load(&path).unwrap().json).unwrap();
    assert_eq!(decoded.pages[0].shapes, document.pages[0].shapes);
    assert_eq!(
        decoded.pages[0].hierarchy.order,
        document.pages[0].hierarchy.order
    );
    assert!(decoded.pages[0].texts[0].styles == document.pages[0].texts[0].styles);
    document.pages[0].texts[0].rect.width = 640.;
    let second = serde_json::to_vec(&document).unwrap();
    save(&path, &second, &[], &first, &text_system).unwrap();
    assert!(save(&path, &first, &[], &first, &text_system).is_err());
    assert_eq!(load(&path).unwrap().json, second);
    document.pages[0].texts[0].styles.runs[0].range.end = 4;
    assert!(Document::decode(&serde_json::to_vec(&document).unwrap()).is_err());
}

#[gpui::test]
fn component_baselines_allow_consecutive_saves_with_fractional_geometry(
    cx: &mut gpui::TestAppContext,
) {
    let text_system = cx.update(|cx| cx.text_system().clone());
    let mut document = fixture();
    document.pages[0].shapes[0].rect.x = 687.28;
    document.pages[0].shapes[0].rect.y = 339.17;
    document.pages[0].shapes[0].color = gpui::rgb(0xd9d9d9);
    let id = uuid::Uuid::new_v4().to_string();
    let template = crate::scene::components::extract(&document.pages[0], 1).unwrap();
    document.pages[0].hierarchy.components.insert(
        1,
        crate::scene::components::Binding {
            component: id.clone(),
            master: true,
            nodes: [(1, 1)].into(),
            baseline: serde_json::to_value(&template).unwrap(),
        },
    );
    document.components.insert(
        id,
        crate::scene::components::Definition {
            name: "Card".into(),
            source: None,
            root: 1,
            page: template,
        },
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("component.rovar");
    let first = serde_json::to_vec(&document).unwrap();
    save(&path, &first, &[], &[], &text_system).unwrap();
    assert_eq!(load(&path).unwrap().json, first);
    document.pages[0].shapes[0].rect.width += 10.;
    let second = serde_json::to_vec(&document).unwrap();
    save(&path, &second, &[], &first, &text_system).unwrap();
    assert_eq!(load(&path).unwrap().json, second);
    assert!(save(&path, &first, &[], &first, &text_system).is_err());
}

#[gpui::test]
fn invalid_references_and_unsupported_version_are_rejected(cx: &mut gpui::TestAppContext) {
    let text_system = cx.update(|cx| cx.text_system().clone());
    let mut document = fixture();
    document.pages[0].hierarchy.parents.insert(1, 999);
    assert!(Document::decode(&serde_json::to_vec(&document).unwrap()).is_err());
    document.pages[0].hierarchy.parents.clear();
    let duplicate = document.pages[0].shapes[0].clone();
    document.pages[0].shapes.push(duplicate);
    assert!(document.validate().is_err());
    document.pages[0].shapes.pop();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("future.rovar");
    save(
        &path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &[],
        &text_system,
    )
    .unwrap();
    use std::io::{Seek, SeekFrom, Write};
    let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.seek(SeekFrom::Start(8)).unwrap();
    file.write_all(&2u16.to_le_bytes()).unwrap();
    file.write_all(&0u16.to_le_bytes()).unwrap();
    assert!(
        load(&path)
            .err()
            .unwrap()
            .to_string()
            .contains("Unsupported Rovar container version: 2.0 (supported: 1.0)")
    );
}

#[gpui::test]
fn changed_object_is_appended_and_export_discards_obsolete_blocks(cx: &mut gpui::TestAppContext) {
    let text_system = cx.update(|cx| cx.text_system().clone());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.rovar");
    let mut document = fixture();
    let mut second = document.pages[0].shapes[0].clone();
    second.id = 2;
    second.uid = uuid::Uuid::new_v4();
    document.pages[0].shapes.push(second);
    document.pages[0].next_id = 3;
    let first = serde_json::to_vec(&document).unwrap();
    save_as(&path, &first, &[], &text_system).unwrap();
    let before = rovar_format::Reader::open(&path).unwrap();
    let original_shape = document.pages[0].shapes[0].clone();
    document.pages[0].shapes[0].name = "Changed".into();
    let json = serde_json::to_vec(&document).unwrap();
    save(&path, &json, &[], &first, &text_system).unwrap();
    let after = rovar_format::Reader::open(&path).unwrap();
    let key = |id| format!("page/{}/shape/{id}", document.pages[0].id);
    assert_eq!(before.entry(&key(2)), after.entry(&key(2)));
    assert_ne!(before.entry(&key(1)), after.entry(&key(1)));
    assert_eq!(
        before.read(&key(1), 65536).unwrap(),
        serde_json::to_vec(&original_shape).unwrap()
    );
    let export = directory.path().join("export.rovar");
    save_as(&export, &json, &[], &text_system).unwrap();
    assert!(std::fs::metadata(&export).unwrap().len() < std::fs::metadata(&path).unwrap().len());
    assert_eq!(load(&export).unwrap().json, json);
}
