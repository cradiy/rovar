#!/usr/bin/env nu

# Build the Web app, or serve it at http://127.0.0.1:8080.
def main [
    --serve # Start the development server
    --release # Build optimized WASM
    --offline # Use cached Cargo dependencies
] {
    cd ($env.FILE_PWD | path dirname | path join web)
    hide-env -i NO_COLOR
    if $offline { $env.CARGO_NET_OFFLINE = "true" }
    mut args = [build --locked --skip-version-check]
    if $serve { $args = [serve --locked --skip-version-check] }
    if $release { $args = ($args | append "--release") }
    ^trunk ...$args
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Web build failed."} }
}
