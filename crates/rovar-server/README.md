# Rovar server

English | [简体中文](README.zh-CN.md)

Salvo + SQLx + PostgreSQL, with the Web editor embedded. Document and component snapshots are encrypted at rest; the server holds the decryption key.

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

## Account and sync recovery

Open **Account settings** from the workspace menu. Change your password using the current password, or view and revoke active sessions. Password changes revoke every other session while keeping the current editor connected. Session identifiers are separate from authentication tokens.

Desktop saves server names and supports multiple accounts. Rename a server in the server list; removal requires signing out, closing its documents, and synchronizing pending changes. Removing an address retains document caches for a future reconnect.

Edits are saved locally before upload. Failed uploads retry every 30 seconds, or immediately from the cloud status panel. Pending requests survive restarts and reuse their original idempotency key before uploading newer edits. An expired session pauses uploads; **Sign in again** resumes with the original account without closing the document. **Keep both versions** saves local work as a new document and fetches the server version separately.

Account endpoints (authenticated): `PUT /api/v1/account/password`, `GET /api/v1/account/sessions`, `DELETE /api/v1/account/sessions/{id}`, and `DELETE /api/v1/account/sessions` (other sessions only).

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
