#!/usr/bin/env nu

# Build a server binary containing the Web editor.
def main [] {
    cd ($env.FILE_PWD | path dirname)
    ^nu scripts/web.nu --release
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Web build failed."} }
    let args = [build --release --locked -p rovar-server --features embedded-web]
    ^cargo ...$args
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Server build failed."} }
    mkdir dist
    cp crates/rovar-server/rovar-server.example.toml dist/rovar-server.example.toml
    cp target/release/rovar-server dist/rovar-server.new
    mv --force dist/rovar-server.new dist/rovar-server
}
