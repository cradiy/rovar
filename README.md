# Rovar

English | [简体中文](README.zh-CN.md)

A desktop design editor built with Rust and GPUI. Supports artboards, vectors, text, images, video, reusable components, multi-page documents and multiple document tabs. Designs save automatically to a local workspace.

Auto layout supports nested horizontal and vertical frames and groups, spacing, padding, alignment and fixed, content-sized or fill sizing. Use the context menu or `Shift+A`; multiple selected objects are grouped first.

Assets separates document components from the local library. Main components update instances across pages while preserving local overrides; components and their media travel with the `.rovar` file. Import local assets for reuse across documents.

Export selections as PNG or SVG, videos in their original format, and documents as `.rovar` files. Available in English and Simplified Chinese.

Primarily developed by GPT-6 Astra, Rovar is a Figma-like project I created to showcase what my GPUI can do. It already has a fairly broad feature set, though a complete product is not the main goal. I may keep developing it if enough people want to use it, or if I need it myself.

![Forma analytics dashboard](docs/screenshots/analytics.png)

![Morrow music player](docs/screenshots/music.png)

![Sora interface components](docs/screenshots/components.png)

## Run

```sh
cargo run --locked -p rovar
```

## Build

Linux, using Nushell:

```nu
nu scripts/build.nu
nu scripts/build.nu all
```

Packages: AppImage, tar.gz, RPM, DEB and Arch. Output: `dist/`.

macOS, using Nushell and Xcode Command Line Tools:

```sh
nu scripts/build.nu macos
```

Builds release packages in `.app`, ZIP and DMG formats.
Output: `dist/Rovar.app` and versioned archives, SHA-256 checksums and build metadata.
Supports Apple Silicon (arm64) only. `--output` and `--keep-work` are also supported.

See [macOS packaging](packaging/macos/README.md) for details.

## Web (experimental)

Install [Trunk](https://trunkrs.dev/) and the Rust `wasm32-unknown-unknown` target, then:

```nu
nu scripts/web.nu --serve
nu scripts/web.nu --release
```

Requires WebGPU on HTTPS or localhost. Build output: `dist/web/`.
For `--serve`, start the backend on `127.0.0.1:8699` with `server.public_origin = "http://127.0.0.1:8080"`, then open that browser origin. Trunk forwards `/api/v1/` to the backend; use a separate development TOML configuration. The embedded server uses its own public origin instead.
Sign-in, registration, and the editor share the GPUI interface and language setting. HTML only displays startup progress while WASM loads; PNG/SVG export and thumbnail rendering use a separate WASM module loaded on demand.
Auto language follows the browser's preferred languages and falls back to English. Login background motion respects reduced-motion preferences and pauses in hidden tabs.
Connect to the server hosting the editor. Documents are cached in IndexedDB and synchronized to the server; export `.rovar` files to keep independent copies. One browser tab owns the workspace; multiple document tabs work inside it. Use file dialogs for imports. Web panels use solid backgrounds, and fonts are bundled.

## Server

Run `nu scripts/server.nu` to build a server binary with the Web editor embedded. Desktop can switch between local storage and multiple servers. See [server setup](crates/rovar-server/README.md).

[File format](crates/rovar-format/FORMAT.md) · [MIT License](LICENSE)
