---
status: accepted
---

# Compose Modules into one post-processing request

Each Workflow performs one speech-recognition request and at most one AI post-processing request. Rather than invoking a separate model for every refinement step, Orally composes an ordered selection of reusable built-in or user-created Modules into one prompt sent to the Workflow's selected LLM. This replaces the sequential-request design in ADR-0001 to reduce latency and cost while making instructions reusable across scenarios.

## Consequences

Modules can be created, saved, selected, removed, and reordered without duplicating their text across Workflows. Individual Modules do not execute or fail independently; validation and failure handling apply to the composed post-processing request as a whole. Branching, parallel execution, and per-Module model selection are outside this model.

The Windows MVP stores static Module text. Declared parameters and per-Workflow parameter values may be added after the basic composition flow is validated.

Workflows retain live references to Modules rather than snapshots. Editing a user-created Module therefore updates every Workflow that uses it, with affected Workflows shown before saving; built-in Modules are read-only and may be copied.

Composition places non-overridable Orally rules first, then modules in their visible order, and the transcript once at the end. Later modules take precedence over earlier user modules, and the settings interface exposes the final composed prompt for inspection.
