use crate::{VERSION, Version};
use anyhow::{Result, ensure};
use rovar_storage::fs::File;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom},
};

pub const MAGIC: &[u8; 8] = b"ROVAR\r\n\x1a";
pub const FORMAT_TAG: &[u8; 8] = b"RVFORMAT";
pub const SLOT: u64 = 104;
pub const HEADER: u64 = 32 + SLOT * 2;
pub const MAX_INDEX: u64 = 64 * 1024 * 1024;
pub const COMMIT: &[u8; 8] = b"RVCOMMIT";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub kind: String,
    pub offset: u64,
    pub length: u64,
    pub hash: [u8; 32],
}

#[derive(Clone, Default)]
pub struct Commit {
    pub generation: u64,
    pub index_offset: u64,
    pub index_length: u64,
    pub end: u64,
    pub hash: [u8; 32],
}
impl Commit {
    pub fn encode(&self) -> [u8; SLOT as usize] {
        let mut bytes = [0; SLOT as usize];
        bytes[..8].copy_from_slice(COMMIT);
        for (i, value) in [
            self.generation,
            self.index_offset,
            self.index_length,
            self.end,
        ]
        .iter()
        .enumerate()
        {
            bytes[8 + i * 8..16 + i * 8].copy_from_slice(&value.to_le_bytes());
        }
        bytes[40..72].copy_from_slice(&self.hash);
        let digest = Sha256::digest(&bytes[..72]);
        bytes[72..].copy_from_slice(&digest);
        bytes
    }
    fn decode(bytes: &[u8; SLOT as usize]) -> Result<Self> {
        ensure!(
            &bytes[..8] == COMMIT && Sha256::digest(&bytes[..72])[..] == bytes[72..],
            "Invalid commit checksum"
        );
        let value = |i: usize| u64::from_le_bytes(bytes[8 + i * 8..16 + i * 8].try_into().unwrap());
        Ok(Self {
            generation: value(0),
            index_offset: value(1),
            index_length: value(2),
            end: value(3),
            hash: bytes[40..72].try_into().unwrap(),
        })
    }
}

pub fn read_commit(file: &mut File) -> Result<(Commit, BTreeMap<String, Block>)> {
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0; HEADER as usize];
    file.read_exact(&mut header)?;
    ensure!(&header[..8] == MAGIC, "This is not a Rovar container");
    ensure!(
        &header[12..20] == FORMAT_TAG,
        "Unsupported Rovar header layout: missing or invalid format identifier"
    );
    let version = Version::from_le_bytes(header[8..12].try_into().unwrap());
    ensure!(
        version == VERSION,
        "Unsupported Rovar container version: {version} (supported: {VERSION})"
    );
    ensure!(
        header[20..32].iter().all(|b| *b == 0),
        "Unsupported Rovar header flags"
    );
    let mut candidates = Vec::new();
    for slot in header[32..].as_chunks::<{ SLOT as usize }>().0 {
        if let Ok(commit) = Commit::decode(slot) {
            candidates.push(commit);
        }
    }
    candidates.sort_by_key(|c| std::cmp::Reverse(c.generation));
    for commit in candidates {
        if let Ok(index) = read_index(file, &commit) {
            return Ok((commit, index));
        }
    }
    anyhow::bail!("No complete Rovar commit")
}

fn read_index(file: &mut File, commit: &Commit) -> Result<BTreeMap<String, Block>> {
    ensure!(
        commit.generation > 0 && commit.index_offset >= HEADER && commit.index_length <= MAX_INDEX,
        "Invalid index bounds"
    );
    let footer = commit
        .index_offset
        .checked_add(commit.index_length)
        .ok_or_else(|| anyhow::anyhow!("Index overflow"))?;
    ensure!(
        footer.checked_add(SLOT) == Some(commit.end) && commit.end <= file.metadata()?.len(),
        "Truncated commit"
    );
    file.seek(SeekFrom::Start(footer))?;
    let mut bytes = [0; SLOT as usize];
    file.read_exact(&mut bytes)?;
    ensure!(bytes == commit.encode(), "Unpublished commit");
    file.seek(SeekFrom::Start(commit.index_offset))?;
    let mut json = vec![0; commit.index_length as usize];
    file.read_exact(&mut json)?;
    ensure!(Sha256::digest(&json)[..] == commit.hash, "Damaged index");
    let entries: Vec<(String, Block)> = serde_json::from_slice(&json)?;
    let mut index = BTreeMap::new();
    let mut ranges = Vec::new();
    for (key, block) in entries {
        validate_key(&key, &block.kind)?;
        ensure!(
            block.offset >= HEADER
                && block
                    .offset
                    .checked_add(block.length)
                    .is_some_and(|end| end <= commit.index_offset),
            "Invalid block bounds"
        );
        if block.length > 0 {
            ranges.push((block.offset, block.offset + block.length));
        }
        ensure!(index.insert(key, block).is_none(), "Duplicate block key");
    }
    ranges.sort_unstable();
    ensure!(
        ranges.windows(2).all(|r| r[0].1 <= r[1].0),
        "Overlapping blocks"
    );
    Ok(index)
}

pub fn validate_key(key: &str, kind: &str) -> Result<()> {
    ensure!(
        !key.is_empty() && key.len() <= 1024 && !kind.is_empty() && kind.len() <= 64,
        "Invalid block key or kind"
    );
    Ok(())
}
