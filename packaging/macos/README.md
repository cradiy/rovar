# macOS packaging / macOS 打包

## Build

Run on Apple Silicon macOS with Rust, just, Nushell and Xcode Command Line Tools installed.

```sh
# Build release and produce all three formats.
just build-macos

# Build only a DMG.
just build-macos-dmg
```

`build-macos` builds all three formats. Use `build-macos-app`, `build-macos-zip`, or `build-macos-dmg` for a single format.
`just` runs from the repository root and uses release builds.
