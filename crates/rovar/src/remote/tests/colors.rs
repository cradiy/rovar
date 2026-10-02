use super::*;
use crate::{
    document::library::{Library, colors},
    scene::color_styles::ColorStyle,
};

fn style(name: &str, color: u32) -> ColorStyle {
    ColorStyle {
        name: name.into(),
        color: gpui::rgb(color),
        gradient: Some(crate::scene::artboard::LinearGradient::default()),
    }
}

fn object(id: &str, title: &str, revision: i64, deleted: bool) -> Object {
    Object {
        id: id.into(),
        title: title.into(),
        kind: Kind::ColorStyle,
        revision,
        created: 1,
        modified: revision as u64,
        deleted,
    }
}

#[gpui::test]
fn existing_library_colors_upload_then_sync_edits_and_deletion(cx: &mut TestAppContext) {
    let id = uuid::Uuid::new_v4().to_string();
    let original = style("Brand", 0x8844cc);
    let edited = style("Primary", 0x4499cc);
    let session = serde_json::to_value(identity()).unwrap();
    let (url, saves, thread) = server(vec![
        (200, session.clone()),
        (
            200,
            serde_json::to_value(object(&id, "Brand", 1, false)).unwrap(),
        ),
        (200, session.clone()),
        (
            200,
            serde_json::to_value(object(&id, "Primary", 2, false)).unwrap(),
        ),
        (200, session),
        (
            200,
            serde_json::to_value(object(&id, "Primary", 3, true)).unwrap(),
        ),
    ]);
    let root = tempfile::tempdir().unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let connection = remote.update(cx, |r, cx| {
        r.connect(url, identity(), "token".into(), cx).unwrap()
    });
    let library_root = remote.read_with(cx, |r, _| r.library_root(&connection));
    write_atomic(
        &library_root.join("components/colors.json"),
        &serde_json::to_vec(&BTreeMap::from([(id.clone(), original.clone())])).unwrap(),
    )
    .unwrap();
    let library = cx.update(|cx| Library::open(&library_root, cx));
    cx.run_until_parked();
    sync(&remote, cx);
    library.update(cx, |lib, cx| {
        lib.set_color(id.clone(), Some(edited.clone()), cx).unwrap()
    });
    sync(&remote, cx);
    let mut temporary = edited.clone();
    temporary.name = "Temporary".into();
    library.update(cx, |lib, cx| {
        lib.set_color(id.clone(), Some(temporary), cx).unwrap();
        lib.set_color(id.clone(), Some(edited.clone()), cx).unwrap();
    });
    library.update(cx, |lib, cx| lib.set_color(id.clone(), None, cx).unwrap());
    sync(&remote, cx);
    thread.join().unwrap();
    let saves = saves.lock().unwrap();
    assert_eq!(saves.len(), 3);
    for (index, expected) in [original, edited].iter().enumerate() {
        let saved: Save = serde_json::from_value(saves[index].clone()).unwrap();
        assert_eq!(saved.kind, Kind::ColorStyle);
        assert_eq!(saved.base_revision, index as i64);
        assert_eq!(
            &serde_json::from_slice::<ColorStyle>(&STANDARD.decode(saved.content).unwrap())
                .unwrap(),
            expected
        );
    }
    assert_eq!(saves[2]["base_revision"], 2);
    assert_eq!(saves[2]["deleted"], true);
    assert_eq!(saves[2]["title"], "Primary");
    remote.read_with(cx, |r, _| {
        assert!(r.links().values().all(|link| !link.dirty && !link.conflict))
    });
    assert!(
        colors::read(&library_root.join("components"))
            .unwrap()
            .is_empty()
    );
}

