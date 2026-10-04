# Rovar

English | [简体中文](README.zh-CN.md)

A design editor built with Rust and GPUI, available on desktop and the Web.

Supports artboards, vectors, text, images and video, auto layout, reusable components, multi-page documents, and PNG/SVG export. Use it locally or connect to a server with personal and team workspaces. Available in English and Simplified Chinese.

I want to build a beautiful, cross-platform UI design tool that is easy for AI agents to operate, while using the project's real-world needs to improve and extend the GPUI framework.

![Forma analytics dashboard](docs/screenshots/analytics.png)

![Morrow music player](docs/screenshots/music.png)

![Sora interface components](docs/screenshots/components.png)

## Features

- **Canvas editing** — artboards, layers, multi-page documents, vector shapes, Bézier paths, and text.
- **Auto layout** — horizontal and vertical layouts, wrapping, alignment, padding, gaps, fixed/hug/fill sizing, and minimum/maximum dimensions.
- **Grid layouts** — fixed or equal column widths, row and column gaps, automatic placement, and row/column spans.
- **Responsive layouts** — edge, center, stretch, and scale constraints, plus frame size presets for phones, tablets, desktop screens, and more.
- **Precision tools** — snapping, distance measurements, equal spacing, multi-selection resizing, repeated duplication, and on-canvas corner editing.
- **Colors and gradients** — document color styles, color libraries, and linear, radial, angular, and diamond gradients.
- **Reusable components** — component libraries, instances, overrides, and updates.
- **Component variants** — named versions in component sets, edited on canvas with the property panel, and instance switching that preserves matching overrides.
- **Drop shadows** — stacked outer shadows for frames, shapes, images, and text, with offset, blur, spread, color, opacity, and PNG/SVG export.
- **Images and video** — media import, image fills, non-destructive cropping, and video playback.
- **Local and self-hosted workspaces** — personal and team spaces, document and library synchronization, and version-conflict comparison.
- **Export** — `.rovar` documents, PNG, and SVG.
- **Desktop and Web** — a shared GPUI interface, English and Simplified Chinese; Web support is experimental.

## Planned

- [ ] **Vector composition** — Boolean operations and masks.
- [ ] **Visual effects** — inner shadows, layer and background blur, glass, glow, blend modes, and mixed effect stacks.
- [ ] **Animation** — keyframes, easing, animated gradients, and layer effects.
- [ ] **Interactive presentations** — event triggers, screen navigation, overlays, scrollable areas, state switching, and transitions.
- [ ] **Presentation sharing** — standalone playback and browser sharing through a self-hosted server.
- [ ] **AI-assisted design** — natural-language generation, selection editing, layout refinement, and previews of editable results.
- [ ] **GPUI DSL export** — layouts, styles, components, and assets for GPUI applications.

## Run

```sh
cargo run --locked -p rovar
```

## Build

Install [just](https://github.com/casey/just) and [Nushell](https://www.nushell.sh/). Run `just` to list all build commands.

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
