# Refactor Architecture Baseline

[English](refactor-baseline.md) |
[简体中文](refactor-baseline.zh-CN.md)

> Status: Active
>
> Baseline branch: `refactor`
>
> Baseline commit: `c452065`
>
> Baseline date: 2026-08-30
>
> Purpose: shared speech-processing foundation with gated backend-first milestones

## 1. Purpose

Orally already provides the capability that matters most for current use:
speech can be sent to an OpenAI-compatible transcription provider, refined
locally or by AI Post-processing, and delivered as usable text. The Windows
Desktop App also supports Toggle Mode, a tray process, an overlay, and clipboard
insertion.

This refactor is not an attempt to turn the prototype into a comprehensive
commercial product. It is a learning route for improving responsibility
placement, code depth, dependency direction, and maintainability while keeping
the useful behavior available.

The first implementation route starts with the shared audio-to-Final-Text seam,
then migrates the CLI before the Desktop App. This changes implementation order,
not product positioning: the Windows Desktop App remains the end-user product,
and the CLI remains a developer-facing tool.

## 2. Scope And Authority

This document is the only execution plan for the current refactor. It owns:

- the implementation snapshot that structural work must preserve;
- the active stages and their order;
- the files each stage may affect;
- explicit non-goals, verification, rollback, and stop points;
- the documentation review required before behavior or feature work.

On 2026-08-30 the user explicitly requested a shared crate for the existing ASR
and post-processing paths. That direction activated the shared-speech milestone
below and superseded the earlier CLI-only proposal.

After reviewing that result, the user established a continuing backend-first
goal: implement documented product capabilities on `refactor` as small,
test-backed milestones, keep the UI simple until later requirements arrive, and
maintain user-facing and decision documents in English and Simplified Chinese.
That program-level direction authorizes planning the whole route, but each
Reference or Deferred capability still passes the Feature Entry Gate before its
implementation begins.

Use the [documentation map](../README.md) to answer other kinds of questions.
In particular:

- code and existing tests are the authority for current behavior;
- [CONTEXT](../../CONTEXT.md) supplies product vocabulary;
- accepted [ADRs](../adr/) preserve difficult decisions;
- product documents describe stable intent and deferred possibilities;
- GitHub Issues track candidate bugs and work, but do not authorize or schedule
  them by themselves.

An accepted ADR remains valid when its capability is deferred. Deferral is not
rejection. A later decision that contradicts an ADR must supersede it
explicitly.

## 3. Current Useful Slice

The repository contains two related but behaviorally different flows.

### 3.1 Windows Desktop App

The Desktop App is the current daily-use reference implementation:

    Global Trigger or tray action
      -> Toggle recording
      -> complete PCM16 audio held in memory
      -> External-request Blocking check
      -> orally-speech: remote ASR, internally split into requests of at most 60 seconds
      -> orally-speech: raw output, configured Local Basic Cleanup, or AI Post-processing
      -> restore the original foreground window
      -> clipboard paste
      -> optional JSONL history append after successful insertion

Important details:

- `output.raw` bypasses both refinement paths.
- `postprocess.mode` selects AI Post-processing only in the Desktop flow.
- `postprocess.fallback_to_builtin` applies only when
  `OpenAiChatPostprocessor::process` returns an error. A missing credential or
  postprocessor construction error occurs before that fallback and fails the
  Voice Input.
- The JSONL history append is optional. A history-write failure is reported
  after insertion and does not undo the insertion.
- The Tauri entry point owns recording, privacy, insertion, and history policy;
  `orally-speech` owns the shared audio-to-Final-Text portion. This flow does not
  use `orally_core::OrallyPipeline`.

### 3.2 CLI Commands

The current CLI behavior is the compatibility baseline for the initial stages:

