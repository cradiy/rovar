# Rovar server

English | [简体中文](README.zh-CN.md)

Salvo + SQLx + PostgreSQL, with the Web editor embedded. Document and component snapshots are encrypted at rest; the server holds the decryption key.

## Run

From the repository root, using Nushell:

```nu
nu scripts/server.nu
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

## Spaces

Use the workspace menu to switch between personal and team spaces. Documents, components, revisions and client caches are scoped to each space. Desktop also supports multiple servers and fully local use.

Team owners generate invitation codes and remove members; members can leave. Codes expire after seven days, work once, and are replaced when a new code is generated. Joining an existing team does not require open registration.

Back up PostgreSQL and the storage directory together, including `master.key`. Desktop credentials use the system credential store; Web sessions use HttpOnly cookies. Client caches are not encrypted.

## Structure

- `domain/`: models, validation and errors.
- `application/`: authentication, spaces and documents; repository/storage interfaces.
- `infrastructure/`: PostgreSQL transactions, password hashing and encrypted storage.
- `http/`: Salvo handlers, authentication and DTO mapping.
- `bootstrap/`: TOML configuration and dependency assembly.

DTOs live in `rovar-api`. Saves use expected revisions and idempotency keys; conflicting edits retain the local copy. Snapshots are limited to 128 MiB.

## Test

```nu
cargo test -p rovar-server
$env.ROVAR_TEST_DATABASE_URL = "postgresql://user:password@localhost/disposable_test_db"
cargo test -p rovar-server -- --include-ignored
```

Use a disposable database for integration tests.
