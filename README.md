# Rovar

English | [简体中文](README.zh-CN.md)

A desktop design editor built with Rust and GPUI. Supports artboards, vectors, text, images, video, reusable components and multiple document tabs. Designs save automatically to a local workspace.

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

[File format](crates/rovar-format/FORMAT.md) · [MIT License](LICENSE)
