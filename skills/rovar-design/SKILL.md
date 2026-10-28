---
name: rovar-design
description: Create and refine editable designs in a running Rovar editor through its local protocol. Use for canvas design, selected-element edits, layout refinement, and visual verification in Rovar, not for developing the editor's source code.
---

# Rovar design

Work in the user's running editor. Keep designs editable and iterate on existing
elements. Treat the running application's capabilities and schemas as authoritative;
there is no pinned protocol version or legacy compatibility mode.

## Connect and discover

Read the running editor's discovery records before searching for an executable:

- Linux/macOS: `~/.rovar/instances/rovar-live-*/session.json`
- Windows: `%USERPROFILE%\.rovar\instances\rovar-live-*\session.json`

Each JSON record contains the absolute `executable`, `endpoint`, `pid`,
`application_version`, `started_at_unix_ms`, and `transport`. Use the executable and
endpoint from the same record. Do not guess build directories, installation paths,
or socket names. Honor a path explicitly supplied by the user. If no live record
exists, ask the user to start Rovar normally; stale records left by crashes are not
proof of a running editor. Do not launch duplicate editors merely to find a session.

In the commands below, replace `rovar` with the recorded executable path, quoted
when needed. `--list-sessions` checks reachability and returns the same metadata.
Normal application startup automatically exposes a local interface on Windows,
macOS and Linux; no agent startup flag is needed. Web does not expose this transport.

```sh
rovar --list-sessions
```

Use an actual discovered endpoint:

```sh
printf '%s\n' '{"request_id":"inspect-session","method":"session.describe"}' |
  rovar --connect /DISCOVERED/PATH/editor.sock
printf '%s\n' '{"request_id":"inspect-tabs","method":"tabs.list"}' |
  rovar --connect /DISCOVERED/PATH/editor.sock
printf '%s\n' '{"request_id":"inspect-apply-schema","method":"schema.get","params":{"method":"document.apply"}}' |
  rovar --connect /DISCOVERED/PATH/editor.sock
```

Windows PowerShell discovery (choose the intended record before connecting):

```powershell
Get-ChildItem "$env:USERPROFILE\.rovar\instances\rovar-live-*\session.json" |
    ForEach-Object { Get-Content -Raw -LiteralPath $_.FullName | ConvertFrom-Json }
$rovar = '/EXECUTABLE/FROM/RECORD'
& $rovar --list-sessions | Out-String
$endpoint = '\\.\pipe\rovar-PID-UUID'
'{"request_id":"inspect-session","method":"session.describe"}' |
  & $rovar --connect $endpoint | Out-String
```

Windows shells may not wait for GUI executables when invoked directly. Keep
PowerShell commands in a pipeline, or use `Start-Process -Wait -PassThru` for
explicit exit status. Agent process APIs should redirect stdin/stdout/stderr and
wait for exit. No separate CLI executable is needed.

Use UTF-8 for JSON input and output. In Windows PowerShell 5.1, set
`$OutputEncoding = [System.Text.UTF8Encoding]::new($false)` before piping non-ASCII
requests. PowerShell 7 uses UTF-8 by default.

`--connect` reads JSON Lines from stdin and returns JSON Lines on stdout; diagnostics
go to stderr. Inspect the JSON error even when the process exits unsuccessfully.
Use structured JSON serialization or a request file rather than interpolating design
text into shell commands. The CLI accepts multiple request lines.

Inspect each candidate's `tabs.list` and `session.describe` to identify the intended
window/document; do not choose the first process or tab arbitrarily. If discovery is
empty despite a running editor, check that both processes use the same user/home
directory. Do not edit persisted `.rovar` files as a substitute for the live protocol.

Before editing, record the target `session_id`, `document_id`, `page_id`, document
`revision`, selection, busy state, and capabilities. Query `schema.get` for the methods
you will use. Never infer supported properties from what the editor UI can do.

## Manage documents

Use `tabs.list` to inspect all windows and tabs, including empty windows and inactive
documents. Tab methods omit envelope `session_id` and `document_id`; their parameters
use the returned opaque `window_id` and, where required, `tab_id`.

- `tabs.create`: `window_id`, optional `title`. Create a document when the design task
  calls for one; it uses the window's current local/remote source.
- `tabs.open`: `window_id`, absolute `.rovar` `path`. External files are imported into
  the workspace. An already-open path activates its existing window/tab.
- `tabs.activate`: `window_id`, `tab_id`. Select the intended document and use
  `tab.session` from the response when `tab.ready` is true. AI can invoke this
  directly when the task requires loading or showing that document; do not ask the
  user to switch tabs manually or seek separate approval for an ordinary switch.
- `tabs.save`: `window_id`, `tab_id`. Save a loaded document without switching tabs.
- `tabs.close`: `window_id`, `tab_id`. Use when the task calls for closing that document;
  unrelated user tabs should remain open. Closing saves changes first.

