# Rovar server

English | [简体中文](README.zh-CN.md)

Hosts the Web editor and provides accounts, personal and team workspaces, and document synchronization. Requires PostgreSQL. Stored documents are encrypted with a server-held key.

## Run

From the repository root, with just and Nushell installed:

```nu
just build-server
cp dist/rovar-server.example.toml rovar-server.toml
# Set the PostgreSQL URL, public origin and storage directory.
./dist/rovar-server --config rovar-server.toml
```

Storage paths are relative to the configuration file. For remote access, use an HTTPS reverse proxy and set `server.public_origin` to the browser origin.

`registration.personal` enables personal signup. `registration.teams` enables team signup and team creation by existing accounts. Both default to `false`. Team signup creates an account, a personal space and a team.

Administrators can provision a personal account regardless of these switches:

```nu
(input --suppress-output "Password: ") | ./dist/rovar-server --config rovar-server.toml --create-user cradiy
```

Passwords require at least 6 characters.

## Backup

Back up PostgreSQL and the storage directory together, including `master.key`. The key is required to decrypt stored documents.
