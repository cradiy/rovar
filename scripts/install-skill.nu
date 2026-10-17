#!/usr/bin/env nu

# Install or update the Rovar design skill in an agent's skills directory.
def main [directory: string = ""] {
    let source = ($env.FILE_PWD | path dirname | path join skills rovar-design SKILL.md)
    let root = if ($directory | is-empty) {
        $nu.home-dir | path join .agents skills
    } else {
        $directory
    }
    let destination = ($root | path expand | path join rovar-design)
    mkdir $destination
    cp --force $source ($destination | path join SKILL.md)
    print $"Installed rovar-design to ($destination)"
}
