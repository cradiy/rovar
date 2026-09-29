# Rovar

[English](README.md) | 简体中文

使用 Rust 和 GPUI 构建的桌面设计编辑器。支持画板、矢量、文字、图片、视频、组件复用、多页面文档和多文档标签页，设计自动保存到本地工作区。

画板和分组支持自动布局，包括横纵排列、嵌套、间距、内边距、对齐，以及固定、适应内容和填满容器。通过右键菜单或 `Shift+A` 切换；多选对象时先创建分组。

Assets 分为本文档组件和本机资源库。主组件修改同步到各页实例，并保留实例的局部修改；组件及媒体随 `.rovar` 文件保存。本机资源可导入不同文档复用。

支持 PNG、SVG、视频原文件和 `.rovar` 文档导出，提供英文和简体中文界面。

这个类 Figma 项目主要由 GPT-6 Astra 开发，目的是展示我的 GPUI 的能力。目前功能已比较完善，但我不一定会把它做成完整产品。如果想用的人多，或者以后我自己需要，可能会继续完善。

![Forma 数据面板](docs/screenshots/analytics.png)

![Morrow 音乐播放器](docs/screenshots/music.png)

![Sora 界面组件](docs/screenshots/components.png)

## 运行

```sh
cargo run --locked -p rovar
```

## 构建

Linux 下使用 Nushell：

```nu
nu scripts/build.nu
nu scripts/build.nu all
```

支持 AppImage、tar.gz、RPM、DEB 和 Arch，产物输出到 `dist/`。

macOS 下使用 Nushell 和 Xcode Command Line Tools：

```sh
nu scripts/build.nu macos
```

构建 release 版本，生成 `.app`、ZIP 和 DMG。
产物包含 `dist/Rovar.app`、带版本号的压缩包、SHA-256 校验文件和构建信息。
仅支持 Apple Silicon（arm64），也可使用 `--output` 和 `--keep-work`。

详见 [macOS 打包说明](packaging/macos/README.md)。

## Web（实验性）

安装 [Trunk](https://trunkrs.dev/) 和 Rust 的 `wasm32-unknown-unknown` 编译目标后：

```nu
nu scripts/web.nu --serve
nu scripts/web.nu --release
```

需要支持 WebGPU 的浏览器，通过 HTTPS 或 localhost 访问。构建产物：`dist/web/`。
使用 `--serve` 时，先启动监听 `127.0.0.1:8699` 的后端，在独立的开发 TOML 配置中设置 `server.public_origin = "http://127.0.0.1:8080"`，然后用浏览器打开该地址。Trunk 会将 `/api/v1/` 转发给后端；嵌入版服务器使用自身的公开地址。
登录、注册和编辑器共用 GPUI 界面与语言设置。HTML 仅在 WASM 加载时显示启动提示；PNG/SVG 导出和缩略图渲染使用按需加载的独立 WASM 模块。
语言设为“自动”时，按浏览器偏好匹配，无匹配则回退英文。登录背景动效遵循减少动态效果设置，标签页隐藏时暂停。
连接提供编辑器的服务器，文档缓存在 IndexedDB 并同步到服务器，可导出 `.rovar` 文件保留独立副本。同一工作区只允许一个浏览器标签页打开，应用内支持多文档标签页。导入请使用文件选择框。Web 面板使用实色背景，字体随应用提供。

## 服务器

执行 `nu scripts/server.nu` 构建内嵌 Web 编辑器的服务器二进制。桌面端可在本地存储与多个服务器之间切换。详见[服务器配置](crates/rovar-server/README.zh-CN.md)。

[文件格式](crates/rovar-format/FORMAT.zh-CN.md) · [MIT 许可证](LICENSE)
