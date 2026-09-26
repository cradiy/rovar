use super::*;
use rovar_format::{Reader, Writer};
use std::io::Read;

const METADATA_LIMIT: u64 = 64 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    id: String,
    next_id: usize,
    boards: Vec<usize>,
    shapes: Vec<usize>,
    texts: Vec<usize>,
    assets: Vec<AssetUse>,
    media: BTreeMap<String, Media>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Media {
    name: String,
    size: [u32; 2],
}

fn manifest(reader: &Reader) -> Result<Manifest> {
    let info: Manifest = serde_json::from_slice(&reader.read("document", METADATA_LIMIT)?)?;
    ensure!(
        info.schema == 1,
        "Unsupported Rovar document schema: {}",
        info.schema
    );
    ensure!(
        uuid::Uuid::parse_str(&info.id).is_ok(),
        "Invalid document ID"
    );
    Ok(info)
}

pub(crate) fn read_id(reader: &Reader) -> Result<String> {
    Ok(manifest(reader)?.id)
}

pub(crate) fn read_document(reader: &Reader) -> Result<Document> {
    let info = manifest(reader)?;
    fn objects<T: serde::de::DeserializeOwned>(
        reader: &Reader,
        kind: &str,
        ids: &[usize],
    ) -> Result<Vec<T>> {
        ids.iter()
            .map(|id| {
                Ok(serde_json::from_slice(
                    &reader.read(&format!("{kind}/{id}"), METADATA_LIMIT)?,
                )?)
            })
            .collect()
    }
    let document = Document {
        id: info.id,
        next_id: info.next_id,
        boards: objects(reader, "board", &info.boards)?,
        shapes: objects(reader, "shape", &info.shapes)?,
        texts: objects(reader, "text", &info.texts)?,
        hierarchy: serde_json::from_slice(&reader.read("hierarchy", METADATA_LIMIT)?)?,
        assets: info.assets,
    };
    ensure!(
        document.boards.iter().map(|x| x.id).eq(info.boards)
            && document.shapes.iter().map(|x| x.id).eq(info.shapes)
            && document.texts.iter().map(|x| x.id).eq(info.texts),
        "Object index differs from scene"
    );
    document.validate()?;
    Ok(document)
}

pub(crate) fn save(
    path: &Path,
    json: &[u8],
    sources: &[AssetSource],
    expected: &[u8],
) -> Result<()> {
    let document = Document::decode(json)?;
    if !path.exists() {
        ensure!(
            expected.is_empty(),
            "The document was removed from its save location"
        );
        return save_as(path, json, sources);
    }
    let mut writer = Writer::open(path)?;
    let previous = read_document(&writer.snapshot())?;
    ensure!(
        serde_json::to_vec(&previous)? == expected,
        "The file changed outside this tab. Export your changes separately"
    );
    ensure!(
        previous.id == document.id,
        "A different document now exists at this location"
    );
    write_document(&mut writer, &document, json, sources)?;
    // Reclaim superseded blocks only when there is substantial garbage.
    if let Err(error) = writer.compact_if_needed(path, 4 * 1024 * 1024) {
        eprintln!("Document saved; container compaction deferred: {error:#}");
    }
    Ok(())
}