Query the method schema before use. Mutations return an acceptance response, not a
completed disk operation. Poll `tabs.list` for loading/saving/closing state and errors.
A failed save keeps the tab; a failed open appears in the window's `open_errors`,
identified by `tab_id`. Do not proceed with editing until `session.describe` exposes
the intended session or a tab snapshot provides `ready:true` and `session`.
Each loaded tab's `session` includes session/document/page IDs, revision, and history
availability. An unloaded tab has `session:null`. If create/open/activate returns
`ready:false`, poll `tabs.list` and use the refreshed scope once ready. On `BUSY`,
inspect pending state rather than flooding retries.
After a timeout, retry the exact request ID/body to avoid creating duplicate tabs.
There are 256 successful tab-operation receipt slots per process, reported by
`remaining_tab_receipts`; reads consume none. Replayed responses are historical
acceptance snapshots, so use `tabs.list` for current state.

## Inspect the design

- `session.describe` includes loaded background tabs. Select the intended
  document/session explicitly and inspect `active` and `busy`; do not assume the
  first session is the visible document. Read, edit, capture, or manage its pages
  in the background when no view change is needed. Use `tabs.activate` only to
  load an unloaded tab or deliberately show it before selection/framing/page activation.
- Use `pages.list` to discover page UUIDs and counts. Node reads, `document.apply`,
  and captures can target an inactive page without changing the user's canvas.
  Revisions are document-wide; reread after edits on any page.
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

Use `pages.apply` for page creation, duplication, renaming, deletion, and ordering;
inspect its schema first. Each batch uses `base_revision` and is one undo entry.
Create and duplicate require a name and unique `$ref`; optional `before` controls
page ordering, otherwise they append. Copies get new node UUIDs and copied component
masters become instances. Preserve existing pages unless removal is requested.
These operations keep the current page visible unless it is deleted.

Call `pages.activate` with `page_id` and `revision` only when the task requires
showing that page or changing its selection/view. It restores the page's saved view
without changing document revision. Page mutations and activation share the apply
receipt cache: an exact retry acknowledges the earlier result without repeating it.
Use a fresh request ID for a deliberate later switch and reread current state after retries.

For a new design, establish the frame and major layout first, inspect the result,
then add and refine content. For an existing design, change the specific geometry,
typography, spacing, or styling at issue. After each meaningful batch, read the result
and capture the affected region before deciding the next adjustment. Let observed
results guide subsequent edits instead of repeatedly regenerating the full design.

Use `document.apply` with a fresh `request_id`, the inspected scope, and
`base_revision`. Submit related operations as one atomic batch within the limits
returned by discovery. Save the exact request until its outcome is known.

- `$references` resolve earlier creates, duplicates, or instances within one batch only. Use the returned UUIDs
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

For appearance edits, read `properties.fills`, `strokes`, `effects`, and `blend`.
Paint stacks are bottom-to-top; modify the intended entry in the returned list and
preserve every other entry. Submit only the changed list, not the entire node.
Use a layer's `hidden` to toggle it without deletion and `opacity` to fade the whole
paint without rewriting gradient-stop alpha. An empty list removes that stack.
Keep existing gradient stop IDs; omit IDs when adding stops. Do not submit the
simple `fill` alongside `fills`. Changing paint detaches its color style; unchanged
paints at the same indices preserve bindings. Import image/SVG sources first and
use the returned asset hash for image paints on the same page. Consult `schema.get`
for gradient, image-placement, and effect fields, including enum casing and color
formats; effect and point-gradient colors are normalized RGBA arrays. Observe the
node-specific paint/effect constraints and capture the result to verify composition.

For mixed typography, inspect the node's `text.runs` and `text.offset_unit`, then use
`edit_text` for the intended range. Offsets count Unicode scalar values, not UTF-8
bytes or JavaScript UTF-16 string indices; use returned run boundaries or count code
points explicitly. Offsets address the current content after earlier batch edits.
Patch only the requested style fields. Use local replacements to preserve formatting
outside the edit. Line height and horizontal alignment affect whole intersecting
paragraphs. Inspect the resulting runs and capture to verify wrapping and emphasis.

For layout, read and preserve the full `layout` and `sizing` objects before editing.
Use Grid columns/spans, wrapping, Hug/Fill, size limits, and constraints to express
responsive relationships rather than repeatedly patching absolute rectangles.
Fill requires a flow child; Hug must not depend on Fill on the same axis. Explicit
geometry requires `sizing.absolute:true` for children of auto layout. Constraints
are for non-flow children; provide only their modes and let the editor capture the
baseline geometry. Clear them with `remove_constraints` before moving into flow.
Read resolved bounds after applying and keep protected descendants intact.

