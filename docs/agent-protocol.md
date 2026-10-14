# Local editor protocol

The Unix desktop build automatically exposes the running editor through a private local socket.
Edits use the open document's history and normal save/sync behavior. An apply response
acknowledges an in-memory edit, not a completed disk save or remote upload.

## Connect

Start Rovar normally and open a document. No special startup arguments are needed.
In another terminal, discover running editors:

```sh
rovar --list-sessions
```

The JSON array contains each running process's `pid` and `socket`. Connect using
one of those socket paths:

```sh
printf '%s\n' '{"request_id":"describe-1","method":"session.describe"}' |
  rovar --connect /PATH/FROM/DISCOVERY/editor.sock
```

`rovar --connect` reads newline-delimited JSON requests from stdin and writes JSON
responses to stdout. Diagnostics go to stderr. Exit status is 0 for successful
requests, 1 for protocol/connection/input failures, and 2 for invalid CLI arguments.
Each socket connection accepts one request. The CLI can send multiple input lines.
Each process creates a unique `rovar-live-*` directory in the user runtime directory
(or system temporary directory if unavailable), with mode 0700 and a socket with
mode 0600. Discovery skips unreachable endpoints. Normal exit removes the process's
directory; a crash may leave an unreachable directory behind. Interface startup
failure is reported to stderr and does not prevent the editor from opening.
The interface is available to local processes running as the same user.

Only active document tabs and their active pages are addressable. A tab/page change
does not redirect a request to a different document. Reopening, reloading, or moving
a document to another window can create a new session. Windows and Web builds do
not expose this transport.

## Discover before editing

`session.describe` returns active sessions, their document/page IDs, revision,
selection UUIDs, `start_frame_id`, busy state, remaining receipt slots, and supported capabilities.
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

IDs are opaque UUIDs, not names or numeric indices. Revisions are opaque strings.

| Method | Behavior |
| --- | --- |
| `session.describe` | List active editor sessions and capabilities; empty parameters. |
| `schema.get` | Get the envelope or a method's input schema. |
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
the successful-apply receipt cache described below applies only to `document.apply`.

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
  `change_variant`, and `media`. Query `document.apply`'s schema for their fields.
- Frame and media targets use active-page node UUIDs or earlier batch `$references`.
  Variant targets must be among the source's `available_variants`.
- `open_overlay` requires `settings`: `position` (`center`, `bottom`, `right`),
  `backdrop`, and `dismiss_outside`.
- `media` requires an existing audio/video target and a `command`: play, pause,
  toggle, restart, seek_by, seek_to, rate, volume, mute, or unmute. Rate is video-only
  and accepts 25–400 percent; volume accepts 0–100 percent. Setting the action does
  not start media playback; the trigger executes it in Present mode.
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

`{"op":"set_start_frame","target":"FRAME_UUID"}` sets the active page's starting
frame. A null target clears the explicit starting frame. Apply responses and
`session.describe` return `start_frame_id`.

## Apply

The current write surface includes frames, rectangles, ellipses, text, and layout
groups. Properties cover names, geometry, solid fills, opacity, rotation, visibility,
rectangle corner radius, whole-box text/font styling, and horizontal/vertical layout.
Other node types can be read and have interactions configured. Component, mask,
boolean, and media geometry, asset import, reparenting, and duplication are not
exposed for mutation. `editable` describes property editing; `interaction_editable`
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
- New groups require a layout and a rectangle defining the layout frame. Layout
  requires direction, gap, padding `[top,right,bottom,left]`, main alignment and cross
  alignment. Alignments are `start`, `center`, or `end`. Layout-controlled child
  positions are resolved by the editor; updates cannot set their `rect` directly.
- Omitted/null property values leave the property unchanged. Supplied compound
  properties such as `rect` and `layout` replace that complete property. Use
  `remove_layout:true` to remove layout; it conflicts with a supplied layout.
- Colors are solid sRGB `#RRGGBBAA`; opacity is 0–1. Setting fill replaces the current
  paint with a solid fill and detaches its color style. Text styles apply to the
  entire text box. Replacing text follows the editor's style inheritance rules.
- Font size is 1–1000 and weight 100–900. An explicit family must exist; otherwise
  `FONT_NOT_FOUND` is returned. Omit it to use the editor default.
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

Requests are limited to 1 MiB and applies to 128 operations. Receipts are retained
without eviction up to 256 successful requests or 16 MiB of serialized requests and
results. At capacity, new applies fail before commit with `LIMIT_REACHED`; existing
receipts remain retryable. Reopen the saved document to start a new session.

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

Errors contain `code`, `message`, and, when attributable to an operation, its zero-based
`operation` index. Other codes include `INVALID_ARGUMENT`, `NODE_NOT_FOUND`, `LOCKED`,
`LAYOUT_CONTROLLED`, `PAGE_NOT_ACTIVE`, and `SESSION_CLOSED`.