#[gpui::test]
fn polling_downloads_colors_and_deletions_without_crossing_spaces(cx: &mut TestAppContext) {
    let id = uuid::Uuid::new_v4().to_string();
    let mut identity = identity();
    identity.spaces.push(rovar_api::Space {
        id: "team".into(),
        name: "Studio".into(),
        kind: "team".into(),
        role: "owner".into(),
    });
    let original = style("Shared", 0x1188aa);
    let edited = style("Renamed", 0x99bbcc);
    let session = serde_json::to_value(&identity).unwrap();
    let mut responses = Vec::new();
    for (revision, value) in [(1, Some(&original)), (2, Some(&edited)), (3, None)] {
        let object = object(
            &id,
            value.map_or("Renamed", |s| s.name.as_str()),
            revision,
            value.is_none(),
        );
        responses.push((200, session.clone()));
        responses.push((
            200,
            serde_json::to_value(rovar_api::Changes {
                objects: vec![object.clone()],
                cursor: revision,
                has_more: false,
            })
            .unwrap(),
        ));
        if let Some(value) = value {
            responses.push((
                200,
                serde_json::to_value(Snapshot {
                    media: Vec::new(),
                    object,
                    content: STANDARD.encode(serde_json::to_vec(value).unwrap()),
                })
                .unwrap(),
            ));
        }
    }
    let (url, _, thread) = server(responses);
    let root = tempfile::tempdir().unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let personal = remote.update(cx, |r, cx| {
        r.connect(url, identity, "token".into(), cx).unwrap()
    });
    let team = remote.read_with(cx, |r, _| {
        r.connections()
            .iter()
            .find(|c| c.space.kind == "team")
            .unwrap()
            .id
            .clone()
    });
    let personal_root = remote.read_with(cx, |r, _| r.library_root(&personal));
    let team_root = remote.read_with(cx, |r, _| r.library_root(&team));
    let private = style("Private", 0x111111);
    write_atomic(
        &personal_root.join("components/colors.json"),
        &serde_json::to_vec(&BTreeMap::from([(id.clone(), private.clone())])).unwrap(),
    )
    .unwrap();
    let library = cx.update(|cx| Library::open(&team_root, cx));
    cx.run_until_parked();
    for expected in [Some(original), Some(edited), None] {
        remote.update(cx, |r, _| {
            r.refresh_at.insert(
                team.clone(),
                web_time::Instant::now() - Duration::from_secs(6),
            );
            r.refresh_at
                .insert(personal.clone(), web_time::Instant::now());
        });
        sync(&remote, cx);
        remote.update(cx, |r, _| {
            assert!(r.error.is_none(), "{:?}", r.error);
            assert_eq!(r.take_library_updates(), BTreeSet::from([team.clone()]));
        });
        library.update(cx, |lib, cx| lib.refresh(cx));
        cx.run_until_parked();
        assert_eq!(
            library.read_with(cx, |lib, _| lib.colors.get(&id).cloned()),
            expected
        );
        assert_eq!(
            colors::read(&personal_root.join("components"))
                .unwrap()
                .get(&id),
            Some(&private)
        );
        remote.read_with(cx, |r, _| {
            assert!(r.links().values().all(|link| !link.dirty))
        });
    }
    thread.join().unwrap();
}

#[gpui::test]
fn conflicting_color_upload_preserves_a_copy_and_refetches_original(cx: &mut TestAppContext) {
    let id = uuid::Uuid::new_v4().to_string();
    let (url, _, thread) = server(vec![
        (200, serde_json::to_value(identity()).unwrap()),
        (
            409,
            serde_json::json!({"code":"revision_conflict", "message":"Newer color exists"}),
        ),
    ]);
    let root = tempfile::tempdir().unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let connection = remote.update(cx, |r, cx| {
        r.connect(url, identity(), "token".into(), cx).unwrap()
    });
    let library_root = remote.read_with(cx, |r, _| r.library_root(&connection));
    let library = cx.update(|cx| Library::open(&library_root, cx));
    cx.run_until_parked();
    let local = style("Brand", 0xff2211);
    library.update(cx, |lib, cx| {
        lib.set_color(id.clone(), Some(local.clone()), cx).unwrap()
    });
    sync(&remote, cx);
    thread.join().unwrap();
    let palette = colors::read(&library_root.join("components")).unwrap();
    assert!(!palette.contains_key(&id));
    let (copy_id, copy) = palette.iter().next().unwrap();
    assert_eq!(copy.color, local.color);
    assert_eq!(copy.gradient, local.gradient);
    assert_ne!(copy.name, local.name);
    remote.read_with(cx, |r, _| {
        let copy = &r.links()[&library_root.join("colors").join(format!("{copy_id}.json"))];
        assert!(copy.dirty && !copy.conflict);
        let original = &r.links()[&library_root.join("colors").join(format!("{id}.json"))];
        assert!(!original.dirty && !original.conflict);
        assert_eq!(original.object.revision, 0);
        assert!(!r.refresh_at.contains_key(&connection));
    });
}
