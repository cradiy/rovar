use super::*;
use serde_json::json;

fn snapshot(values: &[(&str, Value)]) -> Snapshot {
    Snapshot {
        blocks: values
            .iter()
            .map(|(key, value)| {
                (
                    key.to_string(),
                    Block {
                        kind: "json".into(),
                        data: Data::Json(value.clone()),
                    },
                )
            })
            .collect(),
    }
}

#[test]
fn single_property_changes_do_not_resend_unchanged_nodes_and_roundtrip_through_containers() {
    let mut base = snapshot(&[("document", json!({"id":"design", "schema":3}))]);
    for id in 0..200 {
        base.blocks.insert(format!("page/main/shape/{id}"), Block {kind: "json".into(), data: Data::Json(json!({"uid":format!("stable-{id}"), "rect":{"x":0,"y":0,"width":200,"height":100}, "style":{"fill":"purple", "gradient":[1,2,3]}, "content":"unchanged content".repeat(30)}))});
    }
    let mut next = base.clone();
    let Data::Json(node) = &mut next.blocks.get_mut("page/main/shape/42").unwrap().data else {
        unreachable!()
    };
    node["rect"]["x"] = 123.into();
    let delta = base.difference(&next).unwrap();
    assert_eq!(delta.changes.len(), 1);
    assert!(
        matches!(&delta.changes["page/main/shape/42"], Change::Patch(edits) if edits.len() == 1)
    );
    let patch = serde_json::to_vec(&delta).unwrap();
    let full = next.to_bytes(1024 * 1024).unwrap();
    assert!(
        patch.len() * 100 < full.len(),
        "{} byte patch versus {} byte snapshot",
        patch.len(),
        full.len()
    );
    let decoded: Delta = serde_json::from_slice(&patch).unwrap();
    let restored = base
        .apply(&decoded, 1024 * 1024)
        .unwrap()
        .to_bytes(1024 * 1024)
        .unwrap();
    assert_eq!(
        Snapshot::from_bytes(&restored, 1024 * 1024)
            .unwrap()
            .hash()
            .unwrap(),
        next.hash().unwrap()
    );
}

#[test]
fn missing_null_arrays_and_escaped_property_names_remain_distinct() {
    let base = snapshot(&[
        (
            "node",
            json!({"large":"padding".repeat(500), "remove":null, "a/b~c":{"x":1}, "array":[1,2]}),
        ),
        ("removed", json!({"id":2})),
    ]);
    let next = snapshot(&[
        (
            "node",
            json!({"large":"padding".repeat(500), "added":null, "a/b~c":{"x":2}, "array":[2,1]}),
        ),
        ("inserted", json!({"id":3})),
    ]);
    let wire = serde_json::to_vec(&base.difference(&next).unwrap()).unwrap();
    let delta = serde_json::from_slice(&wire).unwrap();
    assert_eq!(
        base.apply(&delta, 65536).unwrap().hash().unwrap(),
        next.hash().unwrap()
    );
    assert!(!base.blocks.contains_key("inserted"));
}

#[test]
fn stale_bases_tampered_results_invalid_paths_and_media_injection_are_rejected() {
    let base = snapshot(&[("node", json!({"padding":"text".repeat(100), "x":1}))]);
    let next = snapshot(&[("node", json!({"padding":"text".repeat(100), "x":2}))]);
    let mut delta = base.difference(&next).unwrap();
    assert!(next.apply(&delta, 65536).is_err());
    delta.result[0] ^= 1;
    assert!(base.apply(&delta, 65536).is_err());
    let mut delta = base.difference(&next).unwrap();
    delta.changes.insert(
        "node".into(),
        Change::Patch(vec![Edit::Set {
            path: vec!["missing".into(), "x".into()],
            value: json!(3),
        }]),
    );
    assert!(base.apply(&delta, 65536).is_err());
    delta.changes.insert(
        "media/injected".into(),
        Change::Put(Block {
            kind: "media".into(),
            data: Data::Bytes(STANDARD.encode(b"hidden media")),
        }),
    );
    assert!(base.apply(&delta, 65536).is_err());
    assert!(base.apply(&base.difference(&next).unwrap(), 10).is_err());
    assert_eq!(
        base.hash().unwrap(),
        snapshot(&[("node", json!({"padding":"text".repeat(100), "x":1}))])
            .hash()
            .unwrap()
    );
}

#[test]
fn compacted_and_reordered_json_have_identical_baselines_but_binary_changes_are_preserved() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("original.rovar");
    let compact = root.path().join("compact.rovar");
    let mut writer = Writer::create(&path).unwrap();
    writer
        .put_bytes("document", "json", br#"{"z":1,"a":{"z":2,"a":3}}"#)
        .unwrap();
    writer
        .put_bytes("preview", "image", b"preview one")
        .unwrap();
    writer
        .put_bytes("media/hash", "media", b"embedded media")
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let base = Snapshot::read(&Reader::open(&path).unwrap(), 65536).unwrap();
    assert!(!base.blocks.contains_key("media/hash"));
    crate::compact(&path, &compact).unwrap();
    let mut writer = Writer::open(&compact).unwrap();
    writer
        .put_bytes("document", "json", br#"{"a":{"a":3,"z":2},"z":1}"#)
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let same = Snapshot::read(&Reader::open(&compact).unwrap(), 65536).unwrap();
    assert_eq!(base.hash().unwrap(), same.hash().unwrap());
    let mut next = same;
    next.blocks.get_mut("preview").unwrap().data = Data::Bytes(STANDARD.encode(b"preview two"));
    let result = base.apply(&base.difference(&next).unwrap(), 65536).unwrap();
    assert_eq!(result.blocks["preview"].bytes().unwrap(), b"preview two");
}
