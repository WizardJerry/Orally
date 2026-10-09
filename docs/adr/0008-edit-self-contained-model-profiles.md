---
status: accepted
---

[English](0008-edit-self-contained-model-profiles.md) |
[简体中文](0008-edit-self-contained-model-profiles.zh-CN.md)

# Edit self-contained model profiles

On 2026-10-04 the user requested a single configuration editor combining the
home and pipeline pages, with profile switching, saving, importing, creating,
and separate speech-recognition and post-processing columns. Post-processing
must support additional model nodes and Prompt child nodes.

On 2026-10-05 the user refined the layout to vertically stacked stages and
uniform OpenAI-compatible service fields: model, service URL, plaintext API Key,
and Prompt, with a recognition-protocol selector for ASR.

The user subsequently requested automatic ASR protocol selection and a tray
configuration menu for switching saved profiles, marking the active profile
with a dot.

On 2026-10-06 the user requested a global test that directly records Voice Input
with every unsaved setting, plus a separate ASR recording test instead of
requiring an uploaded audio file.

On 2026-10-08 the user requested OpenAI-compatible services and removal of
provider-specific defaults and routing. After reporting HTTP 404 for a speech
model served through Chat Audio, the user requested a fix. Automatic selection
therefore negotiates endpoint and audio encoding from HTTP responses instead of
inferring a protocol from a service host or model name.

On 2026-10-09 the user requested a simpler Settings page, one executable-local
configuration file, and direct shortcut capture instead of a preset list.

Later on 2026-10-09 the user removed ASR Prompt configuration, reserving
recognition vocabulary for a future dictionary feature, and requested that
configuration help omit development notes. This supersedes the earlier
shared Prompt field in the ASR and post-processing forms.

## Decision

The current Desktop App stores named profiles in the existing `config.toml`.
Each profile owns its ASR configuration and ordered post-processing models.
Each post-processing model owns its endpoint, credentials, model name, base
instructions, template, fallback choice, and ordered Prompt children. These editor Prompt
nodes are local instructions, rather than shared product Modules.

The ASR editor shows service URL, model, and API Key. Post-processing also exposes
Prompt and ordered Prompt children. Help explains the available settings and
actions without development notes. ASR no longer accepts configured Prompt
through the editor, shared runtime, or CLI. Speech commands no longer accept
`--prompt` or read `ORALLY_ASR_PROMPT`; text post-processing Prompt options remain.
Legacy ASR Prompt values are ignored when loading and omitted when saving again.
The dictionary feature is deferred.
API keys are masked by default with a visibility toggle and saved as plaintext.
Desktop recognition and tests use `auto`, with no protocol selector. Config
loading, serialization, and the editor normalize older protocol values to
`auto` while retaining endpoints, models, credentials, and language hints. Automatic
selection first tries multipart `/audio/transcriptions`; only HTTP 404/405
switches to Chat Audio `/chat/completions`. This sends one user `input_audio`
part containing a WAV Data URL, without a `format` field or injected text part;
an existing language hint forms optional system context. Only
HTTP 400/422 then permits trying raw Base64 plus `format: "wav"` and a text
fixed transcription instruction. Other failures, including authentication, rate limits,
server errors, network errors, empty text, and malformed successful responses,
stop recognition. A selected format is reused for later chunks in that provider
instance. CLI overrides can explicitly select multipart or standard raw Base64
Chat Audio. There are no host/model-specific branches or provider SDKs.
Native multipart requests buffer the in-memory WAV form before sending. This
avoids the blocking upload bridge masking an early HTTP rejection as a request
body failure. Non-success responses retain their received HTTP status even if
reading the error body fails; only the status codes above permit negotiation.
Existing language, environment-variable, user-template,
and fallback values remain in the configuration even though the simplified
service form does not edit them.

Settings omits output locale, config-path and Portable controls. `config.toml`
always resides beside the executable. If absent, a valid legacy file is copied
from the former environment/AppData/home locations without deleting its source
or replacing an existing destination. Read and parse errors remain visible.
This replaces the earlier optional config-only Portable path precedence; the
broader future Portable Installation data contract remains separate.

The sole configurable shortcut is the global dictation toggle. Clicking its
control captures a key combination into the existing `hotkey.preset` field.
Saving registers it immediately; registration or persistence failure retains the
previous binding. Capture temporarily releases the binding and restores it on
completion, Escape, or loss of focus. The settings window retains fixed Ctrl+S
for saving, and no longer stores demonstration editing or window shortcuts.

In AI mode, enabled models execute in visible order. The first receives the
Raw Transcript and each later model receives the previous model's output.
Enabled non-empty Prompt children append to their model's system instruction
in visible order. Disabling every model passes through the Raw Transcript.
Raw output bypasses all refinement; local mode uses the existing local cleanup.

Service connection tests send a real request using the current draft's selected
model and credential, with automatic ASR protocol negotiation and no local
fallback. The input/output test dialog defaults to microphone capture followed
by recognition and the full refinement chain, with an ASR-only recording mode
and a text refinement mode. Recording captures the selected profile and global
draft settings at start, without requiring a file upload. Desktop capture reuses
CPAL; browser capture produces PCM16 WAV in memory. Cancellation, dialog closure,
profile changes, and page lifecycle changes release recording resources.
Recording is bounded to two minutes and 15 MiB. The test never persists, inserts,
or writes history. It respects external-request blocking and makes configured
fallback visible as a warning. Browser tests use the same request shapes directly and require CORS;
native tests use the existing adapters. Speech recognition and LLM services may
share an address and credential, but each model must support its own endpoint.
A successful LLM connection test cannot verify the ASR model or endpoint.

This editor supersedes the one-request restriction of
[ADR-0002](0002-compose-modules-into-one-request.md) for these configuration
profiles. Shared Service Connections and SQLite-backed product Workflows from
[ADR-0003](0003-share-provider-connections-across-workflows.md) and
[ADR-0006](0006-store-workflows-in-sqlite-and-modules-as-toml.md) remain deferred;
this smaller editor does not implement those storage or sharing contracts.

## Consequences

Switching profiles retains drafts. Saving persists the profile collection and
copies the selected profile into the existing top-level `asr` and `postprocess`
fields. Global output, audio, hotkey, and privacy settings stay outside profiles.
The tray lists saved profiles using stable IDs and a dot for the active profile.
Switching persists the selected ID and mirrors its service settings immediately;
saving profiles or changed names refreshes the menu. The settings window receives
a selection event, reads the latest saved configuration, and retains dirty profile and global
drafts. Shared configuration I/O serializes tray switches and editor saves.
Old TOML documents without profiles or model arrays retain their existing
behavior. CLI commands retain their existing single-model behavior.

JSON export contains one profile, including any directly configured credentials.
JSON and existing TOML import create a new draft without overwriting the active
profile. The native app parses TOML; browser preview stores its own local drafts.

Each model retains the existing failure contract: configured local fallback
handles request failures, and a later model can continue from that cleaned text.
Missing credentials and adapter construction failures still stop the operation.
Raw Transcript and Final Text remain separate in history.
