use crate::{
    artboard::{Artboard, Rect},
    layer::{Hierarchy, LayerState},
    shape::Shape,
    text::styles::StyledText,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

mod preview;
#[cfg(test)]
mod tests;

mod storage;
pub(crate) use storage::{cache_preview, load, read_id, save, save_as};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Text {
    pub id: usize,
    pub board: Option<usize>,
    pub rect: Rect,
    pub layer: LayerState,
    pub content: String,
    pub styles: StyledText,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AssetUse {
    pub object: usize,
    pub fill: bool,
    pub hash: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Page {
    pub id: String,
    pub name: String,
    pub boards: Vec<Artboard>,
    pub shapes: Vec<Shape>,
    pub texts: Vec<Text>,
    pub hierarchy: Hierarchy,
    pub next_id: usize,
    pub assets: Vec<AssetUse>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Document {
    pub id: String,
    pub pages: Vec<Page>,
}

impl Document {
    pub fn single(page: Page) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            pages: vec![page],
        }
    }
    pub fn decode(json: &[u8]) -> Result<Self> {
        let document: Self = serde_json::from_slice(json)?;
        document.validate()?;
        Ok(document)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&self.id).is_ok(),
            "Invalid document ID"
        );
        ensure!(
            !self.pages.is_empty(),
            "A document must contain at least one page"
        );
        let mut ids = BTreeSet::new();
        for page in &self.pages {
            ensure!(ids.insert(&page.id), "Duplicate page ID");
            page.validate()?;
        }
        Ok(())
    }
    pub fn first_page(&self) -> &Page {
        self.pages.first().expect("validated nonempty document")
    }
    pub fn assets(&self) -> impl Iterator<Item = &AssetUse> {
        self.pages.iter().flat_map(|page| &page.assets)
    }
}

#[derive(Clone)]
pub(crate) struct AssetSource {
    pub hash: String,
    pub name: String,
    pub path: Arc<crate::media::Source>,
    pub size: [u32; 2],
}

pub(crate) struct Loaded {
    pub json: Vec<u8>,
    pub needs_upgrade: bool,
    pub assets: BTreeMap<String, Arc<crate::media::MediaAsset>>,
}

impl Loaded {
    pub fn into_document(self) -> Result<Document> {
        let mut document = Document::decode(&self.json)?;
        for page in &mut document.pages {
            for use_ in &page.assets {
                let asset = self
                    .assets
                    .get(&use_.hash)
                    .ok_or_else(|| anyhow::anyhow!("Missing asset"))?
                    .clone();
                if use_.fill {
                    if let Some(board) = page.boards.iter_mut().find(|b| b.id == use_.object) {
                        board.image_fill.asset = Some(asset);
                    } else if let Some(shape) = page.shapes.iter_mut().find(|s| s.id == use_.object)
                    {
                        shape.image_fill.asset = Some(asset);
                    }
                } else if let Some(shape) = page.shapes.iter_mut().find(|s| s.id == use_.object) {
                    shape.media = Some(asset);
                }
            }
        }
        Ok(document)
    }
}

pub(crate) fn import(
    source: &Path,
    directory: &Path,
    text_system: &Arc<gpui::TextSystem>,
) -> Result<(Loaded, PathBuf)> {
    let mut loaded = load(source)?;
    let mut document = Document::decode(&loaded.json)?;
    document.id = uuid::Uuid::new_v4().to_string();
    loaded.json = serde_json::to_vec(&document)?;
    let assets: Vec<_> = loaded
        .assets
        .values()
        .map(|asset| AssetSource {
            hash: asset.hash.clone(),
            name: asset.name(),
            path: asset.source.clone(),
            size: [asset.width, asset.height],
        })
        .collect();
    std::fs::create_dir_all(directory)?;
    let path = directory.join(format!("{}.rovar", document.id));
    save_as(&path, &loaded.json, &assets, text_system)?;
    Ok((load(&path)?, path))
}

