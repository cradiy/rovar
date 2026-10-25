# Local editor protocol

Desktop builds automatically expose the running editor through a private local endpoint:
a Unix socket on macOS/Linux or a named pipe on Windows.
Edits use the open document's history and normal save/sync behavior. An apply response
acknowledges an in-memory edit, not a completed disk save or remote upload.

## Connect

Start Rovar normally. No special startup arguments are needed, and no document needs
to be open. Every desktop process atomically publishes a discovery record at:

- Linux/macOS: `~/.rovar/instances/rovar-live-*/session.json`
- Windows: `%USERPROFILE%\.rovar\instances\rovar-live-*\session.json`

Read these files first; finding Rovar does not require PATH or guessing an installation
directory. Each record contains `executable` (the absolute running executable path),
`endpoint`, `pid`, `application_version`, `started_at_unix_ms`, and `transport`
(`unix_socket` or `named_pipe`). Use the recorded executable for CLI commands and
the matching endpoint for that process. Multiple instances have separate records.
If there are multiple candidates, inspect their sessions/tabs to choose the intended editor.
Records are removed on normal exit. A crash may leave a stale record, so connection
success, not file existence or PID alone, determines whether an instance is live.

Using the recorded executable, discover reachable running editors:

```sh
rovar --list-sessions
```

The JSON array contains the same metadata for reachable instances. Connect using
one of those endpoint addresses (replace `rovar` with the recorded executable):

```sh
printf '%s\n' '{"request_id":"describe-1","method":"session.describe"}' |
  rovar --connect /PATH/FROM/DISCOVERY/editor.sock
```

On Windows, the same `rovar.exe` opens the GUI without arguments and handles CLI
commands with arguments. To inspect the records in PowerShell:

```powershell
Get-ChildItem "$env:USERPROFILE\.rovar\instances\rovar-live-*\session.json" |
  ForEach-Object { Get-Content -Raw -LiteralPath $_.FullName | ConvertFrom-Json }
# Choose the intended record and use its executable and endpoint.
$rovar = '/EXECUTABLE/FROM/RECORD'
& $rovar --list-sessions | Out-String
# Copy the endpoint of the intended editor from discovery.
$endpoint = '\\.\pipe\rovar-PID-UUID'
'{"request_id":"describe-1","method":"session.describe"}' |
  & $rovar --connect $endpoint | Out-String
```

Windows shells may launch GUI executables without waiting. In PowerShell, keep
the command in a pipeline (as above) or use `Start-Process -Wait -PassThru` when
you need the process exit code. In interactive cmd.exe, use `start /wait`.
Agent process APIs should redirect standard input/output/error and wait for exit;
they do not need a wrapper executable. CLI startup attaches to the parent's console
when available and preserves redirected streams, without allocating a new console.

Requests and responses use UTF-8. In Windows PowerShell 5.1, set
`$OutputEncoding = [System.Text.UTF8Encoding]::new($false)` before piping non-ASCII
JSON to the CLI. PowerShell 7 uses UTF-8 by default.

`rovar --connect` reads newline-delimited JSON requests from stdin and writes JSON
responses to stdout. Diagnostics go to stderr. Exit status is 0 for successful
requests, 1 for protocol/connection/input failures, and 2 for invalid CLI arguments.
Each connection accepts one request. The CLI can send multiple input lines.
On Unix, each process also creates a unique `rovar-live-*` socket directory in the user runtime directory
(or system temporary directory if unavailable), with mode 0700 and a socket with
mode 0600. Discovery skips unreachable endpoints. Normal exit removes the process's
directory and its discovery record; a crash may leave unreachable records behind.
Unix discovery record directories have mode 0700 and records have mode 0600.
Socket locations are separate from discovery records to avoid Unix socket path length limits.
On Windows, discovery records use the user profile directory described above.
Pipe names contain a process ID and random UUID. Pipes reject remote clients and
use an explicit protected DACL granting access only to the current user.
Normal exit removes the discovery record and closes the listener; discovery skips
unreachable records left by crashes. Interface startup
failure is reported to stderr and does not prevent the editor from opening.
The interface is available to local processes running as the same user.

