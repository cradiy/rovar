# Rovar

English | [简体中文](README.zh-CN.md)

A design editor built with Rust and GPUI, available on desktop and the Web. Create editable interfaces by hand or with an AI agent in the desktop editor.

Supports artboards, vectors, text, images, audio and video, auto layout, reusable components, interactive prototypes, multi-page documents, and PNG/SVG export. Use it locally or connect to a server with personal and team workspaces. Available in English and Simplified Chinese.

I want to build a beautiful, cross-platform UI design tool that is easy for AI agents to operate, while using the project's real-world needs to improve and extend the GPUI framework.

![Forma analytics dashboard](docs/screenshots/analytics.png)

![Still music player](docs/screenshots/music.png)

![Sora interface components](docs/screenshots/components.png)

## Features

- **AI-assisted design** — use an AI agent with the `rovar-design` skill to create editable interfaces, refine selected elements and layouts, import assets, and configure prototype interactions, with visual checks and undo support.
- **Canvas editing** — artboards, layers, multi-page documents, vector shapes, Bézier paths, and text.
- **Auto layout** — horizontal and vertical layouts, wrapping, alignment, padding, gaps, fixed/hug/fill sizing, and minimum/maximum dimensions.
- **Grid layouts** — fixed or equal column widths, row and column gaps, automatic placement, and row/column spans.
- **Responsive layouts** — edge, center, stretch, and scale constraints, plus frame size presets for phones, tablets, desktop screens, and more.
- **Precision tools** — snapping, distance measurements, equal spacing, multi-selection resizing, repeated duplication, and on-canvas corner editing.
- **Colors and gradients** — document color styles, color libraries, linear, radial, angular, and diamond gradients, plus four-point fills with draggable color sources.
- **Reusable components** — component libraries, instances, overrides, and updates.
- **Boolean operations** — union, subtract top layers, intersection, and exclusion for closed shapes and Bézier paths. Operands remain editable and can be released; combinations use the bottom layer's appearance and support PNG/SVG export.
- **Vector masks** — use the bottom closed shape or Boolean group to clip selected artwork. Edit the mask and content independently, nest or release mask groups, and export to PNG/SVG.
- **Layer blending** — Normal, Multiply, Screen, Overlay, Darken, and Lighten modes with independent layer opacity. Apply opacity to whole groups and preserve blending in PNG/SVG exports.
- **Component variants** — named versions in component sets, edited on canvas with the property panel, and instance switching that preserves matching overrides.
- **Shadows** — stacked outer and inner shadows for frames, shapes, images, and text, with offset, blur, spread, color, opacity, and PNG/SVG export.
- **Layer blur** — adjustable blur for shapes, images, and text, including shadows, with PNG/SVG export.
- **Contour glow** — colored light around frame, shape, text, and image outlines, including holes, with adjustable radius, edge width, intensity, and alpha threshold. PNG follows the export scale; SVG embeds the light at 1× and retains the vector artwork.
- **Background blur** — blur behind frames, rectangles, ellipses, and images, clipped to their rotated outlines and rounded corners. PNG follows the export scale; SVG embeds the blurred background at 1× while retaining vector foregrounds. Converting a primitive to a path removes its background blur; undo restores both.
- **SVG import** — keep artwork as a vector asset or convert supported SVGs into editable paths and groups.
- **Images, audio, and video** — media import, image fills, non-destructive cropping, audio cover artwork, and audio/video playback.
- **Interactive prototypes** — click and hover actions for frame navigation, component variants, overlays, media controls, and element property changes. Configure dissolve transitions and hover exit behavior, then play the prototype without changing the source design.
- **Local and self-hosted workspaces** — personal and team spaces, document and library synchronization, and version-conflict comparison.
- **Export** — `.rovar` documents, PNG, and SVG.
- **Desktop and Web** — a shared GPUI interface, English and Simplified Chinese; Web support is experimental.

## Planned

- [ ] **Android support**.
- [ ] **Visual effects** — glass and mixed effect stacks.
- [ ] **Animation** — keyframes, easing, animated gradients, and layer effects.
- [ ] **Advanced interactions** — additional triggers, scrollable areas, and more transition styles.
- [ ] **Presentation sharing** — standalone playback and browser sharing through a self-hosted server.
- [ ] **GPUI DSL export** — layouts, styles, components, and assets for GPUI applications.

## Run

Open **Prototype** in the right panel, choose a starting frame, and select a layer. Add an **On click** or **On hover** action, then choose its target and settings. Connect frames or variants on canvas, or use the panel to configure overlays, media controls, and property changes. Press **Present** beside the inspector tabs to play; press **Esc** to return to editing.

```sh
cargo run --locked -p rovar
```

## Design with AI

Use an AI agent that supports skills and local command execution to work in the running Windows, macOS, or Linux desktop editor. The agent can inspect your selection, create and refine editable layers, import assets, set prototype actions, and check rendered previews. Edits participate in the editor's normal undo and save/sync flow.

Install [just](https://github.com/casey/just) and [Nushell](https://www.nushell.sh/), then run from the repository root:

```sh
just install-skill
```

This installs or updates the [Rovar design skill](skills/rovar-design/SKILL.md) in `~/.agents/skills`. For an agent that uses a different skills directory, run `just install-skill-to /path/to/skills`.

Start a new agent session to load the skill, open a document in Rovar, and ask the agent to use `rovar-design`. For example:

- "Use rovar-design to create a travel planner with an itinerary and destination cards."
- "Use rovar-design to improve the spacing and typography of the selected card while keeping its content."

The skill connects to the running editor and checks its supported operations before making changes. This integration is available in desktop builds. See the [local protocol](docs/agent-protocol.md) for custom integrations.

## Build

Run `just` to list all build commands.

```sh
just build-linux       # Desktop binary
just build-linux-rpm   # RPM package
just build-macos       # App, ZIP and DMG
just build-windows     # Installer and portable ZIP
just build-server      # Server with the Web editor embedded
```

Packages are written to `dist/`. macOS builds require Apple Silicon and Xcode Command Line Tools; see [macOS packaging](packaging/macos/README.md).
Windows packaging requires x64 MSVC, 7-Zip and Inno Setup; see [Windows packaging](packaging/windows/README.md).

## Web and server

The Web editor is experimental. Open your server's address in a browser that supports WebGPU, using HTTPS or localhost.

Building the Web editor requires [Trunk](https://trunkrs.dev/) and the Rust `wasm32-unknown-unknown` target. See [server setup](crates/rovar-server/README.md) for deployment and registration settings.

## Development and contributions

Issues and pull requests are welcome! Feel free to report bugs, suggest features, discuss design improvements, or contribute code and documentation.

[File format](crates/rovar-format/FORMAT.md) · [MIT License](LICENSE)