impl Page {
    pub fn empty(name: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            boards: Vec::new(),
            shapes: Vec::new(),
            texts: Vec::new(),
            hierarchy: Default::default(),
            next_id: 1,
            assets: Vec::new(),
        }
    }
    pub fn decode(json: &[u8]) -> Result<Self> {
        let document: Self = serde_json::from_slice(json)?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&self.id).is_ok(),
            "Invalid document ID"
        );
        ensure!(
            !self.name.trim().is_empty() && self.name.chars().count() <= 200,
            "Invalid page name"
        );
        let mut ids = BTreeSet::new();
        for id in self
            .boards
            .iter()
            .map(|b| b.id)
            .chain(self.shapes.iter().map(|s| s.id))
            .chain(self.texts.iter().map(|t| t.id))
            .chain(self.hierarchy.groups.keys().copied())
        {
            ensure!(id > 0 && ids.insert(id), "Duplicate or invalid object ID");
        }
        ensure!(
            ids.last().is_none_or(|id| self.next_id > *id),
            "Invalid next object ID"
        );
        let boards: BTreeSet<_> = self.boards.iter().map(|b| b.id).collect();
        for board in self
            .shapes
            .iter()
            .map(|s| s.board)
            .chain(self.texts.iter().map(|t| t.board))
            .chain(self.hierarchy.groups.values().map(|g| g.board))
            .flatten()
        {
            ensure!(boards.contains(&board), "Missing frame");
        }
        for (child, parent) in &self.hierarchy.parents {
            ensure!(
                ids.contains(child) && self.hierarchy.groups.contains_key(parent),
                "Invalid group membership"
            );
            let mut visited = BTreeSet::from([*child]);
            let mut next = Some(parent);
            while let Some(parent) = next {
                ensure!(visited.insert(*parent), "Cyclic group membership");
                next = self.hierarchy.parents.get(parent);
            }
        }
        let mut parents: BTreeMap<_, _> = self
            .shapes
            .iter()
            .map(|s| (s.id, s.board))
            .chain(self.texts.iter().map(|t| (t.id, t.board)))
            .chain(self.hierarchy.groups.iter().map(|(id, g)| (*id, g.board)))
            .collect();
        parents.extend(
            self.hierarchy
                .parents
                .iter()
                .map(|(id, parent)| (*id, Some(*parent))),
        );
        for id in &ids {
            let mut seen = BTreeSet::new();
            let mut next = Some(*id);
            while let Some(id) = next {
                ensure!(seen.insert(id), "Cyclic document hierarchy");
                next = parents.get(&id).copied().flatten();
            }
        }
        for rect in self
            .boards
            .iter()
            .map(|b| b.rect)
            .chain(self.shapes.iter().map(|s| s.rect))
            .chain(self.texts.iter().map(|t| t.rect))
        {
            ensure!(
                [rect.x, rect.y, rect.width, rect.height]
                    .iter()
                    .all(|v| v.is_finite())
                    && rect.width >= 0.
                    && rect.height >= 0.,
                "Invalid object geometry"
            );
        }
        for text in &self.texts {
            let mut end = 0;
            for run in &text.styles.runs {
                ensure!(
                    run.range.start == end
                        && run.range.end >= end
                        && text.content.is_char_boundary(run.range.start)
                        && text.content.is_char_boundary(run.range.end),
                    "Invalid text style range"
                );
                end = run.range.end;
            }
            ensure!(end == text.content.len(), "Incomplete text styles");
        }
        for gradient in self
            .boards
            .iter()
            .map(|b| &b.gradient)
            .chain(
                self.shapes
                    .iter()
                    .flat_map(|s| [&s.gradient, &s.stroke.gradient]),
            )
            .chain(self.texts.iter().flat_map(|t| {
                std::iter::once(&t.styles.default.gradient)
                    .chain(t.styles.runs.iter().map(|r| &r.style.gradient))
            }))
        {
            gradient.validate()?;
        }
        let mut slots = BTreeSet::new();
        for asset in &self.assets {
            let shape = self.shapes.iter().find(|s| s.id == asset.object);
            ensure!(
                slots.insert((asset.object, asset.fill))
                    && (if asset.fill {
                        boards.contains(&asset.object) || shape.is_some()
                    } else {
                        shape.is_some_and(|s| s.kind.is_media())
                    })
                    && asset.hash.len() == 64
                    && asset.hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid asset reference"
            );
        }
        for shape in &self.shapes {
            ensure!(
                !shape.kind.is_media() || slots.contains(&(shape.id, false)),
                "Missing media asset"
            );
            ensure!(
                [
                    shape.layer.rotation,
                    shape.radius,
                    shape.inner_radius,
                    shape.stroke.width,
                    shape.image_fill.opacity
                ]
                .iter()
                .all(|v| v.is_finite()),
                "Invalid shape properties"
            );
            ensure!(
                shape
                    .points
                    .0
                    .iter()
                    .all(|p| p.x.is_finite() && p.y.is_finite())
                    && shape
                        .nodes
                        .0
                        .iter()
                        .all(|n| [n.anchor, n.incoming, n.outgoing]
                            .iter()
                            .all(|p| p.x.is_finite() && p.y.is_finite())),
                "Invalid path coordinates"
            );
        }
        Ok(())
    }
}

pub(crate) mod rgba {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(color: &gpui::Rgba, s: S) -> Result<S::Ok, S::Error> {
        [color.r, color.g, color.b, color.a].serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gpui::Rgba, D::Error> {
        let [r, g, b, a] = <[f32; 4]>::deserialize(d)?;
        if [r, g, b, a]
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err(serde::de::Error::custom("Invalid color"));
        }
        Ok(gpui::Rgba { r, g, b, a })
    }
}

pub(crate) mod gradient_kind {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(kind: &gpui::GradientKind, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match kind {
            gpui::GradientKind::Linear => "linear",
            gpui::GradientKind::Radial => "radial",
            gpui::GradientKind::Angular => "angular",
            gpui::GradientKind::Diamond => "diamond",
        })
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gpui::GradientKind, D::Error> {
        match String::deserialize(d)?.as_str() {
            "linear" => Ok(gpui::GradientKind::Linear),
            "radial" => Ok(gpui::GradientKind::Radial),
            "angular" => Ok(gpui::GradientKind::Angular),
            "diamond" => Ok(gpui::GradientKind::Diamond),
            _ => Err(serde::de::Error::custom("Invalid gradient kind")),
        }
    }
}
