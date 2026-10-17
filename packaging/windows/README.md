# Windows packaging / Windows 打包

## Build

Run on x64 Windows with the MSVC Rust toolchain, Visual Studio C++ Build Tools and Windows SDK, just, and Nushell installed. ZIP packaging requires `7z` on PATH. Installer packaging requires [Inno Setup 6.3 or later](https://jrsoftware.org/isdl.php); the script finds standard installation locations, or accepts `ISCC` pointing to `ISCC.exe`.

```nu
just build-windows        # Installer and portable ZIP / 安装包和免安装 ZIP
just build-windows-setup  # Installer only / 仅安装包
just build-windows-zip    # ZIP only / 仅 ZIP
```

All commands build release packages. To repackage an existing release or choose an output directory:

```nu
nu scripts/build.nu windows --skip-build --output dist
```