Document editing addresses all pages in loaded document tabs, including background tabs. `tabs.list`
also exposes inactive, unloaded, saving, and loading tabs, plus windows with no documents.
A tab/page change
does not redirect a request to a different document. Reopening, reloading, or moving
a document to another window can create a new session. Web builds do not expose
this transport.

## Discover before editing

`session.describe` returns loaded sessions, their window/tab/document/page IDs, revision,
selection UUIDs, `start_frame_id`, busy state, remaining receipt slots, and supported capabilities.
Each session includes `active`, indicating whether its tab is selected in its window.
Select the intended session by document identity; do not assume the first session is visible.
Loading, remote checks, closing, and pending window dialogs or drags mark sessions busy.
Unloaded tabs have no editing session; activate them once to load them.
Selection is observable and can be set explicitly; document mutations always name
their targets and never implicitly operate on the current selection.

`schema.get` returns a generated JSON Schema for a method's parameters:

```json
{"request_id":"schema-1","method":"schema.get","params":{"method":"document.apply"}}
```

Omit `params.method` to get the request envelope schema. Query the schema for the
method being used instead of guessing property names. Unknown fields and unknown
methods are rejected. The schemas describe the input structure; the constraints
below and node-dependent validation also apply.

All document methods require the `session_id` and `document_id` returned by discovery:

```json
{
  "request_id": "read-1",
  "session_id": "SESSION_FROM_DESCRIBE",
  "document_id": "DOCUMENT_FROM_DESCRIBE",
  "method": "nodes.get",
  "params": {
    "page_id": "PAGE_FROM_DESCRIBE",
    "revision": "REVISION_FROM_DESCRIBE",
    "limit": 50
  }
}
```

Document/session/page/node IDs are opaque UUIDs, not names or numeric indices.
Window/tab IDs are opaque strings as described below. Revisions are opaque strings.

| Method | Behavior |
| --- | --- |
| `session.describe` | List all loaded editor sessions, active/busy flags, and capabilities; empty parameters. |
| `schema.get` | Get the envelope or a method's input schema. |
| `tabs.list` | List all windows, tabs, load/save states, and opening errors; empty parameters. |
| `tabs.create` | Create and activate a document in a window, with an optional title. |
| `tabs.open` | Open an absolute `.rovar` file path, activating an existing tab if already open. |
| `tabs.activate` | Activate a specific tab and its window, loading the document if needed. |
| `tabs.close` | Close a specific tab through the normal save-before-close flow. |
| `tabs.save` | Request a save of a loaded tab without switching tabs. |
| `pages.list` | List ordered pages, node/frame counts, the active page, and document revision. |
| `pages.apply` | Atomically create, duplicate, rename, delete, or reorder pages. |
| `pages.activate` | Switch the visible page, restoring its saved selection and view. |
| `nodes.get` | Read specified UUIDs, or a page's nodes; returns parent UUIDs, properties, total, and next offset. |
| `nodes.find` | Same read contract, with an optional case-insensitive name substring and exact node type filter. |
| `document.apply` | Apply ordered node edits and prototype interaction settings atomically. |
| `canvas.capture` | Render one node's subtree as a PNG using the export renderer. |
| `selection.set` | Replace the selection with explicit UUIDs; an empty array clears it. |
| `canvas.focus` | Center explicit UUIDs in the available canvas area, keeping the zoom. |
| `canvas.fit` | Fit explicit UUIDs, or all visible page content when the array is empty. |

Reads and captures require a matching document revision. For pagination, keep the
same revision and use `next_offset`; a conflict requires restarting the read.
Read limits are 1–200 nodes. Returned order is bottom-to-top layer order.

## Pages and background editing

