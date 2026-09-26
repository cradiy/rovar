export def --wrapped run [program: string, ...args: string] {
    let result = (^$program ...$args | complete)
    if not ($result.stdout | is-empty) { print $result.stdout }
    if not ($result.stderr | is-empty) { print --stderr $result.stderr }
    if $result.exit_code != 0 {
        error make {msg: $"($program) failed with exit code ($result.exit_code)"}
    }
}

export def --wrapped capture [program: string, ...args: string] {
    let result = (^$program ...$args | complete)
    if $result.exit_code != 0 {
        error make {msg: $"($program) failed: ($result.stderr)"}
    }
    $result.stdout | str trim
}

export def require-tools [names: list<string>] {
    let missing = ($names | where {|name| (which $name | is-empty) })
    if not ($missing | is-empty) {
        error make {msg: $"Missing build tools: ($missing | str join ', '). See README.md."}
    }
}

export def preflight [formats: list<string>] {
    mut tools = [strip readelf sort mktemp pkg-config resvg desktop-file-validate chmod]
    if "deb" in $formats { $tools = ($tools | append dpkg-deb) }
    if "rpm" in $formats { $tools = ($tools | append rpmbuild) }
    if "arch" in $formats { $tools = ($tools | append [bsdtar gzip zstd]) }
    if "tar.gz" in $formats { $tools = ($tools | append tar) }
    if "appimage" in $formats {
        $tools = ($tools | append [ln ($env.APPIMAGETOOL? | default appimagetool)])
    }
    require-tools $tools
}

export def glibc-version [binary: path] {
    capture readelf --version-info $binary | lines
        | parse --regex 'Name: GLIBC_(?<version>[0-9.]+)'
        | get version | uniq | str join "\n" | ^sort -V | lines | last
}

def tree-size [root: path] {
    glob ($root | path join '**/*') | where {|file| ($file | path type) == file }
        | each {|file| ls $file | get size | first | into int } | math sum
}

export def stage [ctx: record, work: path] {
    let root = ($work | path join payload)
    let bin = ($root | path join usr bin)
    let desktop = ($root | path join usr share applications)
    let svg = ($root | path join usr share icons hicolor scalable apps)
    let png = ($root | path join usr share icons hicolor 512x512 apps)
    let license = ($root | path join usr share licenses rovar)
    mkdir $bin $desktop $svg $png $license
    cp $ctx.binary ($bin | path join rovar)
    run strip --strip-unneeded ($bin | path join rovar)
    cp ($ctx.root | path join packaging linux rovar.desktop) $desktop
    cp ($ctx.root | path join assets rovar-icon.svg) ($svg | path join rovar.svg)
    cp ($ctx.root | path join LICENSE) ($license | path join LICENSE)
    run resvg --skip-system-fonts --width 512 ($svg | path join rovar.svg) ($png | path join rovar.png)
    run desktop-file-validate ($desktop | path join rovar.desktop)
    run chmod -R u=rwX,go=rX $root
    run chmod 755 ($bin | path join rovar)
    $root
}

def appimage-dir [payload: path, work: path] {
    let appdir = ($work | path join Rovar.AppDir)
    cp -r $payload $appdir
    run ln -s usr/bin/rovar ($appdir | path join AppRun)
    run ln -s usr/share/applications/rovar.desktop ($appdir | path join rovar.desktop)
    run ln -s usr/share/icons/hicolor/512x512/apps/rovar.png ($appdir | path join rovar.png)
    run ln -s rovar.png ($appdir | path join .DirIcon)
    run chmod -R u=rwX,go=rX $appdir
    $appdir
}

def deb [ctx: record, payload: path, work: path] {
    let root = ($work | path join deb)
    cp -r $payload $root
    mkdir ($root | path join DEBIAN) ($root | path join usr share doc rovar)
    cp ($ctx.root | path join LICENSE) ($root | path join usr share doc rovar copyright)
    let arch = if $ctx.arch == "x86_64" { "amd64" } else { "arm64" }
    let dependencies = ($ctx.config.dependencies.deb | append [
        $"libc6 \(>= ($ctx.glibc)\)"
        $"libglib2.0-0t64 \(>= ($ctx.glib)\)"
        $"libgstreamer1.0-0 \(>= ($ctx.gst)\)"
        $"libgstreamer-plugins-base1.0-0 \(>= ($ctx.gst)\)"
    ] | str join ', ')
    ["Package: rovar", $"Version: ($ctx.package_version)-($ctx.config.release)",
        "Section: graphics", "Priority: optional", $"Architecture: ($arch)",
        $"Maintainer: ($ctx.maintainer)", $"Installed-Size: ((tree-size $root) / 1024 | math ceil | into int)",
        $"Depends: ($dependencies)", $"Description: ($ctx.description)", ""]
        | str join "\n" | save ($root | path join DEBIAN control)
    let result = ($work | path join $"rovar_($ctx.package_version)-($ctx.config.release)_($arch).deb")
    run dpkg-deb --root-owner-group --build $root $result
    $result
}