| Command | Current behavior |
| --- | --- |
| `demo` | Demo text ASR, `BuiltInTextProcessor`, terminal output |
| `process` | `BuiltInTextProcessor` by default; LLM only with explicit `--ai` |
| `record` | CLI-only fixed-duration microphone recording written as WAV |
| `transcribe` | WAV, remote ASR, raw or `BuiltInTextProcessor`, output or optional insertion |
| `dictate` | Fixed-duration recording, remote ASR, raw or `BuiltInTextProcessor`, output or optional insertion |
| `listen` | Windows Toggle Mode, remote ASR, raw or `BuiltInTextProcessor`, forced insertion |
| `config` | TOML path, show, init, and set operations |

Compatibility details that the shared-speech milestone must not normalize or
repair:

- `transcribe`, `dictate`, and `listen` do not select AI Post-processing from
  `postprocess.mode`, do not enforce `privacy.allow_external_requests`, and do
  not write JSONL history.
- `listen` reuses `DictateOptions`, then forces `insert = true`. It accepts and
  parses `seconds`, but Toggle recording does not use that value.
- `listen` always registers `Ctrl+Alt+Space`; it does not read `hotkey.preset`.
- `record` does not load `AppConfig`, `audio.record_output`, or
  `audio.dictate_seconds`. Its own defaults are three seconds and
  `orally-recording.wav`.
- `process --ai` does not use `postprocess.fallback_to_builtin`. Unless explicit
  prompt flags are supplied, it uses prompts selected by its cleanup, outline,
  or translate task rather than `postprocess.system_prompt` and
  `postprocess.user_template`.
- `process` has its own locale and show-changes defaults rather than using
  `OutputConfig`.
- `process --ai`, like the voice commands, does not enforce
  `privacy.allow_external_requests`.
- CLI output options have no environment-variable layer.
- `demo` ignores trailing arguments, so `demo --help` still runs the demo and
  exits `0`. `config path` also ignores trailing arguments.
- The hard-coded cleanup dictionary, process task aliases, request endpoints,
  request shapes, prompts, and timeouts are all compatibility facts for this
  structural route.
- `process --ai` tasks are developer operations, not Workflow or product Module
  implementations.

These quirks may deserve later changes. The current route preserves them until
a separate behavior review says otherwise.

### 3.3 Current Exit Contract

The CLI migration must preserve this process contract:

| Situation | Current result |
| --- | --- |
| No command or unknown top-level command | Print global help to stdout; exit `0` |
| Successful command | Exit `0` |
| Argument rejected by a command parser, including most command-level `--help` flags | Print an error or usage; exit `2` |
| Config, file I/O, audio, network, processing, or insertion failure | Print to stderr; exit `1` |

The exact text and stdout/stderr destination are part of the compatibility
snapshot even where the current interface is unconventional.

## 4. Current Repository Architecture

The Rust dependency direction after the shared-speech migration is:

    orally-cli ---------\
                         -> orally-speech -> orally-asr -> orally-audio + orally-core
    orally-desktop -----/                \-> orally-llm -> orally-core
                                        \-> orally-core

    orally-cli also -> orally-audio + orally-config + orally-core
                    -> orally-llm (text-only process --ai)
                    -> orally-windows
    orally-desktop also -> orally-audio + orally-config + orally-core
                        -> orally-storage + orally-windows

| Code area | Current role | Baseline assessment |
| --- | --- | --- |
| `orally-core` | Shared types and interfaces, Local Basic Cleanup, demo adapters, legacy demo pipeline | Stable foundation; the old pipeline remains demo-only |
| `orally-audio` | CPAL capture, PCM16, metrics, WAV, in-memory splitting | Useful adapter implementation |
| `orally-asr` | Multipart and chat-audio remote ASR implementations | Relatively deep crate |
| `orally-llm` | OpenAI-compatible text refinement | Useful external adapter |
| `orally-speech` | Typed runtime plan, ASR construction, and Raw/Local/AI refinement orchestration | Shared deep seam used by CLI and Desktop |
| `orally-config` | TOML schema, presets, paths, serialization, string updates | Preserved during the shared-speech milestone |
| `orally-storage` | Append-only JSONL history prototype | Desktop-only prototype, not target Local History |
| `orally-windows` | Win32 trigger and clipboard insertion | Platform adapter |
| `orally-cli` | Parsing, configuration resolution, recording, terminal output, and optional insertion | Developer adapter over the shared speech seam |
| `orally-desktop` | Tauri shell, recording/privacy policy, insertion, history, and UI | End-user adapter over the shared speech seam |

