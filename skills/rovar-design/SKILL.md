---
name: rovar-design
description: Create and refine editable designs in a running Rovar editor through its local protocol. Use for canvas design, selected-element edits, layout refinement, and visual verification in Rovar, not for developing the editor's source code.
---

# Rovar design

Work in the user's running editor. Keep designs editable and iterate on existing
elements. Treat the running application's capabilities and schemas as authoritative;
there is no pinned protocol version or legacy compatibility mode.

## Connect and discover

Use the installed `rovar` binary, or an existing build in the user's checkout
(`target/debug/rovar` or `target/release/rovar`). Run `--help` if the CLI differs.
Normal application startup automatically exposes a local interface on Unix desktop
builds; no agent startup flag is needed. Windows and Web do not expose this transport.

```sh
rovar --list-sessions
```

The result lists process IDs and socket paths. Use an actual discovered path:

```sh
printf '%s\n' '{"request_id":"inspect-session","method":"session.describe"}' |
  rovar --connect /DISCOVERED/PATH/editor.sock
printf '%s\n' '{"request_id":"inspect-apply-schema","method":"schema.get","params":{"method":"document.apply"}}' |
  rovar --connect /DISCOVERED/PATH/editor.sock
```

`--connect` reads JSON Lines from stdin and returns JSON Lines on stdout; diagnostics
go to stderr. Inspect the JSON error even when the process exits unsuccessfully.
Use structured JSON serialization or a request file rather than interpolating design
text into shell commands. The CLI accepts multiple request lines.

Inspect each candidate's `session.describe` to identify the intended document. Do not
choose the first process or document arbitrarily. If no document is open, have the
user open or create one, or use an available authorized UI tool. Do not edit persisted
`.rovar` files as a substitute for the live protocol. If discovery is empty despite
a running editor, check that the client uses the same user's desktop runtime directory.

Before editing, record the active `session_id`, `document_id`, `page_id`, document
`revision`, selection, busy state, and capabilities. Query `schema.get` for the methods
you will use. Never infer supported properties from what the editor UI can do.

## Inspect the design

- For a selection-based request, resolve the current selection with `nodes.get`.
  Do not guess the selected element from an old screenshot or its name.
- Use `nodes.find` to locate relevant content and `nodes.get` to inspect parents,
  siblings, geometry, text, and styles before changing them. Names help locate nodes;
  stable UUIDs identify mutation targets.
- Read paginated results at one revision. If the revision changes, restart the read.
- Capture the affected frame with `canvas.capture` when visual context matters.
  Decode `data_base64` to a PNG file and inspect the image; do not print the encoded
  image into the conversation. This is an export-rendered subtree, not a screenshot
  of the desktop window or proof of native rendering parity.

## Modify incrementally

Prefer small, coherent edits followed by inspection over replacing the entire design.
Preserve existing UUIDs, hierarchy, and unrelated user content. Use `update` for
existing elements; create only what is missing and delete only what the task makes
obsolete. Do not delete and recreate a frame, page, or all its children on each pass.
Rebuild a whole region only when the user requests a redesign or the existing
structure cannot express the requested change; keep the replacement scoped to that region.

For a new design, establish the frame and major layout first, inspect the result,
then add and refine content. For an existing design, change the specific geometry,
typography, spacing, or styling at issue. After each meaningful batch, read the result
and capture the affected region before deciding the next adjustment. Let observed
results guide subsequent edits instead of repeatedly regenerating the full design.

Use `document.apply` with a fresh `request_id`, the inspected scope, and
`base_revision`. Submit related operations as one atomic batch within the limits
returned by discovery. Save the exact request until its outcome is known.

- `$references` resolve earlier creates within one batch only. Use the returned UUIDs
  in subsequent batches.
- Coordinates are page units, including children inside frames, independent of zoom
  and DPI. Check the current schema for complete compound-property requirements.
- Omit properties that are not changing. A supplied compound property replaces that
  property; it is not necessarily a partial patch.
- Respect type-specific constraints: groups have no fill, corner radius applies to
  rectangles, and auto-layout controls child positions. Inspect resolved geometry
  after layout changes. Do not flatten or rebuild a layout merely to move a child.
- Locked content and unsupported node types must remain intact. Do not silently
  approximate a requested unsupported feature with a destructive replacement.

Each changed apply is one ordinary undo entry. Its acknowledgement confirms an
in-memory edit, not completion of saving or synchronization.

For prototype behavior, read the node's `interactions`, `interaction_editable`, and
`available_variants` before editing. Use `set_interaction` for only the requested
trigger; preserve the other trigger and omit unchanged transition/hover-exit settings.
Variant targets are the returned component definition IDs, not arbitrary node IDs.
Navigation and media targets are existing page nodes or earlier batch references.
Configure hover exit explicitly when the task calls for restoration or persistence.
Setting an action configures Present-mode behavior; a static capture does not verify
that navigation, variant switching, or media playback actually executed.

Import assets with `document.apply`'s `import` operation and an explicit destination
rectangle. Images accept Base64 file bytes or absolute local paths; SVG accepts XML
text or absolute local paths. Audio/video require paths on the editor's machine.
Prefer paths for large payloads and inspect the live request/data limits. SVG remains
a vector asset unless `editable: true` is requested; conversion failure rejects the
batch. Respect that choice instead of silently changing the requested representation.
Use the returned UUID or batch reference to attach interactions, and distinguish
successful import from verified media playback.

## Verify and present

Use the returned revision to read changed nodes and capture their containing frame.
Check clipping, text wrapping, spacing, hierarchy, and the requested visual result.
If it needs improvement, update those same nodes in the next small batch.

Use the available view methods when helpful:

- `selection.set`: replace the selection with explicit UUIDs; `[]` clears it.
- `canvas.focus`: center explicit targets while retaining zoom.
- `canvas.fit`: fit explicit targets, or visible page content with `ids: []`.

Query their live schemas first. View operations do not modify document content or
undo history; focus/fit preserve selection. Avoid moving the viewport unnecessarily
while the user is working. When presenting a finished change, select or frame the
relevant result so the user can find it.

Report what changed and what was actually verified. Distinguish editable visual
mockups from working interactions, export captures from native-window screenshots,
and in-memory edits from confirmed saves.

## Recover without overwriting newer work

- `REVISION_CONFLICT`: reread the affected content and reconsider the patch. Never
  replace only the revision to force the old operations through.
- `BUSY`: let the current interaction finish; do not cancel the user's editing.
- Unknown outcome after a timeout/disconnection: retry the exact request ID and body
  only while the same session remains available. Successful apply receipts prevent
  duplicate edits. Do not invent a fresh ID and repeat an uncertain create.
- Session loss or changed active tab/page: rediscover and inspect before proceeding;
  do not redirect the pending patch into another document.
- Successful apply retries return the old acknowledgement, even after undo. Reread
  current state before deriving the next edit from that acknowledgement.
- Other validation failures: inspect the error and current schema, correct the
  offending operation, and retry a scoped batch. Stop repeating an unchanged failure.
