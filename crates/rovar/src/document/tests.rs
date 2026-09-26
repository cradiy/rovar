use super::*;
use crate::{artboard::FillMode, shape::ShapeKind};

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
    Document {
        id: uuid::Uuid::new_v4().to_string(),
        boards: vec![],
        shapes: vec![shape],
        texts: vec![],
        hierarchy: Hierarchy::default(),
        next_id: 2,
        assets: vec![],
    }
}

#[test]
fn file_round_trip_and_failed_save_preserve_the_previous_document() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.rovar");
    let mut document = fixture();
    let original = serde_json::to_vec(&document).unwrap();
    save(&path, &original, &[], &[]).unwrap();
    let decoded = Document::decode(&load(&path).unwrap().json).unwrap();
    assert_eq!(decoded.shapes, document.shapes);
    document.shapes[0].name = "changed".into();
    let bytes = serde_json::to_vec(&document).unwrap();
    let bad_asset = AssetSource {
        hash: "0".repeat(64),
        name: "missing.png".into(),
        path: crate::media::Source::file(tempfile::NamedTempFile::new().unwrap().into_temp_path()),
        size: [1, 1],
    };
    assert!(save(&path, &bytes, std::slice::from_ref(&bad_asset), &original).is_err());
    assert_eq!(load(&path).unwrap().json, original);
    assert!(save_as(&path, &bytes, &[bad_asset]).is_err());
    assert_eq!(load(&path).unwrap().json, original);
    document.id = uuid::Uuid::new_v4().to_string();
    assert!(
        save(
            &path,
            &serde_json::to_vec(&document).unwrap(),
            &[],
            &original
        )
        .is_err()
    );
    assert_eq!(load(&path).unwrap().json, original);
}

#[test]
fn embedded_assets_survive_source_removal_and_deduplicate() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("photo.png");
    image::RgbaImage::from_pixel(3, 2, image::Rgba([42, 70, 90, 255]))
        .save(&original)
        .unwrap();
    let asset = crate::media::MediaAsset::load(original.clone()).unwrap();
    std::fs::remove_file(&original).unwrap();
    let mut document = fixture();
    document.shapes[0].kind = ShapeKind::Image;
    document.assets.push(AssetUse {
        object: 1,
        fill: false,
        hash: asset.hash.clone(),
    });
    document.assets.push(AssetUse {
        object: 1,
        fill: true,
        hash: asset.hash.clone(),
    });
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
    assert_eq!(
        (
            loaded.assets[&asset.hash].width,
            loaded.assets[&asset.hash].height
        ),
        (3, 2)
    );
    let (import_result, imported_path) =
        import(&path, &directory.path().join("documents")).unwrap();
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

#[test]
fn rich_text_paths_groups_and_external_edit_conflicts_round_trip() {
    use crate::text::{
        TextStyle,
        styles::{StyleRun, StyledText},
    };
    let mut document = fixture();
    let content = "中文🙂 design".to_owned();
    let style = TextStyle::default();
    let mut accent = style.clone();
    accent.size = 37.;
    accent.color.a = 0.4321;
    document.texts.push(Text {
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
    document.hierarchy.groups.insert(
        3,
        crate::layer::LayerGroup {
            name: "Group".into(),
            board: None,
            layer: Default::default(),
        },
    );
    document.hierarchy.parents.extend([(1, 3), (2, 3)]);
    document.hierarchy.order = vec![3, 2, 1];
    document.next_id = 4;
    let nodes = [
        crate::bezier::Node::corner(gpui::point(12.3, 45.6)),
        crate::bezier::Node::corner(gpui::point(120., 94.)),
    ];
    document.shapes[0].kind = ShapeKind::Bezier;
    document.shapes[0].set_bezier(&nodes, false);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("text.rovar");
    let first = serde_json::to_vec(&document).unwrap();
    save(&path, &first, &[], &[]).unwrap();
    let decoded = Document::decode(&load(&path).unwrap().json).unwrap();
    assert_eq!(decoded.shapes, document.shapes);
    assert_eq!(decoded.hierarchy.order, document.hierarchy.order);
    assert!(decoded.texts[0].styles == document.texts[0].styles);
    document.texts[0].rect.width = 640.;
    let second = serde_json::to_vec(&document).unwrap();
    save(&path, &second, &[], &first).unwrap();
    assert!(save(&path, &first, &[], &first).is_err());
    assert_eq!(load(&path).unwrap().json, second);
    document.texts[0].styles.runs[0].range.end = 4;
    assert!(Document::decode(&serde_json::to_vec(&document).unwrap()).is_err());
}

#[test]
fn invalid_references_and_unsupported_version_are_rejected() {
    let mut document = fixture();
    document.hierarchy.parents.insert(1, 999);
    assert!(Document::decode(&serde_json::to_vec(&document).unwrap()).is_err());
    document.hierarchy.parents.clear();
    document.shapes.push(document.shapes[0].clone());
    assert!(document.validate().is_err());
    document.shapes.pop();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("future.rovar");
    save(&path, &serde_json::to_vec(&document).unwrap(), &[], &[]).unwrap();
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

#[test]
fn changed_object_is_appended_and_export_discards_obsolete_blocks() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.rovar");
    let mut document = fixture();
    let mut second = document.shapes[0].clone();
    second.id = 2;
    document.shapes.push(second);
    document.next_id = 3;
    let first = serde_json::to_vec(&document).unwrap();
    save_as(&path, &first, &[]).unwrap();
    let before = rovar_format::Reader::open(&path).unwrap();
    document.shapes[0].name = "Changed".into();
    let json = serde_json::to_vec(&document).unwrap();
    save(&path, &json, &[], &first).unwrap();
    let after = rovar_format::Reader::open(&path).unwrap();
    assert_eq!(before.entry("shape/2"), after.entry("shape/2"));
    assert_ne!(before.entry("shape/1"), after.entry("shape/1"));
    assert_eq!(
        before.read("shape/1", 65536).unwrap(),
        serde_json::to_vec(&fixture().shapes[0]).unwrap()
    );
    let export = directory.path().join("export.rovar");
    save_as(&export, &json, &[]).unwrap();
    assert!(std::fs::metadata(&export).unwrap().len() < std::fs::metadata(&path).unwrap().len());
    assert_eq!(load(&export).unwrap().json, json);
}
