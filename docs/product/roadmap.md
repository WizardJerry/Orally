# Orally Roadmap

[English](roadmap.md) | [简体中文](roadmap.zh-CN.md)

> Status: Reference
>
> This file is an index of the working baseline and unscheduled product
> possibilities. The [Refactor Architecture
> Baseline](../engineering/refactor-baseline.md) is the only active execution
> plan.

## Working Prototype

The repository already provides the useful speech-to-polished-text loop through
remote ASR, Local Basic Cleanup or AI Post-processing, CLI tools, and a Windows
Desktop prototype. TOML configuration, JSONL history, and executable-local
configuration exist in limited prototype forms. CLI voice commands and the
Desktop App now share their audio-to-Final-Text processing through
`orally-speech`.

The current objective is to preserve and understand that working slice while
adding later capabilities through reviewed, backend-first milestones. See the
baseline for exact current behavior, the implemented shared-speech route, and
the next Active step; it is not repeated here.

## Unscheduled Product Possibilities

- Workflow, product Module, and Active Workflow;
- Service Connection and Default Service Slot;
- SQLite product storage and credential migration;
- complete Portable Installation;
- full Local History, Audio Retention, and recovery;
- progressive long-recording Audio Segments;
- First-run Setup and arbitrary Global Trigger capture;
- Windows TSF;
- Android, macOS, and Linux shells;
- local ASR;
- personal dictionary;
- folder or WebDAV sync;
- provider plugins;
- encrypted backup.

This list records possibility, not priority or commitment. GitHub Issues may
describe concrete bugs or proposals, but an issue becomes scheduled only when
an approved active baseline places it in scope.

## Entry Rule

Before any item becomes active, document the user problem, explain why the
working slice is insufficient, choose the smallest useful behavior, list
non-goals, reconcile product and privacy documents with relevant ADRs, and stop
for user review. The active baseline owns the detailed gate.
