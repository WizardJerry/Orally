---
status: superseded by ADR-0006
---

[English](0005-use-hybrid-portable-storage.md) |
[简体中文](0005-use-hybrid-portable-storage.zh-CN.md)

# Use SQLite for private records and TOML for shareable configuration

Orally uses a hybrid portable storage model. Local History and plaintext Provider Credentials live in the portable SQLite database, while global settings, non-secret Service Connection data, Workflows, and Modules use TOML so users can inspect, version, and share configurations. Shareable TOML must never contain API keys.

## Consequences

Service Connection configuration references credentials stored in SQLite. Workflow files can be distributed independently of secrets, but imports must resolve their referenced connections and Modules. Retained audio remains in the portable audio directory and is linked to history records in SQLite.
