use linux.nu [run-tool require-tools]

def installer-compiler [] {
    if ($env.ISCC? | is-not-empty) { return $env.ISCC }
    let available = (which ISCC.exe)
    if not ($available | is-empty) { return ($available | first | get path) }
    let roots = [($env.LOCALAPPDATA? | default ""), ($env.'ProgramFiles(x86)'? | default ""), ($env.ProgramFiles? | default "")]
    for root in ($roots | where {|root| $root != "" }) {
        for directory in ['Programs/Inno Setup 6' 'Inno Setup 6' 'Programs/Inno Setup 7' 'Inno Setup 7'] {
            let compiler = ($root | path join $directory ISCC.exe)
            if ($compiler | path exists) { return $compiler }
        }
    }
    error make {msg: "Inno Setup compiler not found. Install Inno Setup or set ISCC to ISCC.exe. See packaging/windows/README.md."}
}

export def preflight-windows [formats: list<string>] {
    if "zip" in $formats { require-tools [7z] }
    if "setup" in $formats { installer-compiler | ignore }
}

export def package-windows [ctx: record, formats: list<string>, destination: path, keep_work: bool] {
    if $ctx.arch != "x86_64" { error make {msg: "Windows packages require x86_64 MSVC."} }
    let target_directory = ($ctx.target_directory | path expand)
    let work = ($target_directory | path join $"rovar-package-windows-(random uuid)")
    let payload = ($work | path join Rovar)
    mkdir $payload
    try {
        # Distribution policy: stage only the executable and license, never
        # runtime DLLs, redistributable installers, plugins or development files.
        cp $ctx.binary ($payload | path join rovar.exe)
        cp ($ctx.root | path join LICENSE) ($payload | path join LICENSE)
        let contents = (ls $payload | get name | path basename | sort)
        if $contents != [LICENSE rovar.exe] {
            error make {msg: "Unexpected files in Windows package."}
        }
        let stem = $"Rovar-($ctx.version)-windows-x64"
        mkdir $destination
        mut artifacts = []
        for format in $formats {
            let archive = if $format == "zip" {
                let archive = ($work | path join $"($stem).zip")
                do {
                    cd $work
                    run-tool 7z a -tzip -mx=9 $archive Rovar
                }
                run-tool 7z t $archive
                $archive
            } else {
                let name = $"($stem)-setup"
                run-tool (installer-compiler) /Qp $"/DAppVersion=($ctx.version)" $"/DPayloadDir=($payload)" $"/O($work)" $"/F($name)" ($ctx.root | path join packaging windows rovar.iss)
                $work | path join $"($name).exe"
            }
            if not ($archive | path exists) { error make {msg: $"Missing package: ($archive)"} }
            let target = ($destination | path join ($archive | path basename))
            mv -f $archive $target
            let digest = (open --raw $target | hash sha256)
            $"($digest)  ($target | path basename)\n" | save --force $"($target).sha256"
            $artifacts = ($artifacts | append $target)
            print $"Packaged: ($target)"
        }
        {version: $ctx.version, architecture: "x64", profile: $ctx.profile,
            signing: "unsigned", runtime_libraries_bundled: false, artifacts: $artifacts}
            | to json | save --force ($destination | path join $"($stem).json")
    } catch {|error|
        print --stderr $"Packaging failed. Intermediate files: ($work)"
        error make {msg: $error.msg}
    }
    if $keep_work {
        print $"Intermediate files: ($work)"
    } else {
        # Verify the resolved cleanup target stays directly inside Cargo's output.
        let resolved = ($work | path expand)
        if ($resolved | path dirname) != $target_directory or not (($resolved | path basename) | str starts-with "rovar-package-windows-") {
            error make {msg: $"Refusing to remove unexpected packaging directory: ($resolved)"}
        }
        rm -rf $resolved
    }
}
