# Rovar file format 1.0

English | [简体中文](FORMAT.zh-CN.md)

`rovar-format` provides a block container with seekable reads, incremental writes and compaction. Payloads are uncompressed. Binary integers are unsigned little-endian; offsets are absolute and lengths are in bytes.

## Header

The header occupies 240 bytes. Readers require the identifier, zero reserved bytes, version **1.0** and a valid commit. The format version is independent of the application version.

| Offset | Length | Value |
| --- | --- | --- |
| 0 | 8 | `ROVAR\r\n\x1a` |
| 8 | 2 | Major version: `1` |
| 10 | 2 | Minor version: `0` |
| 12 | 8 | ASCII `RVFORMAT` |
| 20 | 12 | Reserved: zero |
| 32 | 104 | Commit slot A |
| 136 | 104 | Commit slot B |

## Blocks and index

Immutable payload blocks follow the header. An index maps keys to blocks using a UTF-8 JSON array of `[key, descriptor]` pairs, written in key order. Replacing or deleting a key updates the index; compaction removes unused bytes.

| Descriptor field | Type | Meaning |
| --- | --- | --- |
| `kind` | String | Application-defined type, 1–64 UTF-8 bytes |
| `offset` | u64 | Absolute block position |
| `length` | u64 | Stored block length |
| `hash` | Array of 32 bytes | SHA-256 of block contents |

Keys occupy 1–1024 UTF-8 bytes. The index is limited to 64 MiB. Duplicate keys, overlapping nonempty blocks, out-of-bounds blocks and arithmetic overflow are rejected.

Opening reads the index without loading media payloads. `Reader::read` verifies a block; `BlockHandle::reader` exposes a bounded `Read + Seek` stream. `copy_verified` and `Reader::verify` stream full blocks to verify checksums. Partial reads do not verify a whole block.

## Commit record

Each 104-byte record appears after its index as a footer and in one header slot.

| Offset | Length | Value |
| --- | --- | --- |
| 0 | 8 | ASCII `RVCOMMIT` |
| 8 | 8 | Generation, starting at 1 |
| 16 | 8 | Index offset |
| 24 | 8 | Index length |
| 32 | 8 | Committed file length, including footer |
| 40 | 32 | SHA-256 of index bytes |
| 72 | 32 | SHA-256 of the record's first 72 bytes |

Saving holds an exclusive writer lock and follows this order:

1. Recover the latest valid commit and truncate any uncommitted tail.
2. Append changed blocks and the index, then call `sync_all`.
3. Append the footer and call `sync_all`.
4. Write header slot `(generation - 1) % 2` and call `sync_all`.

Readers inspect slots from newest to oldest and select the first with a valid index and matching footer. A footer alone does not publish a commit. Payload checksums are checked when read or verified.

## Compaction

Compaction locks the source, streams and verifies live blocks into a temporary file in the same directory, commits it, then replaces the source by renaming. On Unix, it also syncs the parent directory. Existing readers retain their open snapshot. The compacted file starts at generation 1.

## Inspect a file

```sh
cargo run --locked -p rovar-format --bin rovar-inspect -- design.rovar
cargo run --locked -p rovar-format --bin rovar-inspect -- design.rovar --verify
```

The first command lists the index. The second also verifies every live block.
