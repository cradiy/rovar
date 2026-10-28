# Changelog

## 0.4.0 (Released)

### Added

- Layered fills and strokes with independent paint, opacity, visibility, and ordering controls.
- Bold, italic, underline, and strikethrough text formatting.
- Expanded AI design controls for tabs, pages, components, variants, masks, Boolean operations, path editing, text ranges, layouts, effects, and shared color styles, including background document editing and undo/redo.
- Install the bundled `rovar-design` skill from Settings and check that it matches the running application.
- A dedicated settings page with startup update notifications and manual release checks.

### Improved

- Softer editor sidebars with clearer groups and spacing.
- Redesigned color style picker with inline saving, and gradient panels that grow with available space while keeping their position stable.
- Discover running editor instances and their executable paths through a shared location.
- Use the system certificate store for HTTPS connections.

### Fixed

- Release image atlas resources when their last owner is dropped.
- Keep paint layer selection and gradient stops consistent when rearranging layers or using undo/redo.
- Keep paint opacity controls and image fill previews in sync, and prevent input blur from reselecting another layer.
- Match document preview and project thumbnail backgrounds to the active theme.

## 0.3.0

AI agents can now create and refine editable designs in the running desktop editor, with a local protocol and the `rovar-design` skill.

### Added

- AI-assisted design on Windows, macOS, and Linux: inspect the current document and selection, create and update layers, refine layouts, and review rendered previews. Changes use the editor's undo history and normal save/sync flow.
- The `rovar-design` skill and `just install-skill` installer, with discovery of installed desktop applications.
- Local CLI session discovery, live capability and schema queries, revision-checked atomic edits, and retry handling that prevents duplicate edits.
- Agent-driven image, SVG, audio, and video imports, asset source replacement that preserves existing layers, and prototype interaction configuration.
- SVG import with the choice to preserve the vector asset or convert supported artwork into editable paths and groups.
- Audio import and playback, embedded cover artwork, and a shared media menu for image, audio, and video imports.
- Prototype dissolve transitions with configurable durations for frame navigation and component variant switching.
- Prototype overlays with placement, backdrop, and outside-click dismissal controls.
- Prototype audio/video actions for playback, seeking, volume, and muting, plus video playback speed.
- Prototype property actions to change an element's appearance, geometry, text, or layout during playback, without modifying the source document.
- Canvas context-menu creation and file imports at the clicked position, plus a control to collapse all layers.

### Improved

- Reduce repeated scene and hierarchy scans, cache group bounds, and skip off-screen artwork to improve editing and rendering of large canvases.
- Simplify desktop and Web/server build commands and include shader compiler support in Windows MSVC builds.

### Fixed

- Match editing shortcut bindings and labels to the runtime platform, including the Web editor.
- Preserve keyboard focus when scrubbing gradients or clicking inspector and font controls.
- Fix first-time macOS app packaging when the destination application bundle does not exist.

## 0.2.0

### Added

- Responsive layout constraints, wrapping, minimum/maximum sizes, frame presets, and editable grid layouts.
- Component sets with named variants and instance switching that preserves matching overrides.
- Non-destructive Boolean operations and editable, nested vector masks.
- Stacked drop shadows, inner shadows, layer blur, background blur, and contour glow with PNG/SVG export.
- Layer blend modes, group opacity, and editable four-point gradient fills.
- Prototype connections with click/hover navigation, component variant switching, and configurable hover exit behavior.
- System, light, and dark themes, plus animated inspector disclosures.

### Improved

- Simplified the presentation toolbar and automatically fit frames to the available viewport.
- Preserve layout constraints when moving objects or scrubbing properties.
- Correct background blur compositing and recover node identities in legacy component snapshots.
- Restore temporary hover navigation correctly and end hover interactions when the playback window is resized.

## 0.1.0

Initial release.

- Desktop editor for Linux, macOS, and Windows, with an experimental Web editor and persistent browser workspaces.
- Multi-page documents, artboards, vector shapes, Bézier editing, text, images, video, and non-destructive image cropping.
- Auto layout, reusable components and linked instances, color styles, and editable multi-stop gradients.
- Canvas measurements, snapping, multi-selection resizing, repeated duplication, corner editing, and trackpad zoom.
- Local files and self-hosted personal/team workspaces, incremental synchronization, account management, and conflict resolution.
- PNG/SVG export with per-object settings, document thumbnails, and English/Simplified Chinese interfaces.
- Cross-platform packaging, automated checks, and release drafts generated from version tags.