`pages.list` takes empty parameters and returns `revision`, `active_page_id`, and
ordered `pages` entries with `id`, `name`, `node_count`, and `frame_count`.
`nodes.get`, `nodes.find`, `document.apply`, and `canvas.capture` accept any existing
`page_id` in the targeted session. They do not activate that page. The revision is
document-wide: an edit on any page invalidates earlier reads on every page.
These methods and `pages.apply` also work in loaded background tabs. Their changes
participate in the target document's ordinary autosave and undo history, without
switching tabs or taking keyboard focus from the visible document.

`pages.apply` takes `base_revision` and 1–128 ordered `operations`:

- `create`: `ref`, `name`, optional `before` page UUID or earlier batch reference.
- `duplicate`: source `id`, `ref`, `name`, optional `before`.
- `rename`: `id`, `name`.
- `delete`: `id`.
- `reorder`: `id`, optional `before`.

New references use unique `$names`, scoped to that batch. Omitted `before` appends
to the page list; moving a page before itself is a no-op. Names must contain 1–200
characters and cannot be blank. Duplicates retain artwork and interactions with new
node UUIDs; copied component masters become instances of the original definition.
At least one page must remain when the batch finishes. Page deletion removes the
entire page, including locked content, just as the editor's page deletion does.

Creating or copying a page keeps the current page visible. If the active page is
deleted, the next surviving page in the original order becomes active, falling back
to the previous survivor, then the first newly created page. A changed batch creates
one undo entry; failures and no-ops create none. The result includes `revision`,
`active_page_id`, `changed`, `created` reference mappings, and ordered `pages`.

`pages.activate` takes `page_id` and `revision`. It changes no document content or
revision and creates no undo entry. Selection and canvas framing methods still
require the active page and return `PAGE_NOT_ACTIVE` otherwise. `pages.activate`,
selection, and framing also require the active tab and return `TAB_NOT_ACTIVE`
otherwise; use `tabs.activate` explicitly when the task requires showing a document.
Activation does not
interrupt an editor interaction: it returns `BUSY` while the editor is being edited.

`pages.apply` and `pages.activate` share the successful-request receipt cache with
`document.apply`. Exact retries return the original acknowledgement, including after
undo or later page switches; they never repeat a mutation or page switch. Use
`pages.list` or `session.describe` to read current state. Reads consume no receipts.

## Document tabs

Tab methods are process-scoped: omit envelope `session_id` and `document_id`.
Use opaque string `window_id` and `tab_id` values from `tabs.list`; tab IDs are scoped
to their window. Do not derive them from titles or array indices. Reopening a document
or moving a tab to another window can change its tab ID. After activation, use
`session.describe` to obtain the current editing session and revision.

```json
{"request_id":"new-design","method":"tabs.create","params":{"window_id":"WINDOW_ID","title":"Music player"}}
{"request_id":"open-design","method":"tabs.open","params":{"window_id":"WINDOW_ID","path":"/absolute/design.rovar"}}
{"request_id":"activate-design","method":"tabs.activate","params":{"window_id":"WINDOW_ID","tab_id":"TAB_ID"}}
{"request_id":"close-design","method":"tabs.close","params":{"window_id":"WINDOW_ID","tab_id":"TAB_ID"}}
```

Create uses the window's current local/remote source and normal autosave behavior.
External files are imported into the workspace, as with the UI; their source files
are not overwritten. If a path is already open in another window, `tabs.open`
activates that window and returns its IDs. Remote documents already in the workspace
can be opened using the path returned by `tabs.list`.

Mutations return `status: "accepted"`, `window_id`, `tab_id`, and a `tab` snapshot
(null if already removed). Acceptance is not proof of completed loading, saving,
remote upload, or closing. Poll `tabs.list`: inspect `loaded`, `loading`,
`checking_remote`, `saving`, `closing`, `dirty`, and `error`. A successful close
removes the tab; a failed save retains it with an error. A failed open removes the
loading tab and reports `{tab_id, title, message}` in the window's `open_errors`
while the corresponding error dialog remains open. Pending dialogs, comparisons,
and tab drags return `BUSY`. Closing never discards unsaved changes.

