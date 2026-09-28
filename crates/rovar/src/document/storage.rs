use super::*;
use rovar_format::{Reader, Writer};
use std::io::Read;

const METADATA_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    id: String,
    #[serde(default, skip_serializing, rename = "cover")]
    previous_cover: Option<String>,
    pages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    components: Vec<String>,
    media: BTreeMap<String, Media>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PageIndex {
    id: String,
    name: String,
    next_id: usize,
    boards: Vec<usize>,
    shapes: Vec<usize>,
    texts: Vec<usize>,
    assets: Vec<AssetUse>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SinglePageManifest {
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

enum Index {
    Pages(Manifest),
    Single(SinglePageManifest),
}
impl Index {
    fn id(&self) -> &str {
        match self {
            Self::Pages(info) => &info.id,
            Self::Single(info) => &info.id,
        }
    }
    fn media(&self) -> &BTreeMap<String, Media> {
        match self {
            Self::Pages(info) => &info.media,
            Self::Single(info) => &info.media,
        }
    }
}

fn manifest(reader: &Reader) -> Result<Index> {
    let value: serde_json::Value =
        serde_json::from_slice(&reader.read("document", METADATA_LIMIT)?)?;
    let schema = value
        .get("schema")
        .and_then(|v| v.as_u64())
        .context("Missing document schema")?;
    let index = match schema {
        1 => Index::Single(serde_json::from_value(value)?),
        2 => Index::Pages(serde_json::from_value(value)?),
        _ => anyhow::bail!("Unsupported Rovar document schema: {schema}"),
    };
    ensure!(
        uuid::Uuid::parse_str(index.id()).is_ok(),
        "Invalid document ID"
    );
    Ok(index)
}

pub(crate) fn read_id(reader: &Reader) -> Result<String> {
    Ok(manifest(reader)?.id().to_owned())
}

fn read_page(reader: &Reader, prefix: &str, info: PageIndex) -> Result<Page> {
    fn objects<T: serde::de::DeserializeOwned>(
        reader: &Reader,
        prefix: &str,
        kind: &str,
        ids: &[usize],
    ) -> Result<Vec<T>> {
        ids.iter()
            .map(|id| {
                Ok(serde_json::from_slice(
                    &reader.read(&format!("{prefix}{kind}/{id}"), METADATA_LIMIT)?,
                )?)
            })
            .collect()
    }
    let page = Page {
        id: info.id,
        name: info.name,
        next_id: info.next_id,
        boards: objects(reader, prefix, "board", &info.boards)?,
        shapes: objects(reader, prefix, "shape", &info.shapes)?,
        texts: objects(reader, prefix, "text", &info.texts)?,
        hierarchy: serde_json::from_slice(
            &reader.read(&format!("{prefix}hierarchy"), METADATA_LIMIT)?,
        )?,
        assets: info.assets,
    };
    ensure!(
        page.boards.iter().map(|x| x.id).eq(info.boards)
            && page.shapes.iter().map(|x| x.id).eq(info.shapes)
            && page.texts.iter().map(|x| x.id).eq(info.texts),
        "Object index differs from page"
    );
    page.validate()?;
    Ok(page)
}

pub(crate) fn read_document(reader: &Reader) -> Result<Document> {
    let document = match manifest(reader)? {
        Index::Pages(info) => {
            let mut pages = Vec::with_capacity(info.pages.len());
            for id in info.pages {
                let index: PageIndex =
                    serde_json::from_slice(&reader.read(&format!("page/{id}"), METADATA_LIMIT)?)?;
                ensure!(index.id == id, "Page index differs from document");
                pages.push(read_page(reader, &format!("page/{id}/"), index)?);
            }
            let mut components = crate::components::Definitions::new();
            for id in info.components {
                ensure!(uuid::Uuid::parse_str(&id).is_ok(), "Invalid component ID");
                let definition = serde_json::from_slice(
                    &reader.read(&format!("component/{id}"), METADATA_LIMIT)?,
                )?;
                ensure!(
                    components.insert(id, definition).is_none(),
                    "Duplicate component ID"
                );
            }
            Document {
                id: info.id,
                pages,
                components,
            }
        }
        Index::Single(info) => {
            ensure!(info.schema == 1, "Invalid single-page schema");
            let page = read_page(
                reader,
                "",
                PageIndex {
                    id: info.id.clone(),
                    name: "Page 1".into(),
                    next_id: info.next_id,
                    boards: info.boards,
                    shapes: info.shapes,
                    texts: info.texts,
                    assets: info.assets,
                },
            )?;
            Document {
                id: info.id,
                pages: vec![page],
                components: Default::default(),
            }
        }
    };
    document.validate()?;
    Ok(document)
}

pub(crate) fn save(
    path: &Path,
    json: &[u8],
    sources: &[AssetSource],
    expected: &[u8],
    text_system: &Arc<gpui::TextSystem>,
) -> Result<()> {
    let document = Document::decode(json)?;
    if !path.exists() {
        ensure!(
            expected.is_empty(),
            "The document was removed from its save location"
        );
        return save_as(path, json, sources, text_system);
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
    write_document(&mut writer, &document, sources, text_system)?;
    // Reclaim superseded blocks only when there is substantial garbage.
    if let Err(error) = writer.compact_if_needed(path, 4 * 1024 * 1024) {
        eprintln!("Document saved; container compaction deferred: {error:#}");
    }
    Ok(())
}

fn write_document(
    writer: &mut Writer,
    document: &Document,
    sources: &[AssetSource],
    text_system: &Arc<gpui::TextSystem>,
) -> Result<()> {
    let mut keys = BTreeSet::from(["document".to_owned()]);
    // Component metadata survives document edits.
    if writer.contains("component") {
        keys.insert("component".into());
    }
    let mut media = BTreeMap::new();
    let used: BTreeSet<_> = document.assets().map(|a| a.hash.as_str()).collect();
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
        schema: 2,
        id: document.id.clone(),
        previous_cover: None,
        pages: document.pages.iter().map(|page| page.id.clone()).collect(),
        components: document.components.keys().cloned().collect(),
        media,
    };
    put_json(writer, "document", "document", &info)?;
    for (id, definition) in &document.components {
        let key = format!("component/{id}");
        put_json(writer, &key, "json", definition)?;
        keys.insert(key);
    }
    for page in &document.pages {
        let key = format!("page/{}", page.id);
        let index = PageIndex {
            id: page.id.clone(),
            name: page.name.clone(),
            next_id: page.next_id,
            boards: page.boards.iter().map(|x| x.id).collect(),
            shapes: page.shapes.iter().map(|x| x.id).collect(),
            texts: page.texts.iter().map(|x| x.id).collect(),
            assets: page.assets.clone(),
        };
        put_json(writer, &key, "page", &index)?;
        keys.insert(key.clone());
        let hierarchy = format!("{key}/hierarchy");
        put_json(writer, &hierarchy, "json", &page.hierarchy)?;
        keys.insert(hierarchy);
        macro_rules! objects {
            ($items:expr, $kind:literal) => {
                for item in $items {
                    let key = format!("{}/{}/{}", key, $kind, item.id);
                    put_json(writer, &key, "json", item)?;
                    keys.insert(key);
                }
            };
        }
        objects!(&page.boards, "board");
        objects!(&page.shapes, "shape");
        objects!(&page.texts, "text");
    }
    let cover = document.first_page();
    if !(cover.boards.is_empty() && cover.shapes.is_empty() && cover.texts.is_empty()) {
        match preview::render(cover, sources, text_system) {
            Ok(png) => {
                writer.put_bytes("preview", "png", &png)?;
                keys.insert("preview".into());
            }
            Err(error) => eprintln!("Could not render document preview: {error:#}"),
        }
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

pub(crate) fn save_as(
    path: &Path,
    json: &[u8],
    sources: &[AssetSource],
    text_system: &Arc<gpui::TextSystem>,
) -> Result<()> {
    let document = Document::decode(json)?;
    let directory = path.parent().context("Invalid save location")?;
    let temporary = tempfile::Builder::new()
        .suffix(".rovar")
        .tempfile_in(directory)?
        .into_temp_path();
    let mut writer = Writer::create(&temporary)?;
    write_document(&mut writer, &document, sources, text_system)?;
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
    for asset in document.assets() {
        if assets.contains_key(&asset.hash) {
            continue;
        }
        let meta = info
            .media()
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
    Ok(Loaded {
        json,
        assets,
        needs_upgrade: match info {
            Index::Single(_) => true,
            Index::Pages(info) => info.previous_cover.is_some(),
        },
    })
}
