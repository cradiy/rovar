# Rovar

[English](README.md) | 简体中文

使用 Rust 和 GPUI 构建的设计编辑器，支持桌面端和 Web。

提供画板、矢量、文字、图片与视频、自动布局、组件复用、多页面文档和 PNG/SVG 导出。可完全本地使用，也可连接服务器，使用个人和团队空间。支持英文与简体中文。

我希望打造一个美观、跨平台、方便 AI 操作的 UI 设计工具，并通过这个项目的实际需求，持续完善 GPUI 框架的能力。

![Forma 数据面板](docs/screenshots/analytics.png)

![Morrow 音乐播放器](docs/screenshots/music.png)

![Sora 界面组件](docs/screenshots/components.png)

## 功能

- **画布编辑** — 画板、图层、多页面文档、矢量图形、贝塞尔路径与文字。
- **自动布局** — 横向与纵向布局、自动换行、对齐、内边距、间距，以及固定、适应内容、填充尺寸和最小/最大尺寸限制。
- **网格布局** — 固定或均分列宽、行列间距、自动排列，以及跨行跨列。
- **响应式布局** — 边缘固定、居中、拉伸与按比例缩放约束，以及手机、平板、桌面等常用画板尺寸预设。
- **精细操作** — 吸附、距离测量、等距排列、多选缩放、连续复制与画布圆角调节。
- **颜色与渐变** — 文档颜色样式、颜色库，以及线性、径向、角向和菱形渐变。
- **组件复用** — 组件库、组件实例、属性覆盖与更新。
- **组件变体** — 组件集与命名版本，在画布和属性面板中编辑，以及保留对应属性覆盖的实例切换。
- **阴影** — 画板、形状、图片与文字的多层外阴影和内阴影，支持偏移、模糊、扩展、颜色、不透明度及 PNG/SVG 导出。
- **图片与视频** — 媒体导入、图片填充、无损裁剪与视频播放。
- **本地与自托管工作区** — 个人与团队空间、文档及资源库同步、版本冲突对比。
- **导出** — `.rovar` 文档、PNG 与 SVG。
- **桌面端与 Web** — 统一的 GPUI 界面，支持简体中文和英文；Web 端为实验性功能。

## 计划

- [ ] **矢量组合** — 布尔运算与蒙版。
- [ ] **视觉特效** — 图层模糊、背景模糊、玻璃、发光、混合模式与特效叠加。
- [ ] **动画** — 关键帧、缓动、动态渐变与图层特效。
- [ ] **交互演示** — 事件触发、页面跳转、弹层、滚动区域、状态切换与转场。
- [ ] **演示分享** — 独立播放与自托管浏览器分享。
- [ ] **AI 辅助设计** — 自然语言生成、选区修改、布局优化与可编辑结果预览。
- [ ] **GPUI DSL 导出** — 面向 GPUI 应用的布局、样式、组件与资源导出。

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
just build-windows     # 安装包和免安装 ZIP
just build-server      # 内嵌 Web 编辑器的服务器
```

打包产物位于 `dist/`。macOS 构建需要 Apple Silicon 和 Xcode Command Line Tools，详见 [macOS 打包说明](packaging/macos/README.md)。
Windows 打包需要 x64 MSVC、7-Zip 和 Inno Setup，详见 [Windows 打包说明](packaging/windows/README.md)。

## Web 与服务器

Web 编辑器目前为实验性功能，使用支持 WebGPU 的浏览器，通过 HTTPS 或 localhost 打开服务器地址。

构建 Web 编辑器需要 [Trunk](https://trunkrs.dev/) 和 Rust 的 `wasm32-unknown-unknown` 编译目标。部署和注册配置详见[服务器说明](crates/rovar-server/README.zh-CN.md)。

## 开发与贡献

欢迎提交 Issue 和 PR！无论是反馈问题、提出功能建议、讨论设计改进，还是贡献代码与文档，都欢迎参与。

[文件格式](crates/rovar-format/FORMAT.zh-CN.md) · [MIT 许可证](LICENSE)