Keep the same request ID and exact body when retrying a mutation after a timeout.
Each process retains up to 256 successful tab-operation receipts; a repeated request
returns its original acceptance response without executing again. Changed bodies
with a used ID return `REQUEST_ID_REUSED`; exhausted capacity returns `RECEIPT_LIMIT`
before any change. `tabs.list` reports `remaining_tab_receipts`. Requests are limited
to 16 KiB. Read requests do not consume receipt slots. Poll for current state rather
than treating a replayed snapshot as live state.

## Selection and view

`selection.set`, `canvas.focus`, and `canvas.fit` take `page_id`, `revision`, and
`ids` (a required array of up to 200 UUIDs). `canvas.focus` requires a nonempty array.
They require the same document/session scope and current revision as reads:

```json
{
  "request_id": "fit-card-1",
  "session_id": "SESSION_FROM_DESCRIBE",
  "document_id": "DOCUMENT_FROM_DESCRIBE",
  "method": "canvas.fit",
  "params": {
    "page_id": "PAGE_FROM_DESCRIBE",
    "revision": "REVISION_FROM_DESCRIBE",
    "ids": ["FRAME_UUID"]
  }
}
```

Selection rejects hidden/locked nodes and their descendants with `NOT_SELECTABLE`.
Selecting an ancestor and its descendant keeps the ancestor, matching editor behavior.
Focus/fit preserve selection and reject hidden targets with `NOT_VISIBLE`; visible
locked nodes can still be framed. All IDs are validated before changing selection
or view. Empty content has no fitting bounds and returns `INVALID_ARGUMENT`.

Framing accounts for rotated bounds, side panels, and the editor's existing margins.
Fit uses the editor's 10–100% zoom range; exceptionally large artwork may still exceed
the viewport at minimum zoom. Focus only pans and may leave large targets partly
outside the viewport. Neither operation activates another window or changes pages.
An unlaid-out canvas or active editor interaction returns `BUSY`.

Responses return `page_id`, the unchanged document `revision`, actual `selection`,
and `view` (`zoom` and `pan` in logical pixels relative to the canvas origin).
`session.describe` also reports this view state. These operations do not modify
document content or undo history. They re-execute on retry using the current view;
the successful-request receipt cache applies to `document.apply`, `pages.apply`,
and `pages.activate`, not selection or framing.

## Prototype interactions

Node reads return `interactions.click` and `interactions.hover`, including the action
and transition. Hover also returns `hover_exit`. `available_variants` lists the
source component's other variants as `{id, name}`; these IDs identify component
definitions, not page nodes. An empty list means variant switching is unavailable.

Use `set_interaction` within `document.apply` to replace one trigger's action:

```json
{
  "op": "set_interaction",
  "id": "SOURCE_NODE_UUID",
  "trigger": "hover",
  "action": {"type": "navigate", "target": "FRAME_UUID"},
  "hover_exit": {"type": "restore"},
  "transition": {"type": "dissolve", "duration_ms": 200}
}
```

- Triggers are `click` and `hover`. Changing one preserves the other.
- Actions are `none`, `navigate`, `back`, `open_overlay`, `close_overlay`,
  `change_variant`, `media`, and `set_property`. Query `document.apply`'s schema for their fields.
- Frame, media, and property targets use the requested page's node UUIDs or earlier batch `$references`.
  Variant targets must be among the source's `available_variants`.
- `open_overlay` requires `settings`: `position` (`center`, `bottom`, `right`),
  `backdrop`, and `dismiss_outside`.