`OrallyPipeline` is a shallow, demo-only source module: its interface exposes
several inputs while its implementation only forwards through ASR, processing,
and insertion. `orally-speech` is the shared production seam, but deleting the
old demo pipeline is still a separate cleanup decision.

Desktop captures the dictation shortcut directly and saves it in
`hotkey.preset`; saving immediately updates its Windows registration. The
browser preview uses its own local configuration for UI verification.

## 5. Capability Status

| Capability | Status |
| --- | --- |
| CPAL microphone recording and WAV support | Implemented |
| Multipart and chat-audio remote ASR | Implemented |
| Local Basic Cleanup through `BuiltInTextProcessor` | Implemented |
| OpenAI-compatible AI Post-processing | Implemented |
| Shared ASR and refinement orchestration for CLI and Desktop | Implemented |
| Windows Toggle Mode, tray, overlay, clipboard insertion | Implemented prototype |
| Config beside the executable | Implemented config-local prototype |
| JSONL history append | Implemented Desktop prototype |
| VAD-driven stopping | Not implemented; metrics only |
| Progressive Audio Segment recording | Not implemented |
| Workflow, product Module, Service Connection | Not implemented |
| SQLite product storage | Not implemented |
| Full Portable Installation contract | Not implemented |
| Pending states and Insertion Recovery | Not implemented |
| Windows TSF input method | Scaffold only |
| Android, macOS, Linux shells | Not implemented |

