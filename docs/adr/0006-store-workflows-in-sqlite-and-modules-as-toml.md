---
status: accepted
---

# Store complete Workflows in SQLite and Modules as TOML

Orally stores complete Workflow structures, Service Connections, plaintext Provider Credentials, and Local History in the portable SQLite database. Module content remains independently shareable: each Module is one TOML file, built-in Modules live under read-only `module/builtin/`, and custom Modules live under editable `module/custom/`. Workflows retain live references to those Modules. Global process, window, and hotkey Config remains in `config.toml`, while retained recordings live under `audio/`.

## Consequences

Sharing a Module does not share an entire Workflow, Service Connection, or Provider Credential; the recipient assembles their own Workflow. Editing a custom Module updates every referencing Workflow. Built-in Modules are read-only, may be copied under a new immutable lowercase ASCII kebab-case identifier, and cannot be shadowed by a custom Module. Missing or invalid Module files can make Workflows incomplete, so Orally must validate references before recording and must not include API keys in Module files.
