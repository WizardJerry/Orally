# Orally Product

[English](CONTEXT.md) | [简体中文](CONTEXT.zh-CN.md)

Orally turns natural speech into polished text for the user's current writing
context. This file defines product language only; implementation status,
storage choices, defaults, and project sequence belong in other documents.

## Product Surfaces

**Windows Desktop App**:
The primary end-user product for triggering Voice Input, observing its state,
and configuring behavior on Windows.
_Avoid_: Windows IME, CLI, prototype runner

**Portable Installation**:
A Windows Desktop App distribution whose persistent Orally data travels with
the application directory when it is copied.
_Avoid_: config-only portable mode

**CLI**:
A developer-facing tool for exercising, inspecting, and troubleshooting Orally
outside the end-user product.
_Avoid_: desktop app, end-user interface

**Windows IME**:
A possible native Windows input-method integration.
_Avoid_: Windows Desktop App

## Voice Input

**Voice Input**:
One attempt to turn supplied or recorded speech into delivered, recoverable, or
discarded text.
_Avoid_: recording, transcription

**Toggle Mode**:
A recording interaction in which one trigger starts Voice Input and the next
trigger ends recording.
_Avoid_: hold-to-talk, fixed-duration recording

**Global Trigger**:
The Windows key combination assigned to start and stop Toggle Mode.
_Avoid_: hotkey preset

**Automatic Stop**:
An optional recording behavior that ends Voice Input after sustained silence.
_Avoid_: default stop behavior

**Direct Insertion**:
Delivery of Final Text into the writing target that was active when Voice Input
started.
_Avoid_: preview-first output, transcript-only output

**Insertion Recovery**:
The fallback state after Direct Insertion fails, preserving Final Text for
another delivery attempt.
_Avoid_: successful insertion, transcript retry

**Status Overlay**:
A compact indicator for Voice Input state and available actions that does not
show Raw Transcript or Final Text.
_Avoid_: result preview, history view

**Pending Voice Input**:
Completed audio retained temporarily so speech recognition can be retried
without speaking again.
_Avoid_: Local History, Audio Retention

**Pending Refinement**:
A Raw Transcript retained temporarily so text refinement can be retried.
_Avoid_: Final Text, Local History entry

## Speech And Text

**Raw Transcript**:
Text produced by speech recognition before refinement.
_Avoid_: Final Text

**Final Text**:
The text selected for delivery after raw output, AI Post-processing, or Local
Basic Cleanup.
_Avoid_: Raw Transcript

**AI Post-processing**:
The preferred refinement path that turns Raw Transcript into paste-ready Final
Text while preserving the speaker's meaning.
_Avoid_: speech recognition, Local Basic Cleanup

**Local Basic Cleanup**:
The local refinement path used when AI Post-processing is not selected,
unavailable, disallowed, or unsuccessful.
_Avoid_: AI Post-processing

## Data And Privacy

**Config**:
A software-level setting that controls application behavior.
_Avoid_: Workflow, Module

**Provider Credential**:
A secret used to access an external speech-recognition or AI provider.
_Avoid_: provider preset, account

**External-request Blocking**:
A privacy control that prevents processing through external providers.
_Avoid_: offline transcription

**Local History**:
A user-controlled local record of recent Voice Input outcomes.
_Avoid_: audio archive, cloud history

**Audio Retention**:
Persistence of Voice Input audio as an attachment to its Local History entry.
_Avoid_: temporary processing audio

**Audio Segment**:
A bounded temporary part of recorded audio used to process a longer Voice
Input.
_Avoid_: retained recording, transcript segment

**First-run Setup**:
Guided configuration required before an unconfigured Windows Desktop App can
start Voice Input.
_Avoid_: settings screen, normal startup

## Workflow Language

**Workflow**:
A named, user-adjustable definition of the complete Voice Input process for a
scenario.
_Avoid_: Config, document, preset, voice profile

**Built-in Workflow**:
A read-only Workflow supplied and updated by Orally.
_Avoid_: built-in Module, Custom Workflow

**Custom Workflow**:
A user-created Workflow that can be adjusted and deleted.
_Avoid_: Built-in Workflow, Module

**Speech-recognition Stage**:
The required first stage of a Workflow that turns audio into Raw Transcript.
_Avoid_: text refinement, AI Post-processing

**Module**:
A reusable static Prompt instruction referenced by a Workflow and included in
its Composed Prompt.
_Avoid_: node, Prompt Module, LLM stage, model, request

**Module Identifier**:
The immutable canonical identity of a Module, distinct from its editable
display name.
_Avoid_: display name, Workflow name

**Composed Prompt**:
The single refinement prompt assembled from Orally rules, ordered Modules, and
one Raw Transcript.
_Avoid_: Module, Raw Transcript

**Output-language Module**:
A Module that selects a target natural language while preserving
non-translatable identifiers.
_Avoid_: language detection, ASR language hint

**Service Connection**:
A reusable external-provider connection containing an endpoint, protocol, and
Provider Credential reference.
_Avoid_: Workflow, model, Module

**Default Service Slot**:
A global binding from a default speech-recognition or refinement role to a
Service Connection and model.
_Avoid_: Service Connection, Workflow

**Active Workflow**:
The Workflow selected for the next Voice Input.
_Avoid_: default provider, global Config
