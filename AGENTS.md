# Repository Instructions

These rules apply throughout the repository.

## Language

- Write all AI-facing instructions, including `AGENTS.md` and other agent guidance files, in English.

## Documentation

- Use README files to introduce the project, its main capabilities, and basic usage. Do not turn them into development logs, change journals, or lists of implementation details.
- Do not copy development discussions, temporary proposals, debugging narratives, user feedback, or conversation history into project documentation.
- Document only final usage instructions or technical contracts that readers need. Do not add documents or sections merely to record a development session.

## Compatibility

- Treat all commits not yet pushed to the remote base branch, together with uncommitted work, as one unpublished development phase. Breaking changes are allowed throughout that phase; local commits do not establish compatibility boundaries.
- Keep the API version exactly one above the remote base branch during the development phase. Do not increment it for each local commit or intermediate protocol change.
- When revising unpublished work, update affected callers, configuration, data structures, and tests directly. Remove superseded implementations instead of retaining compatibility branches or transition layers for intermediate implementations, APIs, configuration, or data formats.

## Code Validation

- After changing code, run `cargo clippy` for the affected crates and build targets, and fix issues introduced by the changes.
- Use `cargo clippy --workspace --all-targets --locked -- -D warnings` by default. Include the corresponding target checks when changing Web/WASM code.
- Do not substitute `cargo check`, successful compilation, or passing tests for Clippy. If environmental constraints prevent a check from completing, report that explicitly; never claim it passed.

## File Editing

- Do not use Python scripts or `cat` to create, overwrite, append to, or bulk-edit project code files.
- Prefer `apply_patch` for file changes. Do not use other scripts or commands to circumvent these editing restrictions.