The open [long-recording issue #1](https://github.com/WizardJerry/Orally/issues/1)
is related to
[ADR-0007](../adr/0007-stream-long-recordings-through-audio-segments.md).
Current ASR request splitting is not progressive Audio Segment recording: the
complete recording is retained in memory before the requests are created.

### 5.1 Current Facts Versus Deferred Product Terms

| Area | Current fact | Product or deferred direction |
| --- | --- | --- |
| Portable data | Only executable-local `config.toml` is supported | Portable Installation carries all persistent product data |
| History | Desktop can append raw and final text to JSONL | Local History has reviewed retention, opt-in Raw Transcript, states, and recovery |
| Long recordings | Complete audio is held in memory, then ASR requests are split | Audio Segments are written progressively and cleaned or retained deliberately |
| Refinement | CLI voice commands use raw or `BuiltInTextProcessor`; AI is explicit in `process --ai` | Product language identifies AI Post-processing as preferred; a separate review decides whether and when the developer-facing CLI adopts that policy |
| Trigger | Desktop captures and registers configurable shortcuts immediately; `listen` hard-codes one shortcut | CLI shortcut unification remains deferred |
| Provider configuration | Flat TOML sections and optional environment variables | Service Connections and Default Service Slots are deferred |

[CONTEXT](../../CONTEXT.md) defines vocabulary, not implementation completion.
Accepted ADRs define decisions, not current milestones.

### 5.2 Verification At The Baseline Commit

At `c452065`:

- all 40 Rust workspace tests pass;
- `cargo fmt --all -- --check` passes;
- strict Clippy reports `derivable_impls` in `orally-config`;
- strict Clippy reports `items_after_test_module` in `orally-cli`;
- strict Clippy reports `enum_variant_names` in `orally-desktop`;
- the Desktop Rust target has no behavior tests;
- the frontend has no test, lint, typecheck, or end-to-end script.

These lint findings are recorded debt outside the shared-speech scope. The
broader testing skeleton is intentionally deferred; this milestone uses focused
crate tests, existing tests, and manual smoke checks.

### 5.3 Verification At The Shared-speech Stop

At the S4 stop on 2026-08-30, before D0 added the bilingual document pairs:

- all 53 Rust workspace tests pass, including 13 focused `orally-speech` tests;
- `cargo fmt --all -- --check` passes;
- strict Clippy passes for `orally-speech`, `orally-core`, `orally-asr`, and
  `orally-llm`;
- the same three baseline strict-Clippy findings remain in Config, CLI, and
  Desktop, with no unrelated cleanup folded into this milestone;
- the Desktop Rust target builds;
- all eight non-network CLI smoke cases preserve their expected content and
  native exit status;
- all 40 local Markdown links in the repository documentation resolve.

## 6. Target Architecture Goal

The implemented target is one shared Rust crate for the existing
audio-to-Final-Text path:

    orally-cli ---------\
                         -> orally-speech -> orally-asr -> orally-audio + orally-core
    orally-desktop -----/                \-> orally-llm -> orally-core
                                        \-> orally-core

`orally-speech` owns the typed runtime plan, ASR protocol parsing and adapter
selection, concrete ASR adapter construction, Raw/Local/AI refinement selection,
AI-to-local fallback, default dictionary assembly, and a structured outcome.

Its runtime interface is deliberately narrow:

    SpeechProcessor::new(SpeechPlan) -> Result<SpeechProcessor, SpeechBuildError>
    SpeechProcessor::process(AudioInput) -> Result<SpeechOutcome, OrallyError>

`SpeechOutcome` preserves Raw Transcript, Final Text, and cleanup changes.
`SpeechPlan` is runtime input, not a Workflow, product Module, Service
Connection, or persisted schema.

The seam is justified by two real callers. Deleting it would return protocol
inference, provider construction, refinement policy, fallback order, and result
assembly to both CLI and Desktop.

The crate does not own recording, file reading, Toggle Mode, Global Trigger,
External-request Blocking, terminal output, process exit, Direct Insertion,
foreground restoration, Local History, tray, overlay, or UI. CLI and Desktop
map their different configuration sources and current behavior into
`SpeechPlan`; the shared crate does not silently make those behaviors identical.

## 7. Refactor Rules

Every implementation stage must follow these rules:

1. Preserve observable behavior unless a separate behavior decision was
   reviewed first.
2. Make one primary structural change at a time.
3. Do not combine file moves, behavior changes, dependency changes, and feature
   work in one stage.
4. Do not add a crate, trait, dependency, framework, or configuration key
   without a current need.
5. Keep Desktop usable and do not rewrite it during the CLI stages.
6. Do not rename crates merely to make the tree look finished.
7. Stop after every stage, verify it, and review the next stage before starting.
8. Keep every implementation stage commit-ready and separately reviewable.
   Create commits only when the user requests or approves them; when commits are
   created, keep one primary stage per commit. If completion checks fail, revise
   that stage instead of hiding a repair inside the next one.
9. Use product term `Module` only for the product concept; use Rust crate or
   source module for code structure.
10. Do not add a behavior-test skeleton during this route unless a later
    document review explicitly introduces that step.

## 8. Explicit Non-goals

The initial route does not:

- change CLI command names, flags, aliases, help, output, exit behavior, or
  defaults;
- make CLI voice commands adopt Desktop AI, privacy, fallback, insertion, or
  JSONL history behavior;
- normalize existing flag, environment, TOML, and default precedence;
- change ASR or LLM endpoints, request shapes, protocols, timeouts, or prompts;
- change the hard-coded dictionary, process task aliases, `listen` shortcut,
  forced insertion, or unused `listen --seconds` behavior;
- redesign TOML configuration or credentials;
- upgrade third-party dependencies or rewrite unrelated `Cargo.lock` entries;
- move ASR selection into a public factory during the approved stages;
- migrate the text-only `process --ai` diagnostic command;
- delete or rename the demo-only `OrallyPipeline` cluster in this milestone;
- add Workflow, product Module, Service Connection, or Active Workflow;
- add SQLite or migrate the JSONL history prototype;
- complete Portable Installation, Local History, Audio Retention, pending
  states, or Insertion Recovery;
- implement progressive Audio Segments, First-run Setup, arbitrary Global
  Trigger capture, or a replacement Desktop UI;
- implement TSF, Android, macOS, Linux, local ASR, sync, plugins, encryption, a
  new CLI framework, or a new async runtime.

## 9. Stage Protocol

Before each stage:

1. re-read that stage and the compatibility facts above;
2. confirm the previous stage has a reviewable diff and recorded verification;
3. capture representative stdout, stderr, and exit codes where the stage can
   affect them;
4. implement only the listed change;
5. run the listed verification;
6. stop and report the result before discussing the next stage.

Build once with `cargo build -q -p orally-cli`, then run the native executable
commands below. Running the binary directly matters for the two parse-error
cases because the `cargo run` wrapper reports a child failure as its own exit
`1` instead of preserving the CLI's native exit `2`.

The standard non-network smoke matrix is:

| Command | Expected exit |
| --- | --- |
| `.\target\debug\orally-cli.exe` | `0`, global help on stdout |
| `.\target\debug\orally-cli.exe unknown-command` | `0`, same global help on stdout |
| `.\target\debug\orally-cli.exe process "嗯 今天 写 一封 邮件"` | `0`, built-in processed text on stdout |
| `.\target\debug\orally-cli.exe demo --help` | `0`, runs the demo and ignores the trailing flag |
| `.\target\debug\orally-cli.exe config path unexpected` | `0`, prints the config path and ignores the trailing value |
| `.\target\debug\orally-cli.exe record --seconds nope` | `2`, parse error and global help |
| `.\target\debug\orally-cli.exe process --help` | `2`, current command-help error path |
| `.\target\debug\orally-cli.exe transcribe --file Z:\\orally-missing.wav --api-key dummy --model dummy` | `1`, file error without a network request |

Compare output against the capture made immediately before the stage. These
checks characterize existing behavior without introducing a test framework.

## 10. Completed Shared-speech Milestone

The 2026-08-30 user request activated this milestone and superseded the earlier
CLI-only sequence. Each source migration remains a separate reviewable step; no
stage silently authorizes the next.

### S0 — Shared-seam Decision

- **Status:** Completed.
- **Goal:** record the smallest interface shared by CLI and Desktop.
- **Decision:** add `crates/orally-speech`; use a typed `SpeechPlan`, one
  `SpeechProcessor::process` runtime entry, and a structured
  `SpeechOutcome`.
- **Behavior rule:** callers map their existing policy into the plan. CLI voice
  commands remain Raw or Local Basic Cleanup; Desktop retains Raw, Local, AI,
  and AI-to-local fallback.
- **Ownership rule:** External-request Blocking, recording, insertion, Local
  History, and UI remain outside the crate.
- **Completion:** this baseline is Active and the interface has focused tests
  through its process seam.

### S1 — Add `orally-speech`

- **Status:** Completed.
- **Goal:** implement the shared crate before migrating callers.
- **Allowed changes:** workspace membership, the new crate, necessary local
  workspace dependency entries, and focused tests in the new crate.
- **Interface:** production callers construct `SpeechProcessor` from a typed
  runtime plan and call `process(AudioInput)`.
- **Implementation hidden by the seam:**
  - ASR protocol aliases and `auto` HTTP endpoint/audio-format negotiation;
  - concrete multipart or chat-audio adapter construction;
  - direct-or-named-environment Provider Credential resolution;
  - Raw Transcript preservation;
  - Raw, Local Basic Cleanup, or AI Post-processing selection;
  - AI-process failure fallback to Local Basic Cleanup;
  - current default dictionary and `ProcessInput` assembly;
  - structured Raw Transcript, Final Text, and cleanup changes.
- **Compatibility constraints:**
  - AI credentials and the AI adapter are resolved only after ASR succeeds;
  - missing AI credentials or AI adapter construction errors do not fallback;
  - fallback covers only an AI `process` error;
  - ASR failure never runs refinement;
  - processing remains synchronous and blocking.
- **Must not touch:** existing callers, persisted config schema, external
  request shapes, third-party dependency versions, or the old demo pipeline.
- **Verification:** focused interface tests cover Raw, Local, AI success, AI
  failure with and without fallback, ASR failure, protocol parsing/normalization,
  credential redaction, and call order; new-crate strict Clippy passes.
- **Rollback:** remove the workspace member and new crate if its interface is
  wider than the duplicated behavior it hides.

### S2 — Migrate CLI Voice Paths

- **Status:** Completed.
- **Goal:** make `transcribe`, `dictate`, and `listen` call
  `orally-speech` after they obtain audio.
- **Allowed changes:** CLI source and manifest plus necessary local lockfile
  entries.
- **Behavior to preserve:**
  - CLI flag/environment/TOML precedence remains in CLI parsing;
  - `--raw` selects Raw; every other voice path selects Local Basic Cleanup;
  - CLI does not adopt `postprocess.mode`, AI fallback,
    `privacy.allow_external_requests`, or JSONL history;
  - file reading, recording, Toggle Mode, terminal output, exit codes, changes,
    forced `listen` insertion, and clipboard insertion remain in CLI.
- **Must not touch:** `record`, text-only `process --ai`, command syntax,
  prompts, defaults, Desktop, or persisted config.
- **Verification:** existing CLI tests and the complete smoke matrix pass
  without observable differences. Strict CLI Clippy still stops at the
  baseline `items_after_test_module` debt; this stage does not mix in that
  unrelated cleanup.
- **Rollback:** revert only the CLI migration if stdout, stderr, exit status,
  option precedence, or insertion behavior changes.

### S3 — Migrate Desktop Speech Processing

- **Status:** Completed.
- **Goal:** replace Desktop's duplicate ASR construction and refinement function
  with `orally-speech`.
- **Allowed changes:** Desktop Rust source and manifest plus necessary local
  lockfile entries.
- **Behavior to preserve:**
  - External-request Blocking remains after recording and before processor
    construction;
  - `output.raw` bypasses refinement;
  - `postprocess.mode = "llm" | "ai"` selects AI Post-processing;
  - other modes use Local Basic Cleanup;
  - fallback scope and message remain unchanged;
  - foreground restoration, Direct Insertion, JSONL history order, overlay, and
    error presentation remain Desktop responsibilities.
- **Must not touch:** Tauri commands, tray, hotkey, recording lifecycle, UI,
  config schema, History storage, or clipboard adapter.
- **Verification:** Desktop compiles, workspace tests pass, and duplicate
  `AsrProtocol`, `AsrOptions`, provider factory, refinement function, and
  dictionary are absent from Desktop.
- **Rollback:** revert only the Desktop migration if the processing sequence,
  insertion, or history behavior changes.

### S4 — Integration Stop

- **Status:** Completed and reviewed as the foundation for the continuing
  refactor.
- **Goal:** verify the shared seam and stop before unrelated cleanup.
- **Allowed changes:** factual documentation updates only after verification.
- **Verification:** workspace tests, formatting, and the CLI smoke matrix pass;
  strict Clippy passes for `orally-speech`, `orally-core`, `orally-asr`, and
  `orally-llm`; Desktop builds. The three strict-Clippy findings recorded at the
  baseline remain unchanged in Config, CLI, and Desktop rather than being
  folded into this milestone.
- **Explicit stop:** do not delete `OrallyPipeline`, split CLI files, normalize
  CLI/Desktop behavior, add async/streaming, or begin a new feature in this
  milestone.
- **Next review:** decide separately whether the demo pipeline should be
  removed and whether CLI voice commands should ever adopt Desktop AI or privacy
  policy.

## 11. Deferred Design Commitments

Accepted ADRs preserve these decisions for the day their capabilities enter
active scope:

| ADR | Deferred capability |
| --- | --- |
| [ADR-0002](../adr/0002-compose-modules-into-one-request.md) | Ordered product Modules compose into one AI request |
| [ADR-0003](../adr/0003-share-provider-connections-across-workflows.md) | Service Connections are shared across Workflows |
| [ADR-0004](../adr/0004-keep-portable-data-with-the-application.md) | Complete Portable Installation data contract |
| [ADR-0006](../adr/0006-store-workflows-in-sqlite-and-modules-as-toml.md) | SQLite stores Workflow, connection, credential, and Local History records; product Modules remain TOML |
| [ADR-0007](../adr/0007-stream-long-recordings-through-audio-segments.md) | Long recordings use progressive Audio Segments |

[ADR-0001](../adr/0001-ordered-workflow-stages.md) and
[ADR-0005](../adr/0005-use-hybrid-portable-storage.md) are historical because
they were superseded.

Other deferred possibilities include TSF, Android, macOS, Linux, local ASR,
sync, provider plugins, personal dictionaries, encrypted backup, and a complete
Local History interface.

## 12. Feature Entry Gate

Before any deferred capability becomes active:

1. state the user problem and why the current useful slice is insufficient;
2. re-read product, privacy, product language, and related ADRs;
3. choose the smallest behavior that solves the problem;
4. list explicit non-goals;
5. update this baseline;
6. stop for user review;
7. implement only after the baseline is approved and Active.

This gate keeps learning and feature growth from collapsing into one
unreviewable change.

## 13. Completed Bilingual-documentation Milestone

The continuing goal begins with a documentation baseline so every later feature
can be reviewed in either English or Simplified Chinese without changing the
meaning of its scope or stop conditions.

### D0 — Establish The Pairing Contract

- **Status:** Completed; awaiting user review before the next backend
  milestone.
- **Goal:** make every repository-owned user, product, engineering, and decision
  document available as an English canonical file and a Simplified Chinese
  paired file.
- **Naming:** keep existing English paths; add `.zh-CN.md` before the extension
  for the Chinese pair.
- **Required pairs:** root README, product language, documentation map, product
  definition, architecture, privacy, roadmap, Grilling directory guide,
  refactor baseline, Windows IME research note, and ADR-0001 through ADR-0007.
- **Exception:** individual exploratory documents generated by the Grilling
  skill may remain English. If a conclusion becomes a requirement or decision,
  promote it into a bilingual product document, baseline, or ADR.
- **Translation invariants:** keep status values, commands, paths,
  configuration keys, environment variables, error text, code identifiers, ADR
  status, and supersession relationships unchanged.
- **Agent-only files:** `.agents/` remains English-only so machine instructions
  do not acquire a second, drifting copy.
- **Verification:** every required pair exists, every pair has bidirectional
  language navigation, local Markdown links resolve, ADR statuses match, and
  English/Chinese heading structures have been reviewed for coverage.
- **Completion evidence:** `scripts/check-docs.ps1` passes for 17 required
  language pairs and 151 local links; heading structures and ADR statuses match;
  the translations received independent coverage and terminology review.
- **Non-goals:** no user-facing behavior, dependency, persisted schema, or UI
  change enters D0.
- **Stop:** complete and review D0 before activating the next backend capability.

## 14. Candidate Backend Sequence

This is a dependency-aware planning sequence, not blanket implementation
authorization. Each item receives its own Active milestone through the Feature
Entry Gate:

1. strengthen evidence for the current backend through separate, behavior-neutral
   milestones: JSONL append, Config I/O and path precedence, ASR/LLM loopback
   HTTP contracts, then a repeatable manual Windows adapter smoke check;
2. close isolated structural and strict-Clippy debt separately from those
   contract-test milestones;
3. improve the current core path with progressive Audio Segments;
4. implement Windows interaction capabilities separately: Global Trigger,
   Automatic Stop, then First-run Setup;
5. introduce domain behavior in order: product Module, Service Connection,
   Workflow, then Active Workflow;
6. add the ADR-0006 persistence model only after those domain interfaces are
   stable;
7. build Local History, pending states, Insertion Recovery, and Audio Retention
   on that persistence foundation;
8. complete the Portable Installation contract;
9. revisit TSF, local ASR, other platform shells, sync, plugins, and encryption
   only after the Windows backend is stable.

## 15. Draft Backend-evidence Milestone

This section is a proposal for the next review. It is not Active until the user
approves it.

### E1 — Lock The JSONL Append Contract

- **Status:** Draft.
- **Problem:** Desktop currently appends successful Voice Input outcomes through
  `HistoryStore`, but the only storage test covers path derivation. Parent
  directory creation, JSON encoding, append order, and failure propagation are
  relied upon without direct evidence.
- **Goal:** characterize the existing `HistoryStore::append` behavior through
  its current interface without changing product behavior.
- **Allowed changes:** focused `orally-storage` tests and private test support if
  required. Avoid a new dependency when standard-library temporary paths are
  sufficient.
- **Required cases:** create a missing parent directory; write one valid JSONL
  object; append multiple entries without overwrite and in order; preserve
  Chinese text while escaping embedded newlines; return an `OrallyError` for an
  invalid or unwritable target.
- **Interface:** keep `HistoryStore::new`, `HistoryStore::append`,
  `HistoryStore::path`, and `default_history_path` unchanged.
- **Non-goals:** no history reading, query, deletion, retention limit, SQLite,
  Workflow identity, Raw Transcript opt-in, Audio Retention, Desktop UI, or
  persisted-schema change.
- **Verification:** focused storage tests, workspace tests, formatting, strict
  `orally-storage` Clippy, and unchanged CLI smoke behavior pass.
- **Stop:** report E1 independently before drafting Config I/O tests or another
  backend capability.

## 16. Active Model Configuration Editor

- **Status:** Active, requested directly by the user on 2026-10-04 for `develop`.
- **Problem:** Home profile cards only change titles, and added model/Prompt
  nodes are DOM-only demonstrations that disappear on reload or save.
- **Behavior:** merge home and pipeline into one editor with profile switching,
  saving, importing, exporting, creating, and renaming at the top; stack ASR
  above editable post-processing model/Prompt nodes. ASR exposes service URL,
  model, and plaintext API Key; post-processing retains Prompt. ASR automatically
  negotiates
  OpenAI-compatible transcriptions or Chat Audio, with the selector hidden
  (user refinement on 2026-10-08). Support
  enabling, removal, and ordering with drafts preserved on switch. The tray lists
  saved profiles, marks the active one with a dot, and switches it immediately.
  Settings omits output language and config-path controls, keeps only the
  captured dictation shortcut, and stores config beside the executable
  (user refinement on 2026-10-09).
  A further 2026-10-09 refinement removes ASR Prompt from configuration and
  speech-command options, leaving recognition vocabulary for a future dictionary
  feature. Legacy ASR Prompt values are ignored on load and removed on resave.
  Configuration help omits development notes. Dictionary support remains deferred.
- **Persistence:** self-contained profiles in existing TOML, with selected
  profile fields mirrored at the top level for old readers. Global output,
  audio, privacy, and shortcuts remain outside profiles. See
  [ADR-0008](../adr/0008-edit-self-contained-model-profiles.md) for the scoped
  exception to the deferred shared-Workflow design.
- **Execution:** enabled AI models process sequentially; enabled Prompt children
  compose each model's instructions. Preserve raw output and existing local
  fallback behavior. CLI speech commands remove ASR Prompt input; their other
  behavior and text post-processing Prompt options remain unchanged. Chat Audio
  Data URLs retain only optional language context; raw Base64 Chat Audio keeps
  its fixed transcription instruction.
- **Testing surface:** the global test records Voice Input with the current
  unsaved profile and global settings. ASR recording tests bypass refinement;
  text tests retain refinement-only coverage. No upload is required, and tests
  never persist configuration, insert output, or write history. Cancellation and
  closure release recording resources.
- **Verification:** old/new TOML round trips including ignored legacy ASR Prompt
  values and their removal on resave, draft switching and import
  validation, node ordering, real loopback sequential model requests, Rust
  workspace tests, frontend state tests, formatting, paired docs, and a browser
  interaction/layout check.
