#!/usr/bin/env nu

# Install or update the Rovar design skill in an agent's skills directory.
def main [directory: string = ""] {
    let source = ($env.FILE_PWD | path dirname | path join skills rovar-design SKILL.md)
    let version = (open ($env.FILE_PWD | path dirname | path join Cargo.toml) | get workspace.package.version)
    let root = if ($directory | is-empty) {
        $nu.home-dir | path join .agents skills
    } else {
        $directory
    }
    let destination = ($root | path expand | path join rovar-design)
    mkdir $destination
    open --raw $source | str replace --all "\r\n" "\n" | str replace --all '__ROVAR_VERSION__' $version | save --force ($destination | path join SKILL.md)
    print $"Installed rovar-design for Rovar ($version) to ($destination)"
}
