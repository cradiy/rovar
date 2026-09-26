# macOS packaging / macOS 打包

## Build

Run on Apple Silicon macOS with Rust, Nushell and Xcode Command Line Tools installed.

```sh
# Build release and produce all three formats.
nu scripts/build.nu macos

# Choose individual formats and a destination.
nu scripts/build.nu macos app dmg --output dist/macos
```

`--keep-work` retains intermediate files under `target/`.
Relative output paths are interpreted relative to the caller's working directory.

## Output

- `Rovar.app`
- `Rovar-<version>-macos-arm64.zip`
- `Rovar-<version>-macos-arm64.dmg` with an Applications shortcut
- SHA-256 checksums and build metadata

Output defaults to `dist/`. / 产物默认输出到 `dist/`。