- `media` requires an existing audio/video target and a `command`: play, pause,
  toggle, restart, seek_by, seek_to, rate, volume, mute, or unmute. Rate is video-only
  and accepts 25–400 percent; volume accepts 0–100 percent. Setting the action does
  not start media playback; the trigger executes it in Present mode.
- `set_property` changes one property of a target element in Present mode. For example,
  `{"type":"set_property","target":"NODE_UUID","property":{"property":"visible","value":false}}`
  hides an element; `{"property":"visible","value":"toggle"}` toggles its visibility.
  Boolean properties (`visible`, `fill_enabled`, `stroke_enabled`, `layout_wrap`)
  accept `true`, `false`, or `"toggle"`. Toggle resolves against the current playback
  value each time the action runs; authoring never changes the target value.
  Node reads return `action_properties`, listing supported properties with current values.
  Hidden targets are allowed. Coordinates are local to the containing frame; auto-layout
  positions and plain group geometry are unavailable. Width/height changes use fixed sizing.
  Rotation is available for shapes and text, using the same angle normalization as Design.
  Fill and stroke values use the complete paint structure returned by `action_properties`:
  `color` (`#RRGGBB` or `#RRGGBBAA`), `mode`, `gradient`, `image`, and `image_hash`.
  Edit that returned value incrementally. Fill supports solid, gradient, image, and point
  paints where supported by the target; text and strokes support solid and gradient paints.
  Image hashes must reference an existing image on the requested page. Layer opacity is 0–1,
  and font families must be installed.
  Properties affect the isolated playback scene, restore on hover exit when configured,
  and reset on restart. They never edit the source document or component definition.
- Omitted/null `transition` and `hover_exit` preserve their current values.
  Transitions are `instant` or `dissolve` (50–2000 ms), using the editor's existing
  playback behavior. Hover exit is `restore`, `keep`, or `change_variant` with an
  available variant UUID. It may only be supplied for a nonempty hover action.
- `{"type":"none"}` clears that trigger and resets its transition. Clearing hover
  also resets its exit behavior to `restore`. Do not supply transition/exit settings
  when clearing an action.
- Locked sources and descendants of locked nodes reject interaction changes.
  Target type, media command, and variant membership are validated before committing
  the batch. The same revision, retry, and undo contracts apply as for node edits.

`{"op":"set_start_frame","target":"FRAME_UUID"}` sets the requested page's starting
frame. A null target clears the explicit starting frame. Apply responses and
`session.describe` return `start_frame_id`.

## Apply

The write surface includes frames, rectangles, ellipses, Bezier paths, text, and
plain or layout groups. Properties cover names, geometry, solid fills, opacity, rotation, visibility,
rectangle corner radius, whole-box text/font styling, and horizontal/vertical layout.
Existing vector and media nodes support basic property edits, duplication, and
deletion. Source replacement is supported for asset nodes of the same kind;
Bezier control points are readable and writable through `properties.path`.
Subtrees can be duplicated, reparented, masked, combined with Boolean operations,
and bound to components. `editable` describes property editing; `interaction_editable`
separately describes whether a node's interaction settings can be changed.

```json
{
  "request_id": "create-card-1",
  "session_id": "SESSION_FROM_DESCRIBE",
  "document_id": "DOCUMENT_FROM_DESCRIBE",
  "method": "document.apply",
  "params": {
    "page_id": "PAGE_FROM_DESCRIBE",
    "base_revision": "REVISION_FROM_DESCRIBE",
    "operations": [
      {
        "op": "create",
        "ref": "$card",
        "type": "frame",
        "properties": {
          "name": "Card",
          "rect": {"x": 100, "y": 100, "width": 320, "height": 180},
          "fill": "#FFFFFFFF"
        }
      },
      {
        "op": "create",
        "ref": "$title",
        "type": "text",
        "parent": "$card",
        "properties": {
          "name": "Title",
          "rect": {"x": 124, "y": 124, "width": 272, "height": 40},
          "content": "Welcome",
          "font_size": 24,
          "fill": "#20232BFF"
        }
      }
    ]
  }
}
```

