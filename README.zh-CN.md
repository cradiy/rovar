# Rovar

[English](README.md) | 简体中文

使用 Rust 和 GPUI 构建的设计编辑器，支持桌面端和 Web。

提供画板、矢量、文字、图片与视频、自动布局、组件复用、多页面文档和 PNG/SVG 导出。可完全本地使用，也可连接服务器，使用个人和团队空间。支持英文与简体中文。

这个类 Figma 项目主要由 GPT-6 Astra 开发，目的是展示我的 GPUI 的能力。目前功能已比较完善，但我不一定会把它做成完整产品。如果想用的人多，或者以后我自己需要，可能会继续完善。

![Forma 数据面板](docs/screenshots/analytics.png)

![Morrow 音乐播放器](docs/screenshots/music.png)

![Sora 界面组件](docs/screenshots/components.png)

## 运行

```sh
cargo run --locked -p rovar
```

## 构建

安装 [just](https://github.com/casey/just) 和 [Nushell](https://www.nushell.sh/)，运行 `just` 查看全部构建命令。

```sh
just build-linux       # 桌面二进制
just build-linux-rpm   # RPM 安装包
just build-macos       # App、ZIP 和 DMG
just build-server      # 内嵌 Web 编辑器的服务器
```

打包产物位于 `dist/`。macOS 构建需要 Apple Silicon 和 Xcode Command Line Tools，详见 [macOS 打包说明](packaging/macos/README.md)。

## Web 与服务器

Web 编辑器目前为实验性功能，使用支持 WebGPU 的浏览器，通过 HTTPS 或 localhost 打开服务器地址。

构建 Web 编辑器需要 [Trunk](https://trunkrs.dev/) 和 Rust 的 `wasm32-unknown-unknown` 编译目标。部署和注册配置详见[服务器说明](crates/rovar-server/README.zh-CN.md)。

[文件格式](crates/rovar-format/FORMAT.zh-CN.md) · [MIT 许可证](LICENSE)