fn write_document(
    writer: &mut Writer,
    document: &Document,
    json: &[u8],
    sources: &[AssetSource],
) -> Result<()> {
    let mut keys = BTreeSet::from(["document".to_owned(), "hierarchy".to_owned()]);
    // Component metadata survives document edits.
    if writer.contains("component") {
        keys.insert("component".into());
    }
    let mut media = BTreeMap::new();
    let used: BTreeSet<_> = document.assets.iter().map(|a| a.hash.as_str()).collect();
    for source in sources {
        let digest: [u8; 32] = decode_hash(&source.hash)?;
        ensure!(
            source.size.iter().all(|v| *v > 0),
            "Invalid media dimensions"
        );
        // Validate supplied sources even if a caller accidentally passes an unused one.
        if !used.contains(source.hash.as_str()) {
            let mut input = source.path.open()?;
            let mut hash = Sha256::new();
            let mut buffer = vec![0; 1024 * 1024];
            loop {
                match input.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => hash.update(&buffer[..count]),
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            ensure!(hash.finalize()[..] == digest, "Asset changed during save");
            continue;
        }
        let key = format!("media/{}", source.hash);
        keys.insert(key.clone());
        if !writer.contains(&key) {
            writer.put(&key, "media", source.path.open()?, Some(digest))?;
        }
        media.insert(
            source.hash.clone(),
            Media {
                name: source.name.clone(),
                size: source.size,
            },
        );
    }
    ensure!(
        used.iter().all(|hash| media.contains_key(*hash)),
        "Missing media source"
    );
    let info = Manifest {
        schema: 1,
        id: document.id.clone(),
        next_id: document.next_id,
        boards: document.boards.iter().map(|x| x.id).collect(),
        shapes: document.shapes.iter().map(|x| x.id).collect(),
        texts: document.texts.iter().map(|x| x.id).collect(),
        assets: document.assets.clone(),
        media,
    };
    put_json(writer, "document", "document", &info)?;
    put_json(writer, "hierarchy", "json", &document.hierarchy)?;
    macro_rules! objects {
        ($items:expr, $kind:literal) => {
            for item in $items {
                let key = format!("{}/{}", $kind, item.id);
                put_json(writer, &key, "json", item)?;
                keys.insert(key);
            }
        };
    }
    objects!(&document.boards, "board");
    objects!(&document.shapes, "shape");
    objects!(&document.texts, "text");
    if !(document.boards.is_empty() && document.shapes.is_empty() && document.texts.is_empty())
        && let Ok(png) = preview::render(json, sources)
    {
        writer.put_bytes("preview", "png", &png)?;
        keys.insert("preview".into());
    }
    writer.retain(&keys);
    writer.commit()?;
    Ok(())
}

fn put_json(writer: &mut Writer, key: &str, kind: &str, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        bytes.len() as u64 <= METADATA_LIMIT,
        "Metadata block is too large: {key}"
    );
    writer.put_bytes(key, kind, &bytes)
}

fn decode_hash(value: &str) -> Result<[u8; 32]> {
    ensure!(value.len() == 64 && value.is_ascii(), "Invalid media hash");
    let mut digest = [0; 32];
    for (i, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)?;
    }
    Ok(digest)
}

pub(crate) fn cache_preview(path: &Path, directory: &Path) -> Result<Option<String>> {
    let reader = Reader::open(path)?;
    if reader.entry("preview").is_none() {
        return Ok(None);
    }
    let png = reader.read("preview", METADATA_LIMIT)?;
    let name = format!("{}.png", hex::encode(Sha256::digest(&png)));
    std::fs::create_dir_all(directory)?;
    let path = directory.join(&name);
    if !path.exists() {
        std::fs::write(path, png)?;
    }
    Ok(Some(name))
}

pub(crate) fn save_as(path: &Path, json: &[u8], sources: &[AssetSource]) -> Result<()> {
    let document = Document::decode(json)?;
    let directory = path.parent().context("Invalid save location")?;
    let temporary = tempfile::Builder::new()
        .suffix(".rovar")
        .tempfile_in(directory)?
        .into_temp_path();
    let mut writer = Writer::create(&temporary)?;
    write_document(&mut writer, &document, json, sources)?;
    drop(writer);
    std::fs::rename(&temporary, path)?;
    rovar_format::sync_parent(path)?;
    Ok(())
}

pub(crate) fn load(path: &Path) -> Result<Loaded> {
    let reader = Reader::open(path)?;
    let info = manifest(&reader)?;
    let document = read_document(&reader)?;
    let json = serde_json::to_vec(&document)?;
    let mut assets = BTreeMap::new();
    for asset in &document.assets {
        if assets.contains_key(&asset.hash) {
            continue;
        }
        let meta = info
            .media
            .get(&asset.hash)
            .context("Missing media metadata")?;
        ensure!(meta.size.iter().all(|v| *v > 0), "Invalid media dimensions");
        let block = reader.block(&format!("media/{}", asset.hash))?;
        ensure!(
            block.info.kind == "media" && block.info.hash == decode_hash(&asset.hash)?,
            "Invalid media block"
        );
        let source = crate::media::Source::block(block, &meta.name);
        assets.insert(
            asset.hash.clone(),
            crate::media::MediaAsset::deferred(
                meta.name.clone().into(),
                source,
                asset.hash.clone(),
                meta.size,
            ),
        );
    }
    Ok(Loaded { json, assets })
}
