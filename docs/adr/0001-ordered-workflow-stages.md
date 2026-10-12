---
status: superseded by ADR-0002
---

[English](0001-ordered-workflow-stages.md) |
[简体中文](0001-ordered-workflow-stages.zh-CN.md)

# Workflows use ordered processing stages

Each Workflow contains exactly one speech-recognition stage followed by zero or more user-ordered text-refinement stages. Orally uses a linear chain rather than branching, parallel, or merged execution so configuration, progress, history, and retry behavior remain predictable while still supporting scenario-specific multi-step processing.

## Consequences

Users can add, remove, enable, disable, and reorder text-refinement stages, and each stage selects its own model and instructions. A required stage pauses insertion when it fails; a skippable stage passes through the previous successful text with a warning. Branching and parallel graphs are outside the Windows MVP.
