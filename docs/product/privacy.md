# Privacy And Permissions

[English](privacy.md) | [简体中文](privacy.zh-CN.md)

> Status: Reference
>
> The completed shared-speech milestone preserved current privacy behavior. It
> introduced no new credential store, Local History model, or sync mechanism.
> Every later milestone remains subject to the review guarantees below.

## Privacy Principle

Users should be able to tell:

- which data leaves the machine;
- which provider receives it;
- which data is retained locally;
- which permission enables each behavior.

Documentation must distinguish current facts from deferred target policy.

## Current Implementation Facts

### External Requests

- Configured remote ASR receives recorded or supplied audio.
- Configured AI Post-processing receives the raw transcript and prompt context.
- The Desktop path checks `privacy.allow_external_requests` after
  recording and before remote processing.
- CLI network commands do not currently enforce that setting.

### Provider Credentials

- The preferred current setup stores an environment-variable name in TOML and
  keeps the secret in the process environment.
- Current configuration also permits a plaintext API key in TOML.
- The settings UI masks key fields, but the persisted TOML value is not
  encrypted.
- `config show` prints a directly stored API key without redaction, and passing
  one through `config set` may retain it in shell history. Configuration output
  must be redacted before it is copied or shared.

The completed shared-speech refactor did not silently change this behavior or
claim stronger protection than currently exists. Any later credential-handling
change requires its own behavior and privacy review.

### JSONL History Prototype

- The current default is `privacy.history_enabled = true`; after a successful
  insertion, the Desktop path appends `history.jsonl` beside the active
  configuration or at a configured path.
- Each current entry stores raw text, final text, a millisecond timestamp, and
  the `desktop` provider marker.
- The file is append-only and has no current query, deletion, retention-count,
  or recovery interface.
- CLI voice commands do not write this JSONL history.
- Set `privacy.history_enabled = false` to disable this retention.

This is a JSONL history prototype, not the complete Local History concept in
[CONTEXT](../../CONTEXT.md).

### Audio

- Recorded audio is held in memory for the active session.
- Remote ASR may receive the audio in requests split to at most 60 seconds.
- Audio is not intentionally retained after the current flow.
- Progressive temporary Audio Segments and Audio Retention are not implemented.

### Clipboard

- Windows insertion temporarily places final text on the clipboard and sends
  synthetic paste input.
- Optional restoration preserves prior text clipboard content.
- Images, files, rich formats, and proof that the target accepted the paste are
  not fully preserved or verified.

## Continuing Refactor Guarantees

The completed shared-speech milestone:

- collected no new data;
- introduced no additional external request;
- left credential precedence and storage unchanged;
- left JSONL history behavior unchanged.

For every later milestone:

- privacy-affecting behavior remains unchanged unless the Active milestone lists
  and receives approval for that change;
- no privacy switch is broadened without a separate behavior review;
- documentation is updated before any privacy behavior changes.

## Deferred Target Policies

The following policies remain deferred:

- External-request Blocking before a Voice Input begins;
- Local History enabled by default with the latest ten Final Text outcomes,
  timestamps, Workflow identity, and processing state;
- a user-selectable larger retention count;
- Raw Transcript retention as a separate opt-in;
- Audio Retention disabled by default and tied to Local History lifecycle;
- Pending Voice Input, Pending Refinement, and Insertion Recovery;
- a full Portable Installation data contract.

[ADR-0006](../adr/0006-store-workflows-in-sqlite-and-modules-as-toml.md)
selects plaintext Provider Credentials in local SQLite if that storage design is
implemented. Changing that storage decision requires a superseding ADR.

Explicit risk acknowledgement remains a separate deferred product policy in
this privacy document; it was not decided by ADR-0006. Application-managed
cross-platform encryption remains a possible later upgrade.

Any future credential store must keep Provider Credentials out of logs, Local
History, and shareable product Module files, and the interface must continue to
mask secret values.

## Permissions

| Permission or capability | Current need | Reason |
| --- | --- | --- |
| Microphone | Required for recording commands and Toggle Mode | Capture speech |
| Network | Required for configured remote ASR and AI refinement | Call user-selected providers |
| Clipboard | Required for the current Windows insertion adapter | Deliver final text |
| Global hotkey | Required for Windows Toggle Mode | Start and stop Voice Input |
| Accessibility or input-method integration | Not used by the current prototype | Possible future direct insertion |
| Start at login | Not part of current scope | No current need |

Each future permission must identify its user-visible benefit and degraded
behavior before implementation.

## Deferred Sync Direction

Folder sync, WebDAV, encrypted bundles, and Local History sync are unscheduled.
They are not part of the current privacy promise or refactor completion
criteria.