- `$references` exist only within the batch, must be unique, and can only refer to
  earlier creates. `created` in the response maps them to permanent UUIDs. Subsequent
  requests must use those UUIDs.
- Coordinates are logical page units, positive right/down, independent of zoom/DPI.
  Even children inside a frame use page coordinates in this protocol. `rect` is the
  unrotated box; rotation is clockwise around its center and is not inherited.
- Create requires a complete `rect`. Width and height are 1–100000; coordinates have
  magnitude at most 1000000. New frames are page roots. Other nodes may have a frame
  or group parent. Omitted/null parent means the page root.
- New groups can be plain containers; their visible bounds come from their children.
  Their creation rectangle is used only when a layout is supplied. Layout
  requires direction, gap, padding `[top,right,bottom,left]`, main alignment and cross
  alignment. Alignments are `start`, `center`, or `end`. Layout-controlled child
  positions are resolved by the editor; updates cannot set their `rect` directly.
- Updating an existing plain group's `rect` moves/scales its members together in
  page coordinates, including nested plain groups and converted SVG paths. Node
  UUIDs, source assets, normalized path points, interactions, and style values stay
  intact. Fonts and stroke widths do not scale. All members must be editable and
  remain within the geometry limits; groups containing auto layout return
  `LAYOUT_CONTROLLED` so their layout frames can be edited explicitly instead.
- `properties.path` replaces a Bezier path with `{closed, nodes}`. Each of the 2–4096
  nodes supplies `anchor: {x,y}`, optional `incoming`/`outgoing` handles (defaulting
  to the anchor), and optional `smooth`. Points are normalized to the node's `rect`,
  before layer rotation; handles may lie outside `[0,1]`. Bezier creation requires
  both `rect` and `path`. Read the current path before changing individual handles.
- `duplicate` takes `id`, a new node `ref`, and optional `offset: {x,y}` in page units
  (default zero). It copies the subtree under the same parent with fresh UUIDs,
  preserves media and styling, remaps internal action targets, and retains external
  action targets. Copied component bindings are instances, never additional masters.
- `reparent` takes `id`, `parent` (UUID/$ref or null for the page), and optional
  `before` (a different destination sibling). Sibling order is bottom-to-top;
  omitting `before` places the subtree above its siblings. Page geometry is retained
  before auto-layout resolution. Cycles and frames nested inside frames are rejected.
  Moving an instance member outside its instance requires detaching that instance
  first (`COMPONENT_BOUNDARY`); moving the whole instance preserves its binding.
  Reads return ordered `children` UUIDs and `sibling_index`.
- `set_mask` takes a group `id` and `source` (a closed vector direct child, or null
  to remove clipping). `set_boolean` takes a group `id` and `operation` (`union`,
  `subtract`, `intersect`, `exclude`, or null). Establish at least two closed vector
  operands before enabling a Boolean group. Mask, Boolean, and auto-layout roles
  are mutually exclusive. Children remain editable; invalid final structures fail
  atomically. Reads expose `mask_source_id` and `boolean_operation`.
- `create_component` takes an unbound subtree `id`, a component `ref`, and `name`.
  It creates a main component. `instantiate_component` takes `component` (definition
  UUID or earlier component `$ref`), a node `ref`, optional `parent`, and page-space
  top-left `position: {x,y}`. `detach_component` takes a binding root `id` and keeps
  its contents. Nested components are not supported. `session.describe` lists
  component IDs/names; node reads expose a root's `component: {id,master}`.
  Component references are returned separately in `created_components`; node
  references are returned in `created`.
- Ordinary `update`, `delete`, and structure operations can edit component content.
  Main-component changes synchronize instances on all document pages while retaining
  instance overrides. The apply response lists `changed_pages`; all affected pages
  and definitions share one undo entry. Locked objects on any affected page remain
  protected. These operations still require an open editor and an active target page.
