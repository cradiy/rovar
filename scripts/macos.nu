use linux.nu [run-tool capture require-tools]

export def preflight-macos [formats: list<string>] {
    mut tools = [sips iconutil otool plutil codesign ditto lipo chmod mktemp]
    if "dmg" in $formats { $tools = ($tools | append [hdiutil ln]) }
    require-tools $tools
}

def minimum-system-version [binary: path] {
    let commands = (capture otool -l $binary)
    let modern = ($commands | lines | parse --regex '^\s+minos (?<version>[0-9.]+)' | get version)
    let legacy = ($commands | parse --regex '(?s)cmd LC_VERSION_MIN_MACOSX\s+cmdsize \d+\s+version (?<version>[0-9.]+)' | get version)
    let versions = ($modern | append $legacy)
    if ($versions | is-empty) { error make {msg: $"Cannot read macOS deployment target from ($binary)"} }
    $versions | sort --natural | last
}

def make-icon [ctx: record, work: path, resources: path] {
    let iconset = ($work | path join Rovar.iconset)
    mkdir $iconset
    for size in [16 32 128 256 512] {
        for scale in [1 2] {
            let suffix = if $scale == 2 { "@2x" } else { "" }
            let pixels = ($size * $scale | into string)
            let name = $"icon_($size)x($size)($suffix).png"
            capture sips -z $pixels $pixels ($ctx.root | path join assets rovar-icon.png) --out ($iconset | path join $name) | ignore
        }
    }
    run-tool iconutil -c icns $iconset -o ($resources | path join Rovar.icns)
}

# Distribution policy: never include runtimes, dylibs, frameworks, plugins or scanners.
def verify-contents [app: path] {
    let allowed = [
        Contents/MacOS/rovar
        Contents/Resources/Rovar.icns
        Contents/Resources/LICENSE
        Contents/Info.plist
        Contents/PkgInfo
        Contents/_CodeSignature/CodeResources
    ]
    let actual = (glob ($app | path join '**/*')
        | where {|file| ($file | path type) != dir }
        | each {|file| $file | path relative-to $app } | sort)
    if $actual != ($allowed | sort) {
        error make {msg: $"Unexpected application bundle contents: ($actual | str join ', ')"}
    }
}

def stage-macos [ctx: record, work: path] {
    let app = ($work | path join Rovar.app)
    let contents = ($app | path join Contents)
    let executable = ($contents | path join MacOS rovar)
    let resources = ($contents | path join Resources)
    mkdir ($executable | path dirname) $resources
    cp $ctx.binary $executable
    run-tool chmod 755 $executable
    cp ($ctx.root | path join LICENSE) ($resources | path join LICENSE)
    make-icon $ctx $work $resources
    let version = ($ctx.version | split row '-' | first | split row '+' | first)
    let info = (capture plutil -convert json -o - ($ctx.root | path join packaging macos Info.plist)
        | from json
        | upsert CFBundleShortVersionString $version
        | upsert CFBundleVersion $version
        | upsert LSMinimumSystemVersion (minimum-system-version $ctx.binary))
    $info | to json | save ($work | path join Info.json)
    run-tool plutil -convert xml1 -o ($contents | path join Info.plist) ($work | path join Info.json)
    "APPL????" | save ($contents | path join PkgInfo)
    run-tool plutil -lint ($contents | path join Info.plist)
    # Local ad-hoc signing requires neither certificates nor a network connection.
    run-tool codesign --force --sign - --timestamp=none $app
    run-tool codesign --verify --deep --strict $app
    verify-contents $app
    {app: $app, minimum_macos: $info.LSMinimumSystemVersion}
}

export def package-macos [ctx: record, formats: list<string>, destination: path, keep_work: bool] {
    if $ctx.arch != "aarch64" { error make {msg: "macOS packages support Apple Silicon only."} }
    let arch = "arm64"
    run-tool lipo $ctx.binary -verify_arch $arch
    let work = (capture mktemp -d ($ctx.target_directory | path join rovar-package-macos.XXXXXX))
    try {
        let staged = (stage-macos $ctx $work)
        let app = $staged.app
        let stem = $"Rovar-($ctx.version)-macos-($arch)"
        mkdir $destination
        mut artifacts = []
        for format in $formats {
            let target = if $format == "app" {
                let target = ($destination | path join Rovar.app)
                let kind = ($target | path type)
                if $kind not-in ["" dir] {
                    error make {msg: $"Refusing to replace non-directory app bundle: ($target)"}
                }
                if ($target | path exists) { rm -rf $target }
                run-tool ditto $app $target
                $target
            } else {
                let archive = ($work | path join $"($stem).($format)")
                if $format == "zip" {
                    run-tool ditto -c -k --sequesterRsrc --keepParent $app $archive
                } else {
                    let image = ($work | path join dmg)
                    mkdir $image
                    run-tool ditto $app ($image | path join Rovar.app)
                    run-tool ln -s /Applications ($image | path join Applications)
                    run-tool hdiutil create -volname Rovar -srcfolder $image -format UDZO -ov $archive
                    run-tool hdiutil verify $archive
                }
                let target = ($destination | path join ($archive | path basename))
                mv -f $archive $target
                let digest = (open --raw $target | hash sha256)
                $"($digest)  ($target | path basename)\n" | save --force $"($target).sha256"
                $target
            }
            $artifacts = ($artifacts | append $target)
            print $"Packaged: ($target)"
        }
        {version: $ctx.version, architecture: $arch, profile: $ctx.profile,
            minimum_macos: $staged.minimum_macos, signing: "ad-hoc",
            runtime_libraries_bundled: false, artifacts: $artifacts,
            external_dependencies: (capture otool -L $ctx.binary | lines | skip 1 | each { str trim })}
            | to json | save --force ($destination | path join $"($stem).json")
    } catch {|error|
        print --stderr $"Packaging failed. Intermediate files: ($work)"
        error make {msg: $error.msg}
    }
    if $keep_work { print $"Intermediate files: ($work)" } else { rm -rf $work }
}
