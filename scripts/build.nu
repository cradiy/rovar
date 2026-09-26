#!/usr/bin/env nu

use linux.nu *

# Build Rovar on Linux. Add one or more formats to package it; "all" builds every format.
# Examples: nu scripts/build.nu; nu scripts/build.nu all --offline
def main [
    ...formats: string # appimage, tar.gz, rpm, deb, arch, or all
    --profile: string = "release" # Cargo profile (release or dev)
    --offline # Do not let Cargo access the network
    --skip-build # Package the existing binary from the selected Cargo profile
    --output: path # Output directory; defaults to dist/ in the checkout
    --keep-work # Keep intermediate packaging files in target/
] {
    if $nu.os-info.name != "linux" { error make {msg: "This build entry currently supports Linux only."} }
    if $profile not-in [release dev] { error make {msg: "Profile must be release or dev."} }
    let supported = [appimage tar.gz rpm deb arch]
    let selected = if "all" in $formats { $supported } else { $formats | uniq }
    for format in $formats {
        if $format not-in ($supported | append all) { error make {msg: $"Unknown package format: ($format)"} }
    }
    let root = ($env.FILE_PWD | path dirname)
    let destination = if $output == null { $root | path join dist } else { $output | path expand }
    cd $root
    require-tools [cargo rustc]
    if not ($selected | is-empty) { preflight $selected }
    let metadata = (capture cargo metadata --no-deps --format-version 1 --offline | from json)
    let package = ($metadata.packages | where name == rovar | first)
    let host = (capture rustc -vV | lines | parse 'host: {host}' | get host | first)
    if $host !~ '^(x86_64|aarch64)-unknown-linux-gnu$' {
        error make {msg: $"Unsupported native build target: ($host). Use x86_64 or aarch64 GNU/Linux."}
    }
    let arch = ($host | split row '-' | first)
    # An explicit target prevents CARGO_BUILD_TARGET or Cargo config from silently cross-compiling.
    if not $skip_build {
        print $"Building Rovar: ($profile), ($host)"
        mut args = [build --locked -p rovar --bin rovar --profile $profile --target $host]
        if $offline { $args = ($args | append "--offline") }
        run cargo ...$args
    }
    let profile_dir = if $profile == "dev" { "debug" } else { "release" }
    let binary = ($metadata.target_directory | path join $host $profile_dir rovar)
    if not ($binary | path exists) { error make {msg: $"Missing ($binary). Run without --skip-build first."} }
    print $"Built: ($binary)"
    if ($selected | is-empty) { return }
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
