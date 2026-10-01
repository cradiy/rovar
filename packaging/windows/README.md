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

`--offline` disables Cargo network access. `--keep-work` retains intermediate packaging files. Packaging does not download tools or dependencies.

## Output

- `Rovar-<version>-windows-x64-setup.exe`: per-user installer with a Start menu shortcut and uninstaller.
- `Rovar-<version>-windows-x64.zip`: extract and run `Rovar/rovar.exe`.
- SHA-256 checksums and build metadata.

Output defaults to `dist/`. / 产物默认输出到 `dist/`。

Application payloads contain only `rovar.exe` and `LICENSE`. Runtime DLLs, redistributable installers, media plugins, caches, and development files are not bundled. Packages are unsigned. Windows supplies Media Foundation; the target machine must provide the Microsoft Visual C++ x64 runtime. Both distributions use the normal per-user application data directory; the ZIP does not relocate user data beside the executable. Uninstalling preserves user documents and settings.