- Image, SVG, video, and audio nodes accept name, rect, opacity, rotation, and hidden
  updates. `rect` supplies both dimensions explicitly; preserve the source aspect
  ratio when that is desired. Assets have no solid fill. Bezier nodes accept basic
  transforms and layer styling; solid fill is available only for closed paths.
- Omitted/null property values leave the property unchanged. Supplied compound
  properties such as `rect` and `layout` replace that complete property. Use
  `remove_layout:true` to remove layout; it conflicts with a supplied layout.
- Colors are solid sRGB `#RRGGBBAA`; opacity is 0–1. Setting fill replaces the current
  paint with a solid fill and detaches its color style. Text styles apply to the
  entire text box. Replacing text follows the editor's style inheritance rules.
- Font size is 1–1000 and weight 100–900. An explicit family must exist; otherwise
  `FONT_NOT_FOUND` is returned. Omit it to use the editor default.
- Text nodes also accept boolean `italic`, `underline`, and `strikethrough`
  properties. These apply to the entire text box and can be combined with font weight.
- Locked nodes and descendants of locked nodes cannot be edited or deleted. Layout
  edits that move protected descendants also fail. Active editing/dragging/previews
  return `BUSY` rather than finishing or interrupting the user's gesture.

All operations run on a copy and are validated, including resolved layout, before
commit. Failure leaves the document and undo history unchanged. A changed batch
creates exactly one ordinary undo entry; a no-op creates none. Selection is preserved
except for deleted or no longer selectable nodes. Apply returns the resulting revision, created references,
updated node descriptions, and deleted UUIDs. It does not implicitly select the new
nodes or move the viewport.

Successful requests are remembered for the session: retrying the identical
`request_id` and body returns the original result without applying it again. Reusing
a successful ID with a different body returns `REQUEST_ID_REUSED`. This remains true
after undo: a receipt is an acknowledgement of the old commit, not the current state.
Reread before continuing. Failed requests make no edit, except `TIMEOUT`, whose outcome
is unknown: retry the identical request while the session is still available.
After session loss, rediscover and inspect instead of blindly submitting a new edit.

Requests are limited to 24 MiB and applies to 128 operations. Receipts are retained
without eviction up to 256 successful requests or 16 MiB of request fingerprints and
results; inline asset bytes are not retained in the receipt cache.
At capacity, new applies fail before commit with `LIMIT_REACHED`; existing
receipts remain retryable. Reopen the saved document to start a new session.

## Import images, SVG, video, and audio

Use `import` inside `document.apply`. Each import requires a unique batch `ref`, a
destination `rect` in page coordinates, and a typed `source`. Optional `parent`
accepts a frame/group UUID or earlier batch reference. Optional `name` labels the
imported node; it defaults to the file name. Placement never depends on selection,
zoom, or the file's intrinsic dimensions.

```json
{
  "op": "import",
  "ref": "$photo",
  "parent": "FRAME_UUID",
  "name": "Cover",
  "rect": {"x": 120, "y": 140, "width": 320, "height": 240},
  "source": {
    "type": "image",
    "source": {"type": "data", "base64": "ENCODED_IMAGE_FILE_BYTES"}
  }
}
```

Supported `source` structures:

| Kind | Local file | Direct data |
| --- | --- | --- |
| Image | `{"type":"image","source":{"type":"path","path":"/absolute/photo.png"}}` | `{"type":"image","source":{"type":"data","base64":"..."}}` |
| SVG | `{"type":"svg","source":{"type":"path","path":"/absolute/icon.svg"}}` | `{"type":"svg","source":{"type":"data","svg":"<svg ...>...</svg>"}}` |
| Video | `{"type":"video","path":"/absolute/clip.mp4"}` | Not supported |
| Audio | `{"type":"audio","path":"/absolute/music.mp3"}` | Not supported |