def rpm [ctx: record, payload: path, work: path] {
    let top = ($work | path join rpm)
    let root = ($top | path join root)
    mkdir $top
    cp -r $payload $root
    let spec = ($top | path join rovar.spec)
    ["Name: rovar", $"Version: ($ctx.package_version)", $"Release: ($ctx.config.release)",
        $"Summary: ($ctx.description)", $"License: ($ctx.license)", $"Packager: ($ctx.maintainer)",
        $"Requires: ($ctx.config.dependencies.rpm | str join ', ')",
        "", "%description", $ctx.description, "", "%files", "%defattr(-,root,root,-)",
        "/usr/bin/rovar", "/usr/share/applications/rovar.desktop",
        "/usr/share/icons/hicolor/scalable/apps/rovar.svg",
        "/usr/share/icons/hicolor/512x512/apps/rovar.png",
        "%license /usr/share/licenses/rovar/LICENSE", ""]
        | str join "\n" | save $spec
    run rpmbuild -bb --nodeps --target $ctx.arch --buildroot $root --define $"_topdir ($top)" --define $"_tmppath ($top)" --define '_build_id_links none' --define 'debug_package %{nil}' --define 'source_date_epoch_from_changelog 0' $spec
    glob ($top | path join RPMS '**/*.rpm') | first
}

def arch [ctx: record, payload: path, work: path] {
    let root = ($work | path join arch)
    cp -r $payload $root
    let timestamp = ($env.SOURCE_DATE_EPOCH? | default ((date now | into int) // 1_000_000_000) | into string)
    let dependencies = ($ctx.config.dependencies.arch | append [
        $"glibc>=($ctx.glibc)", $"glib2>=($ctx.glib)", $"gstreamer>=($ctx.gst)"])
    ["pkgname = rovar", "pkgbase = rovar", "xdata = pkgtype=pkg",
        $"pkgver = ($ctx.package_version)-($ctx.config.release)", $"pkgdesc = ($ctx.description)",
        "url =", $"builddate = ($timestamp)", $"packager = ($ctx.maintainer)",
        $"size = (tree-size $root)", $"arch = ($ctx.arch)", $"license = ($ctx.license)"]
        | append ($dependencies | each {|dep| $"depend = ($dep)" }) | append ""
        | str join "\n" | save ($root | path join .PKGINFO)
    let result = ($work | path join $"rovar-($ctx.package_version)-($ctx.config.release)-($ctx.arch).pkg.tar.zst")
    cd $root
    let mtree = (^bsdtar --format=mtree --options='!all,use-set,type,uid,gid,mode,time,size,sha256,link'
        --uid 0 --gid 0 -cf - .PKGINFO usr | complete)
    if $mtree.exit_code != 0 { error make {msg: $mtree.stderr} }
    $mtree.stdout | save ($work | path join package.mtree)
    run gzip -n ($work | path join package.mtree)
    mv ($work | path join package.mtree.gz) .MTREE
    run bsdtar --zstd --uid 0 --gid 0 --uname root --gname root -cf $result .PKGINFO .MTREE usr
    $result
}

export def package-linux [format: string, ctx: record, payload: path, work: path] {
    match $format {
        deb => { deb $ctx $payload $work }
        rpm => { rpm $ctx $payload $work }
        arch => { arch $ctx $payload $work }
        "tar.gz" => {
            let directory = ($work | path join $"Rovar-($ctx.version)-linux-($ctx.arch)")
            cp -r ($payload | path join usr) $directory
            let result = ($work | path join $"Rovar-($ctx.version)-linux-($ctx.arch).tar.gz")
            run tar --owner=0 --group=0 --numeric-owner -czf $result -C $work ($directory | path basename)
            $result
        }
        appimage => {
            let appdir = (appimage-dir $payload $work)
            let result = ($work | path join $"Rovar-($ctx.version)-($ctx.arch).AppImage")
            let runtime = $env.APPIMAGE_RUNTIME_FILE?
            let args = if $runtime == null { [--no-appstream $appdir $result] } else {
                [--no-appstream --runtime-file $runtime $appdir $result]
            }
            with-env {APPIMAGE_EXTRACT_AND_RUN: "1", ARCH: $ctx.arch, VERSION: $ctx.version} {
                run ($env.APPIMAGETOOL? | default appimagetool) ...$args
            }
            run chmod 755 $result
            $result
        }
    }
}
