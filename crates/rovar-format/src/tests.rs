use super::*;
use std::io::{Read, Seek, SeekFrom, Write};

#[test]
fn append_reuses_blocks_streams_independently_and_compacts_live_data() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("a.rovar");
    let mut writer = Writer::create(&path).unwrap();
    writer.put_bytes("scene", "json", b"first").unwrap();
    writer.put_bytes("media", "media", b"0123456789").unwrap();
    writer.commit().unwrap();
    let before = writer.snapshot();
    let offset = before.entry("media").unwrap().offset;
    let pinned = before.block("scene").unwrap();
    let mut a = before.block("media").unwrap().reader();
    let mut b = before.block("media").unwrap().reader();
    a.seek(SeekFrom::Start(7)).unwrap();
    let mut bytes = [0; 2];
    b.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"01");
    a.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"78");
    writer.put_bytes("scene", "json", b"second").unwrap();
    writer.put_bytes("media", "media", b"0123456789").unwrap();
    writer.commit().unwrap();
    assert_eq!(writer.snapshot().entry("media").unwrap().offset, offset);
    assert_eq!(before.read("scene", 10).unwrap(), b"first");
    writer.remove("media");
    writer.commit().unwrap();
    drop(writer);
    let length = std::fs::metadata(&path).unwrap().len();
    compact(&path, &path).unwrap();
    let reader = Reader::open(&path).unwrap();
    assert!(reader.entry("media").is_none());
    assert_eq!(reader.read("scene", 10).unwrap(), b"second");
    assert!(reader.committed_length() < length);
    let mut old = Vec::new();
    pinned.copy_verified(&mut old).unwrap();
    assert_eq!(old, b"first");
}

#[test]
fn failed_stream_and_interrupted_publish_leave_last_complete_commit() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("a.rovar");
    let mut writer = Writer::create(&path).unwrap();
    writer.put_bytes("scene", "json", b"first").unwrap();
    writer.commit().unwrap();
    assert!(
        writer
            .put("scene", "json", &b"bad"[..], Some([0; 32]))
            .is_err()
    );
    drop(writer);
    assert_eq!(
        Reader::open(&path).unwrap().read("scene", 10).unwrap(),
        b"first"
    );
    let original = std::fs::read(&path).unwrap();
    let mut writer = Writer::open(&path).unwrap();
    let prior_end = writer.snapshot().committed_length();
    writer.put_bytes("scene", "json", b"second").unwrap();
    writer.commit().unwrap();
    drop(writer);
    let complete = std::fs::read(&path).unwrap();
    // Every possible truncation of the appended transaction must recover first.
    for end in prior_end as usize..complete.len() {
        let mut interrupted = complete[..end].to_vec();
        // Simulate a torn/complete new header pointer; the old slot is untouched.
        interrupted[32..32 + layout::SLOT as usize]
            .copy_from_slice(&original[32..32 + layout::SLOT as usize]);
        std::fs::write(&path, interrupted).unwrap();
        assert_eq!(
            Reader::open(&path).unwrap().read("scene", 10).unwrap(),
            b"first",
            "cut={end}"
        );
    }
    // Every torn header slot must also preserve the previous commit.
    let start = (32 + layout::SLOT) as usize;
    for cut in 0..layout::SLOT as usize {
        let mut torn = complete.clone();
        torn[start + cut..start + layout::SLOT as usize].fill(0);
        std::fs::write(&path, torn).unwrap();
        assert_eq!(
            Reader::open(&path).unwrap().read("scene", 10).unwrap(),
            b"first"
        );
    }
    std::fs::write(&path, &complete).unwrap();
    assert_eq!(
        Reader::open(&path).unwrap().read("scene", 10).unwrap(),
        b"second"
    );
}

