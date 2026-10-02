//! Versioned metadata deltas. Media blocks are transferred by content hash.
use crate::{Reader, Writer};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const MAX_CHANGES: usize = 65_536;
const MAX_PATH: usize = 64;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    blocks: BTreeMap<String, Block>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Block {
    kind: String,
    data: Data,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Data {
    Json(Value),
    Bytes(String),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delta {
    version: u32,
    pub base: [u8; 32],
    result: [u8; 32],
    changes: BTreeMap<String, Change>,
}

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "op",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Change {
    Put(Block),
    Remove,
    Patch(Vec<Edit>),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
    Set { path: Vec<String>, value: Value },
    Remove { path: Vec<String> },
}

impl Snapshot {
    pub fn read(reader: &Reader, maximum: usize) -> Result<Self> {
        let mut total = 0u64;
        let mut blocks = BTreeMap::new();
        for (key, block) in reader.entries() {
            if block.kind == "media" || key.starts_with("media/") {
                continue;
            }
            total = total
                .checked_add(block.length)
                .context("Metadata size overflow")?;
            ensure!(
                total <= maximum as u64 && blocks.len() < MAX_CHANGES,
                "Metadata exceeds delta limits"
            );
            let bytes = reader.read(key, maximum as u64)?;
            let data = if block.kind == "json" {
                let mut value: Value = serde_json::from_slice(&bytes)?;
                canonicalize(&mut value);
                Data::Json(value)
            } else {
                Data::Bytes(STANDARD.encode(bytes))
            };
            blocks.insert(
                key.into(),
                Block {
                    kind: block.kind.clone(),
                    data,
                },
            );
        }
        Ok(Self { blocks })
    }

    pub fn from_bytes(bytes: &[u8], maximum: usize) -> Result<Self> {
        ensure!(bytes.len() <= maximum, "Container exceeds delta limits");
        let file = rovar_storage::tempfile::NamedTempFile::new()?;
        rovar_storage::fs::write(file.path(), bytes)?;
        Self::read(&Reader::open(file.path())?, maximum)
    }

    /// Canonical JSON makes this independent of container layout, compaction,
    /// and serde_json's optional preservation of object insertion order.
    pub fn hash(&self) -> Result<[u8; 32]> {
        let mut hash = Sha256::new();
        hash.update(b"rovar/metadata-snapshot/v1");
        for (key, block) in &self.blocks {
            let mut block = block.clone();
            if let Data::Json(value) = &mut block.data {
                canonicalize(value);
            }
            let bytes = serde_json::to_vec(&block)?;
            hash.update((key.len() as u64).to_le_bytes());
            hash.update(key.as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        Ok(hash.finalize().into())
    }

    pub fn difference(&self, next: &Self) -> Result<Delta> {
        let mut changes = BTreeMap::new();
        for key in self
            .blocks
            .keys()
            .chain(next.blocks.keys())
            .collect::<BTreeSet<_>>()
        {
            let before = self.blocks.get(key);
            let after = next.blocks.get(key);
            if before == after {
                continue;
            }
            let change = match (before, after) {
                (_, None) => Change::Remove,
                (Some(before), Some(after)) if before.kind == after.kind => {
                    let replacement = Change::Put(after.clone());
                    if let (Data::Json(before), Data::Json(after)) = (&before.data, &after.data) {
                        let mut edits = Vec::new();
                        diff(before, after, &mut Vec::new(), &mut edits);
                        let patch = Change::Patch(edits);
                        if matches!(&patch, Change::Patch(edits) if edits.len() <= MAX_CHANGES)
                            && serde_json::to_vec(&patch)?.len()
                                < serde_json::to_vec(&replacement)?.len()
                        {
                            patch
                        } else {
                            replacement
                        }
                    } else {
                        replacement
                    }
                }
                (_, Some(after)) => Change::Put(after.clone()),
            };
            changes.insert(key.clone(), change);
        }
        ensure!(changes.len() <= MAX_CHANGES, "Too many changed blocks");
        let edits: usize = changes
            .values()
            .map(|change| match change {
                Change::Patch(edits) => edits.len(),
                _ => 0,
            })
            .sum();
        ensure!(edits <= MAX_CHANGES, "Too many property edits");
        Ok(Delta {
            version: 1,
            base: self.hash()?,
            result: next.hash()?,
            changes,
        })
    }

    pub fn apply(&self, delta: &Delta, maximum: usize) -> Result<Self> {
        ensure!(
            delta.version == 1 && delta.changes.len() <= MAX_CHANGES,
            "Unsupported or oversized delta"
        );
        ensure!(self.hash()? == delta.base, "Delta baseline mismatch");
        let mut next = self.clone();
        let mut operations = 0usize;
        for (key, change) in &delta.changes {
            ensure!(!key.starts_with("media/"), "Media cannot be patched");
            match change {
                Change::Remove => {
                    ensure!(next.blocks.remove(key).is_some(), "Missing removed block");
                }
                Change::Put(block) => {
                    next.blocks.insert(key.clone(), block.clone());
                }
                Change::Patch(edits) => {
                    operations = operations
                        .checked_add(edits.len())
                        .context("Delta size overflow")?;
                    ensure!(operations <= MAX_CHANGES, "Too many property edits");
                    let block = next.blocks.get_mut(key).context("Missing patched block")?;
                    let Data::Json(value) = &mut block.data else {
                        anyhow::bail!("Cannot patch a binary block");
                    };
                    for edit in edits {
                        apply_edit(value, edit)?;
                    }
                    canonicalize(value);
                }
            }
        }
        next.validate(maximum)?;
        ensure!(
            next.hash()? == delta.result,
            "Delta result checksum mismatch"
        );
        Ok(next)
    }

    fn validate(&self, maximum: usize) -> Result<()> {
        ensure!(self.blocks.len() <= MAX_CHANGES, "Too many metadata blocks");
        let mut total = 0usize;
        for (key, block) in &self.blocks {
            ensure!(
                !key.is_empty()
                    && key.len() <= 1024
                    && block.kind != "media"
                    && !key.starts_with("media/"),
                "Invalid metadata block"
            );
            ensure!(
                matches!((&*block.kind, &block.data), ("json", Data::Json(_)))
                    || (block.kind != "json" && matches!(block.data, Data::Bytes(_))),
                "Invalid block encoding"
            );
            total = total
                .checked_add(block.bytes()?.len())
                .context("Metadata size overflow")?;
            ensure!(total <= maximum, "Metadata exceeds delta limits");
        }
        Ok(())
    }

    pub fn to_bytes(&self, maximum: usize) -> Result<Vec<u8>> {
        self.validate(maximum)?;
        let file = rovar_storage::tempfile::NamedTempFile::new()?;
        let mut writer = Writer::create(file.path())?;
        for (key, block) in &self.blocks {
            writer.put_bytes(key, &block.kind, &block.bytes()?)?;
        }
        writer.commit()?;
        drop(writer);
        let bytes = rovar_storage::fs::read(file.path())?;
        ensure!(bytes.len() <= maximum, "Container exceeds delta limits");
        Ok(bytes)
    }
}

impl Block {
    fn bytes(&self) -> Result<Vec<u8>> {
        Ok(match &self.data {
            Data::Json(value) => serde_json::to_vec(value)?,
            Data::Bytes(value) => STANDARD.decode(value)?,
        })
    }
}

fn canonicalize(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for field in fields.values_mut() {
                canonicalize(field);
            }
            fields.sort_keys();
        }
        Value::Array(items) => {
            for item in items {
                canonicalize(item);
            }
        }
        _ => {}
    }
}

fn diff(before: &Value, after: &Value, path: &mut Vec<String>, edits: &mut Vec<Edit>) {
    if edits.len() > MAX_CHANGES {
        return;
    }
    if before == after {
        return;
    }
    if path.len() < MAX_PATH
        && let (Value::Object(before), Value::Object(after)) = (before, after)
    {
        for key in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
            path.push(key.clone());
            match (before.get(key), after.get(key)) {
                (Some(before), Some(after)) => diff(before, after, path, edits),
                (_, Some(after)) => edits.push(Edit::Set {
                    path: path.clone(),
                    value: after.clone(),
                }),
                (_, None) => edits.push(Edit::Remove { path: path.clone() }),
            }
            path.pop();
        }
    } else {
        edits.push(Edit::Set {
            path: path.clone(),
            value: after.clone(),
        });
    }
}

fn apply_edit(root: &mut Value, edit: &Edit) -> Result<()> {
    let path = match edit {
        Edit::Set { path, .. } | Edit::Remove { path } => path,
    };
    ensure!(path.len() <= MAX_PATH, "Property path is too deep");
    let Some((key, parents)) = path.split_last() else {
        let Edit::Set { value, .. } = edit else {
            anyhow::bail!("Cannot remove the JSON root");
        };
        *root = value.clone();
        return Ok(());
    };
    let mut target = root;
    for parent in parents {
        target = target
            .as_object_mut()
            .context("Invalid property parent")?
            .get_mut(parent)
            .context("Missing property parent")?;
    }
    let target = target.as_object_mut().context("Invalid property parent")?;
    match edit {
        Edit::Set { value, .. } => {
            target.insert(key.clone(), value.clone());
        }
        Edit::Remove { .. } => {
            ensure!(target.remove(key).is_some(), "Missing removed property");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
