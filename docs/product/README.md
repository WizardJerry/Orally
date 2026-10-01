# Orally Product Definition

[English](README.md) | [简体中文](README.zh-CN.md)

> Status: Reference
>
> Active engineering scope:
> [Refactor Architecture Baseline](../engineering/refactor-baseline.md)

## Product Problem

Orally helps a person speak naturally and receive polished text in the place
where they are already writing.

The useful product loop is intentionally small:

1. start a Voice Input;
2. capture or provide audio;
3. transcribe speech;
4. refine the raw transcript;
5. deliver the final text.

## Current Useful Scope

The existing prototype already satisfies the current basic need:

- OpenAI-compatible speech recognition;
- Local Basic Cleanup or OpenAI-compatible AI Post-processing;
- shared audio-to-Final-Text orchestration used by CLI voice commands and the
  Windows Desktop App;
- CLI commands for recording, transcription, processing, and dictation;
- a Windows Desktop App with Toggle Mode, tray controls, an overlay, and
  clipboard insertion;
- local TOML configuration;
- a JSONL history prototype in the Desktop path.

The current goal is to preserve and understand this slice while expanding only
through reviewed, minimal milestones. It is not to implement every deferred
possibility at once.

## Product Roles

- **Windows Desktop App** is the end-user product.
- **CLI** is a developer-facing tool and was the first driving adapter for the
  completed shared-speech migration.
- **Windows IME** is deferred research, not the current product.

Starting the shared-speech migration with the CLI changed implementation order,
not product positioning.

## Product Principles

- **Useful first**: preserve the working speech-to-polished-text loop.
- **Small steps**: prefer one understandable improvement over a complete
  framework.
- **User controlled**: provider endpoints, models, prompts, and keys remain
  explicit configuration.
- **Privacy visible**: document what leaves the machine and what is retained.
- **Native where needed**: platform-specific triggering and insertion remain
  adapters around shared behavior.
- **Learning over breadth**: architecture is introduced when current behavior
  justifies it.

## Active Development Intent

The active baseline defines the behavior-preserving shared-speech refactor,
records its implemented stop point, and owns the continuing backend-first
sequence. Its stages, affected files, and next review belong only in that
baseline. This product definition does not independently authorize further
implementation.

## Deferred Product Breadth

The [roadmap](roadmap.md) indexes possible future capabilities. They are not
missing acceptance criteria, and only an approved active baseline may schedule
one of them.

## Before Product Growth

Before implementing any new capability, review:

1. this product definition;
2. [Privacy And Permissions](privacy.md);
3. the relevant terms in [CONTEXT](../../CONTEXT.md);
4. related accepted and superseded ADRs;
5. the active refactor baseline.

Implementation begins only after those documents agree on the smallest useful
scope and explicit non-goals.
