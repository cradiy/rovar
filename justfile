set windows-shell := ["nu", "-c"]

# Show available commands.
default:
    @just --list

# Build the Linux release binary.
build-linux:
    nu scripts/build.nu

# Package all Windows formats.
build-windows:
    nu scripts/build.nu windows

build-windows-zip:
    nu scripts/build.nu windows zip

build-windows-setup:
    nu scripts/build.nu windows setup

# Package all Linux formats.
build-linux-all:
    nu scripts/build.nu all

build-linux-appimage:
    nu scripts/build.nu appimage

build-linux-tar:
    nu scripts/build.nu tar.gz

build-linux-rpm:
    nu scripts/build.nu rpm

build-linux-deb:
    nu scripts/build.nu deb

build-linux-arch:
    nu scripts/build.nu arch

# Package all macOS formats.
build-macos:
    nu scripts/build.nu macos

build-macos-app:
    nu scripts/build.nu macos app

build-macos-zip:
    nu scripts/build.nu macos zip

build-macos-dmg:
    nu scripts/build.nu macos dmg

# Build the Web release.
build-web:
    nu scripts/web.nu --release

# Build the server with the Web editor embedded.
build-server:
    nu scripts/server.nu

# Start the Web development server.
serve:
    nu scripts/web.nu --serve