#[test]
fn corruption_versions_read_limits_and_competing_writers_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("a.rovar");
    let mut writer = Writer::create(&path).unwrap();
    writer.put_bytes("scene", "json", b"data").unwrap();
    writer.commit().unwrap();
    assert!(Writer::open(&path).is_err());
    let reader = writer.snapshot();
    drop(writer);
    assert!(reader.read("scene", 3).is_err());
    let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.seek(SeekFrom::Start(reader.entry("scene").unwrap().offset))
        .unwrap();
    file.write_all(b"X").unwrap();
    assert!(reader.verify().is_err());
    drop(file);
    for (major, minor) in [(0u16, 9u16), (1, 1), (2, 0)] {
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[8..10].copy_from_slice(&major.to_le_bytes());
        bytes[10..12].copy_from_slice(&minor.to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();
        let error = Reader::open(&path).err().unwrap().to_string();
        assert_eq!(
            error,
            format!("Unsupported Rovar container version: {major}.{minor} (supported: 1.0)")
        );
        assert!(Writer::open(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn indexes_and_seeks_use_64_bit_offsets() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("large.rovar");
    let mut writer = Writer::create(&path).unwrap();
    writer.put_bytes("scene", "json", b"small").unwrap();
    writer.commit().unwrap();
    drop(writer);
    // Sparse gap exercises offsets above 4 GiB without allocating that memory.
    let mut writer = Writer::open(&path).unwrap();
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(5 * 1024 * 1024 * 1024).unwrap();
    writer.put_bytes("high", "media", b"tail").unwrap();
    writer.commit().unwrap();
    drop(writer);
    let reader = Reader::open(&path).unwrap();
    assert!(reader.entry("high").unwrap().offset > u32::MAX as u64);
    assert_eq!(reader.read("high", 4).unwrap(), b"tail");
    reader.verify().unwrap();
}

#[test]
fn invalid_index_ranges_and_duplicate_keys_are_rejected_before_block_allocation() {
    use crate::layout::{Block, Commit, HEADER, SLOT};
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("invalid.rovar");
    let mut writer = Writer::create(&path).unwrap();
    writer.put_bytes("original", "json", b"ok").unwrap();
    writer.commit().unwrap();
    drop(writer);
    let original = std::fs::read(&path).unwrap();
    let valid = Block {
        kind: "media".into(),
        offset: HEADER,
        length: 1,
        hash: [0; 32],
    };
    let cases = [
        vec![(
            "a",
            Block {
                length: u64::MAX,
                ..valid.clone()
            },
        )],
        vec![(
            "a",
            Block {
                offset: 0,
                ..valid.clone()
            },
        )],
        vec![("a", valid.clone()), ("a", valid.clone())],
        vec![("a", valid.clone()), ("b", valid)],
    ];
    for entries in cases {
        let index = serde_json::to_vec(&entries).unwrap();
        let index_offset = original.len() as u64;
        let commit = Commit {
            generation: 1,
            index_offset,
            index_length: index.len() as u64,
            end: index_offset + index.len() as u64 + SLOT,
            hash: Sha256::digest(&index).into(),
        };
        let mut bytes = original.clone();
        bytes[32..HEADER as usize].fill(0);
        bytes[32..32 + SLOT as usize].copy_from_slice(&commit.encode());
        bytes.extend(index);
        bytes.extend(commit.encode());
        std::fs::write(&path, bytes).unwrap();
        assert!(Reader::open(&path).is_err());
    }
}

#[test]
fn untagged_development_files_are_rejected_despite_identical_version_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("versioned.rovar");
    let mut writer = Writer::create(&path).unwrap();
    writer
        .put_bytes("scene", "json", b"preserve these bytes")
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let current = std::fs::read(&path).unwrap();
    assert_eq!(&current[12..20], b"RVFORMAT");
    assert_eq!(Reader::open(&path).unwrap().version().to_string(), "1.0");
    for marker in [[0; 8], *b"INVALID!"] {
        let mut unsupported = current.clone();
        // Both previous layouts encoded their version as these exact bytes.
        unsupported[8..12].copy_from_slice(&[1, 0, 0, 0]);
        unsupported[12..20].copy_from_slice(&marker);
        std::fs::write(&path, &unsupported).unwrap();
        let error = Reader::open(&path).err().unwrap().to_string();
        assert!(error.contains("Unsupported Rovar header layout"), "{error}");
        assert!(Writer::open(&path).is_err());
        let output = directory.path().join("export.rovar");
        assert!(compact(&path, &output).is_err());
        assert!(!output.exists());
        assert_eq!(std::fs::read(&path).unwrap(), unsupported);
    }
}
