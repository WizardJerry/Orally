# Orally Product Architecture

[English](architecture.md) | [简体中文](architecture.zh-CN.md)

> Status: Reference
>
> This document describes stable product boundaries and deferred product
> decisions. Repository facts and the proposed migration route live in the
> [Refactor Architecture Baseline](../engineering/refactor-baseline.md).

## Core Value Flow

The product flow is intentionally small:

    trigger or command
      -> audio input
      -> speech recognition
      -> text refinement
      -> text delivery

Local History, Audio Retention, Automatic Stop, Workflow selection, pending
states, and recovery may support that flow. They are not mandatory processing
stages.

## Product Boundaries

- A **driving shell** starts Voice Input, presents state, and delivers Final
  Text through its environment. The Windows Desktop App and CLI are different
  shells with different users and interaction constraints.
- An **audio adapter** supplies bounded audio without deciding how text should
  be refined or delivered.
- A **speech-recognition adapter** turns audio into Raw Transcript and owns the
  external protocol details needed for that request.
- A **refinement policy** selects raw output, AI Post-processing, or Local Basic
  Cleanup and produces Final Text.
- A **delivery adapter** writes or inserts Final Text without owning speech or
  refinement policy.
- Configuration, Local History, pending states, and recovery support the value
  flow without becoming mandatory dependencies of every adapter.

Environment-specific lifecycle belongs in the driving shell. Shared code is
valuable when it hides a real product decision for more than one caller; a
pass-through wrapper is not a product architecture boundary by itself.

## Deferred Design Commitments

Accepted ADRs apply if their capabilities become active:

| Decision | Capability |
| --- | --- |
| [ADR-0002](../adr/0002-compose-modules-into-one-request.md) | Product Modules compose into one AI request |
| [ADR-0003](../adr/0003-share-provider-connections-across-workflows.md) | Workflows share Service Connections |
| [ADR-0004](../adr/0004-keep-portable-data-with-the-application.md) | A full Portable Installation keeps persistent data with the application |
| [ADR-0006](../adr/0006-store-workflows-in-sqlite-and-modules-as-toml.md) | SQLite stores Workflow, connection, credential, and Local History records; product Modules remain TOML |
| [ADR-0007](../adr/0007-stream-long-recordings-through-audio-segments.md) | Long recordings use progressive Audio Segments |

[ADR-0001](../adr/0001-ordered-workflow-stages.md) and
[ADR-0005](../adr/0005-use-hybrid-portable-storage.md) are historical because
they were superseded. Accepted means a decision remains in force if its
capability is implemented; it does not schedule that capability.

## Deferred Behavior Reference

These previously agreed product behaviors are retained for the next relevant
review, not activated by this document:

- a saved Global Trigger is captured directly, checked for conflicts, and
  applied without restarting;
- Automatic Stop is optional and disabled by default;
- First-run Setup validates speech recognition and microphone access before
  offering AI Post-processing setup and enabling the Global Trigger;
- Built-in Workflows are read-only; a user copies one before customizing it;
- Active Workflow can be selected from the tray or settings, and its name is
  visible when selection matters;
- Local History is user controlled and retains a bounded set of Voice Input
  outcomes;
- Audio Retention is disabled by default and follows the lifecycle of its Local
  History entry;
- Pending Voice Input permits speech-recognition retry without speaking again
  and is cleared after success, discard, a new recording, or exit;
- Pending Refinement permits refinement retry from Raw Transcript during the
  current run; cross-run recovery requires the separate Raw Transcript
  retention opt-in;
- when AI Post-processing fails and Local Basic Cleanup succeeds, Final Text
  remains deliverable and the shell indicates that degraded path;
- Insertion Recovery keeps Final Text available on the clipboard and in Local
  History, with actions to retry insertion or open Local History;
- External-request Blocking prevents external providers but does not prevent a
  configured local speech-recognition path;
- a complete Portable Installation does not create a startup entry in the
  Windows MVP.

Every item is rechecked against the current user problem, privacy model, and
accepted ADRs before implementation.

## Platform Direction

Platform shells may eventually include Windows TSF, Android
InputMethodService, a macOS menu-bar integration, or Linux IBus/Fcitx5. Platform
breadth is a deferred possibility, not a product success criterion or an active
milestone.