Before introducing repeated colors, call `colors.list` and reuse an appropriate
document style. Create missing styles with `create_color_style`, then bind explicit
fill/stroke indices or text ranges using `bind_color_style`. Inspect
`color_style_bindings` and text-run `color_style` references before changing a paint.
Use `update_color_style` only when the change should propagate to every bound use
across pages and components. For a local exception, detach that binding with
`style:null` or explicitly edit its paint. Deleting a style preserves materialized
appearance but detaches all uses. Keep shared style edits small and inspect affected
pages; locked references cause atomic rejection. The protocol manages document
styles, not the application's separate local library.

For structure changes, prefer `reparent` to rebuilding a subtree and `duplicate`
to recreating repeated content. Inspect ordered `children` and `sibling_index`;
`before` in a reparent refers to a destination sibling in bottom-to-top order.
Reparenting preserves page coordinates before auto-layout resolution; read the
resolved geometry afterward. Duplicates remap internal action targets and preserve
external targets, so review the copied interactions before changing them.

Configure a plain group with `set_mask` or `set_boolean`; keep its children editable.
The mask must be a closed vector direct child. Boolean groups need closed vector
operands. Remove a conflicting mask, Boolean operation, or layout explicitly before
assigning another role. Read `properties.path` before editing Bezier control points:
the complete path is replaced, and points are normalized to the unrotated rect.

Use `create_component` on an existing subtree and `instantiate_component` for linked
reuse. Discover definitions and roots with `components.list`; inspect a node's
`component.master` before editing. Main-component edits propagate to instances on
other pages; instance edits retain overrides. Review `changed_pages` in the receipt.
Use `detach_component` only when independent content is intended. Component refs
resolve separately from node refs and are returned in `created_components`.

For reusable component states, inspect `components.list.sets` and each definition's
`roots`. Use `create_variant_set` to combine compatible definitions or start with
one member, then `add_variant` to clone a current definition into a new main
component. Supply distinct node `ref` and `component_ref` values. Edit the new main
subtree incrementally instead of recreating all states. Use `rename_variant_set`
and `rename_variant` for their display names. Apply `switch_variant` to an instance
root using a definition ID from the same set; do not detach and reconstruct the
instance. Inspect the resulting subtree because matching preserves corresponding
nodes and overrides while unmatched content may be added or removed. Switching a
main component is not supported; locate the desired main root instead. Metadata
and switches participate in ordinary atomic apply, revision checks, locks, and undo.

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
Prefer paths for large payloads and inspect the live request/data limits. For simple
SVGs such as icons, basic shapes, and small path illustrations, prefer importing
with `editable: true` so the result is editable paths rather than a single SVG asset.
Check the live import schema for where to set this field. Preserve a vector asset
when the user explicitly requests it, or when conversion cannot preserve a complex
SVG's appearance. Conversion failure rejects the batch; inspect the failure before
retrying, and explain any fallback to a non-editable SVG asset. Verify the converted
result with node reads and a capture.
Use the returned UUID or batch reference to attach interactions, and distinguish
successful import from verified media playback.
Move, resize, rename, or restyle imported assets by updating their existing UUIDs.
Adjust a converted SVG as a whole through its root group's `rect`; do not regenerate
the SVG just to change placement. Supply both dimensions to preserve the desired
aspect ratio. Plain-group transforms retain stroke widths and font sizes and reject
locked members or nested auto layout instead of partially transforming the group.
Use `replace_source` when only an asset's content changes. It preserves the existing
UUID, placement, styling, and actions and requires the same asset kind. SVG replacement
keeps a vector asset; do not pass `editable: true` or use it on converted path groups.
Check the returned `asset.hash` to confirm the source changed, then inspect the visual
result, especially when the new source has different proportions.

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

- Use `document.undo` / `document.redo` with `base_revision` to replay one ordinary
  document history entry. Activate the target tab yourself first if necessary.
  One apply batch is one undo entry; history also contains manual edits. For AI
  self-correction, undo only when the current revision still matches the edit being
  reverted and no intervening work would be undone. Otherwise inspect and make an
  incremental correction. Read the page/content again after replay because page
  identity and selection may change. Exact retries never undo or redo twice.
- `REVISION_CONFLICT`: reread the affected content and reconsider the patch. Never
  replace only the revision to force the old operations through.
- `BUSY`: let the current interaction finish; do not cancel the user's editing.
- Unknown outcome after a timeout/disconnection: retry the exact request ID and body
  only while the same session remains available. Successful apply receipts prevent
  duplicate edits. Do not invent a fresh ID and repeat an uncertain create.
- Session loss: rediscover and inspect before proceeding;
  do not redirect the pending patch into another document.
- Successful apply retries return the old acknowledgement, even after undo. Reread
  current state before deriving the next edit from that acknowledgement.
- Other validation failures: inspect the error and current schema, correct the
  offending operation, and retry a scoped batch. Stop repeating an unchanged failure.
