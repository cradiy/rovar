# Rovar

English | [简体中文](README.zh-CN.md)

A design editor built with Rust and GPUI, available on desktop and the Web.

Supports artboards, vectors, text, images and video, auto layout, reusable components, multi-page documents, and PNG/SVG export. Use it locally or connect to a server with personal and team workspaces. Available in English and Simplified Chinese.

Primarily developed by GPT-6 Astra, Rovar is a Figma-like project I created to showcase what my GPUI can do. It already has a fairly broad feature set, though a complete product is not the main goal. I may keep developing it if enough people want to use it, or if I need it myself.

![Forma analytics dashboard](docs/screenshots/analytics.png)

![Morrow music player](docs/screenshots/music.png)

![Sora interface components](docs/screenshots/components.png)

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

[File format](crates/rovar-format/FORMAT.md) · [MIT License](LICENSE)
