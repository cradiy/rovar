#!/usr/bin/env nu

use linux.nu *
use macos.nu [preflight-macos package-macos]
use windows.nu [preflight-windows package-windows]

# Build Rovar for the current platform. See justfile for the unified build entry.
def main [
    ...formats: string # Linux: appimage, tar.gz, rpm, deb, arch; macOS: app, zip, dmg; Windows: zip, setup; or all
    --profile: string = "release" # Cargo profile (release or dev)
    --offline # Do not let Cargo access the network
    --skip-build # Package the existing binary from the selected Cargo profile
    --output: path # Output directory; defaults to dist/ in the checkout
    --keep-work # Keep intermediate packaging files in target/
] {
    build-rovar $formats $profile $offline $skip_build $output $keep_work
}

# Build and package the Windows release. Omit formats to produce ZIP and installer.
def "main windows" [
    ...formats: string # zip or setup
    --offline # Do not let Cargo access the network
    --skip-build # Package the existing release binary
    --output: path # Output directory; defaults to dist/ in the checkout
    --keep-work # Keep intermediate packaging files in target/
] {
    if $nu.os-info.name != "windows" { error make {msg: "Run Windows builds on Windows."} }
    for format in $formats {
        if $format not-in [zip setup] { error make {msg: $"Unknown Windows package format: ($format)"} }
    }
    let selected = if ($formats | is-empty) { [zip setup] } else { $formats }
    build-rovar $selected release $offline $skip_build $output $keep_work
}

# Build and package the macOS release. Omit formats to produce app, zip and dmg.
def "main macos" [
    ...formats: string # app, zip, or dmg
    --offline # Do not let Cargo access the network
    --skip-build # Package the existing release binary
    --output: path # Output directory; defaults to dist/ in the checkout
    --keep-work # Keep intermediate packaging files in target/
] {
    if $nu.os-info.name != "macos" { error make {msg: "Run macOS packaging on macOS."} }
    for format in $formats {
        if $format not-in [app zip dmg] { error make {msg: $"Unknown macOS package format: ($format)"} }
    }
    let selected = if ($formats | is-empty) { [app zip dmg] } else { $formats }
    build-rovar $selected release $offline $skip_build $output $keep_work
}

def build-rovar [formats: list<string>, profile: string, offline: bool, skip_build: bool, output: any, keep_work: bool] {
    let macos = $nu.os-info.name == "macos"
    let windows = $nu.os-info.name == "windows"
    if $nu.os-info.name not-in [linux macos windows] { error make {msg: "This build entry supports Linux, macOS and Windows."} }
    if $profile not-in [release dev] { error make {msg: "Profile must be release or dev."} }
    let supported = if $windows { [zip setup] } else if $macos { [app zip dmg] } else { [appimage tar.gz rpm deb arch] }
    let selected = if "all" in $formats { $supported } else { $formats | uniq }
    for format in $formats {
        if $format not-in ($supported | append all) { error make {msg: $"Unknown package format: ($format)"} }
    }
    let root = ($env.FILE_PWD | path dirname)
    let destination = if $output == null { $root | path join dist } else { $output | path expand }
    cd $root
    require-tools [cargo rustc]
    if not ($selected | is-empty) {
        if $windows { preflight-windows $selected } else if $macos { preflight-macos $selected } else { preflight $selected }
    }
    let metadata = (capture cargo metadata --no-deps --format-version 1 --offline | from json)
    let package = ($metadata.packages | where name == rovar | first)
    let host = (capture rustc -vV | lines | parse 'host: {host}' | get host | first)
    let supported_host = if $windows { '^x86_64-pc-windows-msvc$' } else if $macos { '^aarch64-apple-darwin$' } else { '^(x86_64|aarch64)-unknown-linux-gnu$' }
    if $host !~ $supported_host {
        let requirement = if $windows { "Use the x86_64-pc-windows-msvc toolchain." } else if $macos { "Use the aarch64-apple-darwin toolchain; macOS packages support Apple Silicon only." } else { "Use x86_64 or aarch64 GNU/Linux." }
        error make {msg: $"Unsupported native build target: ($host). ($requirement)"}
    }
    let arch = ($host | split row '-' | first)
    # An explicit target prevents CARGO_BUILD_TARGET or Cargo config from silently cross-compiling.
    if not $skip_build {
        print $"Building Rovar: ($profile), ($host)"
        mut args = [build --locked -p rovar --bin rovar --profile $profile --target $host]
        if $offline { $args = ($args | append "--offline") }
        run-tool cargo ...$args
    }
    let profile_dir = if $profile == "dev" { "debug" } else { "release" }
    let executable = if $windows { "rovar.exe" } else { "rovar" }
    mut binary = ($metadata.target_directory | path join $host $profile_dir $executable)
    if $macos and $skip_build and not ($binary | path exists) {
        # Also accept the native binary produced by a plain cargo build.
        $binary = ($metadata.target_directory | path join $profile_dir rovar)
    }
    if not ($binary | path exists) { error make {msg: $"Missing ($binary). Run without --skip-build first."} }
    print $"Built: ($binary)"
    if ($selected | is-empty) { return }
    if $windows {
        package-windows {
            root: $root, binary: $binary, arch: $arch, version: $package.version,
            profile: $profile, target_directory: $metadata.target_directory,
        } $selected $destination $keep_work
        return
    }
    if $macos {
        package-macos {
            root: $root, binary: $binary, arch: $arch, version: $package.version,
            profile: $profile, target_directory: $metadata.target_directory,
        } $selected $destination $keep_work
        return
    }
    let config = (open ($root | path join packaging linux config.toml))
    let version = $package.version
    let package_version = ($version | str replace --all '-' '~' | str replace --all '+' '.')
    let context = {
        root: $root, binary: $binary, arch: $arch,
        version: $version, package_version: $package_version,
        maintainer: ($package.authors | str join ", "), license: $package.license,
        description: $package.description, config: $config,
        glibc: (glibc-version $binary),
        gst: (capture pkg-config --modversion gstreamer-1.0),
        glib: (capture pkg-config --modversion glib-2.0),
    }
    let work = (capture mktemp -d ($metadata.target_directory | path join rovar-package.XXXXXX))
    try {
        let payload = (stage $context $work)
        mkdir $destination
        mut artifacts = []
        for format in $selected {
            print $"Packaging: ($format)"
            let artifact = (package-linux $format $context $payload $work)
            let target = ($destination | path join ($artifact | path basename))
            mv -f $artifact $target
            let digest = (open --raw $target | hash sha256)
            $"($digest)  ($target | path basename)\n" | save --force $"($target).sha256"
            $artifacts = ($artifacts | append $target)
            print $"Packaged: ($target)"
        }
        {version: $version, architecture: $arch, profile: $profile, glibc: $context.glibc,
            gstreamer: $context.gst, artifacts: $artifacts}
            | to json | save --force ($destination | path join $"rovar-($version)-($arch).json")
    } catch {|error|
        print --stderr $"Packaging failed. Intermediate files: ($work)"
        error make {msg: $error.msg}
    }
    if $keep_work { print $"Intermediate files: ($work)" } else { rm -rf $work }
}
