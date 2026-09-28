# Rovar

English | [简体中文](README.zh-CN.md)

A desktop design editor built with Rust and GPUI. Supports artboards, vectors, text, images, video, reusable components, multi-page documents and multiple document tabs. Designs save automatically to a local workspace.

Auto layout supports nested horizontal and vertical frames and groups, spacing, padding, alignment and fixed, content-sized or fill sizing. Use the context menu or `Shift+A`; multiple selected objects are grouped first.

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

[File format](crates/rovar-format/FORMAT.md) · [MIT License](LICENSE)