Paths must identify existing absolute local files accessible to the editor process.
URLs, directories, and relative paths are rejected. Video/audio use the editor's
existing formats and platform decoders. A successful import is not playback verification.
Images use standard Base64 of the encoded file; SVG uses XML text. Neither accepts
a data URI prefix. Direct data is limited to 16 MiB decoded per asset, within the
24 MiB total JSON request limit; use a file path for larger images. Existing image
decoder limits and the SVG parser's 16 MiB source limit also apply.

SVG defaults to a vector asset. Set `editable: true` on its outer `source` object
to require conversion into editable paths/groups. Unsupported conversion returns
`SVG_NOT_EDITABLE` with the reason and leaves the entire batch unchanged; it does
not silently substitute an asset. SVG asset nodes report `asset.svg_editable` and
`asset.svg_conversion_error`; other asset metadata includes name, width, and height.
The returned root UUID references the converted group or preserved asset.

File reads, decoding, and SVG parsing run in the background. Before committing, the
editor checks session, page existence, document revision, busy state, and request
deadline again. Failure or stale state leaves the document and history unchanged.
Imported source bytes are owned by the document and embedded by normal saving;
removing the original path afterwards does not break undo/redo or saved documents.
Retrying an already successful request returns its receipt without rereading files.

Later operations in the same batch may use the import's `$reference`, including
setting its prototype interactions. The entire batch is one undo entry. Importing
preserves source data during subsequent basic property edits. To adjust a converted
SVG as a whole, update its returned root group's rectangle instead of reimporting it.

Use `replace_source` to change an existing asset without rebuilding its node:

```json
{
  "op": "replace_source",
  "id": "EXISTING_IMAGE_UUID",
  "source": {"type": "image", "source": {"type": "path", "path": "/absolute/new-cover.png"}}
}
```

It accepts the same typed source structures and limits as import, including earlier
batch references in `id`. The replacement must have the same kind (image, SVG, video,
or audio). SVG must remain a vector asset; `editable: true` is rejected. Converted
path groups do not have a replaceable source. Node identity, name, geometry, crop,
layer styling, and interactions are preserved. Different source proportions keep
the current rectangle and placement settings; adjust those separately if desired.
Reads expose `asset.hash` so changed content is observable even when its name and
dimensions stay the same. Equal source content and name make replacement a no-op.
Undo restores the old source. Changing a media source invalidates its old player
and pending commands; playback position is not transferred to the new file.

## Capture and verification

Call `canvas.capture` with `page_id`, `revision`, `node_id`, and optional integer
`scale` (1–4; default 1). Output is limited to 4 million pixels. It returns
`data_base64`, `mime_type`, `width`, `height`, `canvas_rect`, and the captured revision.
Rasterization runs off the UI thread from the captured snapshot; subsequent user
edits do not change that snapshot. Use `canvas_rect` and image dimensions to map image
coordinates to page coordinates.

The renderer is explicitly identified as `export`: this is a subtree render without
editor chrome or surrounding sibling content, not a screenshot of the native window.
Subtrees containing audio/video return `UNSUPPORTED`. Rendering/resource errors return
`CAPTURE_FAILED`. Use structural reads and captures together; do not infer native
viewport rendering parity from an exported PNG.

After apply, use the returned revision to read affected nodes or capture their
containing frame. On `REVISION_CONFLICT`, rediscover and reread before constructing a
new request. Never replace just the revision to force a stale edit through.

Import read/decode failures return `IMPORT_FAILED`; type/placement errors return
`INVALID_ARGUMENT`. Errors contain `code`, `message`, and, when attributable to an operation, its zero-based
`operation` index. Other codes include `INVALID_ARGUMENT`, `NODE_NOT_FOUND`, `LOCKED`,
`LAYOUT_CONTROLLED`, `PAGE_NOT_FOUND`, `PAGE_NOT_ACTIVE`, `TAB_NOT_ACTIVE`, and `SESSION_CLOSED`.
