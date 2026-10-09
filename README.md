# Orally

[English](README.md) | [简体中文](README.zh-CN.md)

Orally is a working early prototype for AI-assisted voice input and a
learning-oriented Rust refactoring project.

The current useful slice records or accepts audio, sends it to an
OpenAI-compatible speech-recognition provider, refines the transcript locally or
with an AI postprocessor, and produces paste-ready text. The Windows Desktop App
also provides Toggle Mode, tray controls, an overlay, and clipboard insertion.

## Development Approach

The active refactor now shares the existing ASR and post-processing path through
`orally-speech`. CLI voice commands and the Desktop App use that seam while
retaining their different outer policies. Its purpose is to improve
responsibility placement in small, behavior-preserving steps, not to build every
capability in the product notes.

Read the
[Refactor Architecture Baseline](docs/engineering/refactor-baseline.md) before
implementation work and [Documentation Map](docs/README.md) for document roles.

## Local Development (Windows)

With Rust, Node.js/npm, the Windows Tauri build prerequisites, and `just`
installed, run this from the repository root:

```powershell
just dev
```

This installs frontend development dependencies with `npm ci` when Tauri or Vite
is missing, then starts the Desktop App through `tauri dev`. Tauri starts Vite at
`http://localhost:1420`, reloads frontend edits, and rebuilds the app after Rust
changes. Later starts reuse the installed dependencies. Run `just setup` after
changing `package.json` or `package-lock.json` to refresh them. The first Rust
build can take longer. Press `Ctrl+C` in the terminal to stop development.

Run `just` or `just --list` to see the commands:

| Command | Action |
| --- | --- |
| `just dev` | Prepare frontend dependencies if needed and start desktop development |
| `just dev-ui` | Prepare frontend dependencies if needed and start only Vite |
| `just setup` | Install or refresh locked frontend development dependencies |
| `just build` | Build the release desktop app with embedded UI and prepare `dist/portable/Orally` |
| `just build-debug` | Build all Rust workspace packages in debug mode |
| `just test` | Run Rust workspace tests |
| `just check` | Check Rust formatting and compilation |
| `just fmt` | Format the Rust workspace |

## Current Prototype

This repository currently contains:

- `crates/orally-core`: shared Rust domain types, interfaces, Local Basic
  Cleanup, demo adapters, and the legacy demo pipeline.
- `crates/orally-audio`: cross-platform microphone capture, PCM16 metrics, and
  WAV encoding.
- `crates/orally-speech`: shared audio-to-Final-Text orchestration, including
  ASR adapter selection and Raw, local, or AI refinement.
- `apps/orally-cli`: a developer-facing tool for demo, recording,
  transcription, text processing, dictation, hotkey listening, and
  configuration.
- `apps/orally-desktop`: a Tauri desktop shell with a Windows tray icon and a
  Material-style settings UI backed by the local TOML config.
- `docs/product`: product definition, architecture notes, privacy model,
  roadmap, and future product grilling notes.
- `docs/engineering`: human-readable implementation and platform notes.
- `.agents`: agent-only setup, skill, and tooling context.

## Try It

```powershell
cargo run -p orally-cli -- demo
cargo run -p orally-cli -- process "嗯 今天 我想 写 一封 邮件 给 visual studio code 团队"
cargo run -p orally-cli -- record --seconds 3 --output orally-recording.wav
cargo run -p orally-cli -- transcribe --file orally-recording.wav --model <model>
cargo run -p orally-cli -- dictate --seconds 3 --model <model>
cargo run -p orally-cli -- transcribe --file orally-recording.wav --raw
cargo run -p orally-cli -- dictate --seconds 3 --show-changes
cargo run -p orally-cli -- dictate --seconds 3 --insert --paste-delay-ms 1200
cargo run -p orally-cli -- listen --paste-delay-ms 300
cargo run -p orally-cli -- config init --provider openai-compatible
cargo run -p orally-cli -- config set output.paste_delay_ms 300
cargo run -p orally-cli -- config show
cargo run -p orally-desktop
```

The `record` command captures audio from the default microphone and writes a
PCM16 WAV file. `transcribe` sends an existing WAV to the configured ASR
provider. `dictate` records from the microphone and sends the captured audio to
ASR in one step.

By default, remote ASR output is passed through Orally's built-in post-processing
pipeline. Use `--raw` to print the provider transcript without post-processing,
or `--show-changes` to show local cleanup actions.

On Windows, use `--insert` to paste the final text into the currently focused
input field. The CLI waits for `--paste-delay-ms` before pasting so you can focus
the target app after recording or transcription completes.

Use `listen` on Windows to keep Orally running and trigger dictation with
`Ctrl+Alt+Space`. Press once to start recording, then press the same hotkey again
to stop recording, transcribe, post-process, and paste. Press `Ctrl+C` in the
terminal to stop the listener.

For ASR, configure an OpenAI-compatible speech endpoint:

```powershell
$env:ORALLY_ASR_API_KEY="..."
$env:ORALLY_ASR_BASE_URL="https://api.openai.com/v1"
$env:ORALLY_ASR_MODEL="whisper-1"
cargo run -p orally-cli -- transcribe --file orally-recording.wav
```

ASR defaults to `auto`: it first tries multipart `/audio/transcriptions`, then
tries Chat Audio at `/chat/completions` if the transcription route returns
HTTP 404 or 405. Chat Audio sends a WAV Data URL, with an existing language hint
as optional system context. HTTP 400 or 422 triggers a second attempt with raw
Base64, `format: "wav"`, and a fixed transcription instruction.
Native multipart uploads are buffered in memory so an early HTTP rejection can
still drive protocol selection. An unreadable error-response body preserves the
received HTTP status.
Authentication, rate limits, server errors, network failures, and invalid
successful responses do not trigger protocol retries. Successful selection is
reused for subsequent audio chunks. Orally does not guess from host or model
names. The CLI can explicitly select `--protocol openai-transcriptions` or
`--protocol chat-audio`. LLM post-processing uses text `/chat/completions`;
sharing a service URL and API Key does not verify the speech model.

## Configuration

Orally reads and saves `config.toml` beside its executable. If that file is
missing, it copies an existing legacy configuration from `ORALLY_CONFIG`,
`%APPDATA%\Orally\config.toml`, or `$HOME/.config/orally/config.toml`, in that
order. The original file is retained, and an existing executable-local file is
never replaced by migration. An unreadable or invalid legacy file reports an
error instead of silently resetting settings.

Initialize a provider preset:

```powershell
cargo run -p orally-cli -- config init --provider openai-compatible
```

Supported presets:

- `openai-compatible` (default; uses `ORALLY_OPENAI_COMPAT_API_KEY`)
- `openai`

Show the active config:

```powershell
cargo run -p orally-cli -- config show
```

Update one config value:

```powershell
cargo run -p orally-cli -- config set asr.model whisper-1
cargo run -p orally-cli -- config set asr.api_key_env ORALLY_OPENAI_COMPAT_API_KEY
cargo run -p orally-cli -- config set postprocess.mode llm
cargo run -p orally-cli -- config set postprocess.model gpt-4o-mini
cargo run -p orally-cli -- config set postprocess.fallback_to_builtin true
cargo run -p orally-cli -- config set output.paste_delay_ms 300
cargo run -p orally-cli -- config set output.restore_clipboard true
cargo run -p orally-cli -- config set output.show_changes true
```

Direct `asr.api_key` and `postprocess.api_key` values are supported, but they
are stored as plaintext. Passing a direct key to `config set` may also retain it
in shell history, and `config show` prints direct keys without redaction.
Prefer environment-variable references, and redact configuration output before
copying or sharing it.

Common keys:

- `asr.base_url`
- `asr.model`
- `asr.protocol`
- `asr.api_key`
- `asr.api_key_env`
- `asr.language`
- `postprocess.mode`
- `postprocess.base_url`
- `postprocess.model`
- `postprocess.api_key`
- `postprocess.api_key_env`
- `postprocess.system_prompt`
- `postprocess.user_template`
- `postprocess.fallback_to_builtin`
- `output.locale`
- `output.raw`
- `output.show_changes`
- `output.insert`
- `output.paste_delay_ms`
- `output.restore_clipboard`
- `output.restore_clipboard_delay_ms`
- `audio.dictate_seconds`
- `audio.record_output`
- `audio.input_mode`
- `audio.auto_stop_enabled`
- `audio.min_record_ms`
- `audio.max_record_ms`
- `audio.silence_timeout_ms`
- `audio.silence_threshold`
- `hotkey.preset`
- `privacy.allow_external_requests`
- `privacy.history_enabled`
- `privacy.history_path`

For privacy, prefer storing the API key environment variable name and keeping
the key in your shell or OS environment. For the OpenAI-compatible preset, set:

```powershell
$env:ORALLY_OPENAI_COMPAT_API_KEY="..."
```

If you want the desktop app to run without a pre-set environment variable, the
local config can also store `asr.api_key`. That value is saved in
`config.toml`, so use it only on machines you trust.

## Desktop Settings App

Run the current desktop settings shell:

```powershell
cargo run -p orally-desktop
```

The app uses Tauri v2 and keeps a tray icon alive while the settings window is
open or hidden. Left-click the tray icon, or choose `Open Settings`, to show the
window again.

At startup, the desktop app registers the Windows dictation hotkey selected by
`hotkey.preset`. New configurations default to `Ctrl+Alt+Space`; an unreadable
or invalid preset also falls back to that shortcut. Press once to start
recording. A compact, translucent dark overlay with rounded corners appears
near the bottom of the screen. Its bottom bars react to microphone volume and
return to small dots during silence. Press the same hotkey again, or click the
stop icon, to stop recording, transcribe,
post-process, and paste the final text back into the window that was active when
recording started.

The desktop app currently uses Toggle Mode exclusively. Silence, key release,
fixed duration, and maximum duration do not stop a recording; only another
hotkey press or an explicit stop action ends it. Legacy automatic-stop fields
remain in the config schema for compatibility but are ignored by the desktop
runtime.

The tray menu can also start or stop dictation, pause or resume the hotkey, open
settings, and quit Orally. Its configuration submenu lists every saved profile,
with a dot before the active one. Selecting a profile activates it immediately
and updates the settings window while retaining unsaved drafts. Saving profiles,
including new names, refreshes this menu.

Click the dictation shortcut in Settings, then press the desired combination.
Escape cancels capture. Saving registers the new shortcut immediately; a
registration conflict or failed save preserves the previous binding. The old
binding is temporarily released during capture and restored when capture ends.
Settings only exposes the dictation shortcut; `Ctrl+S` remains the fixed save
action inside the settings window.

The settings UI can edit:

- OpenAI-compatible speech service URL, model, and API key.
- AI post-processing mode and ordered OpenAI-compatible model and Prompt nodes.
- Global shortcut, paste delay, Raw Transcript mode, and
  clipboard restoration.
- The shared `output.insert` and `output.show_changes` settings used by CLI
  voice commands. Desktop currently always inserts Final Text and does not show
  the processing-change list.
- External-request privacy switch and local history.

Saving writes `config.toml` beside the executable. Desktop and CLI share the
file when installed in the same directory. Settings omits output language and
configuration-path controls; the existing locale value is retained. Leave the
API Key field blank to use only an environment variable reference.

## Desktop AI Post-Processing

The model editor combines profile selection and pipeline editing. Use the top
bar to switch, create, import, export, or save profiles, and rename the current
profile below it. Switching retains unsaved drafts; saving writes the collection
and activates the selected configuration. Speech recognition appears above
post-processing. Speech recognition uses service URL, model, and API Key;
post-processing also provides Prompt. Desktop ASR, connection tests, and
recording tests share automatic transcription/Chat Audio negotiation. Older
profiles are normalized to `auto`, preserving service fields. There is no
recognition-protocol selector.
Speech recognition has no configurable Prompt. Older ASR Prompt values are
ignored on load and omitted when saved again. A future dictionary feature is
planned for recognition vocabulary; it is not implemented yet.
API keys are masked by default, with an eye button to show or hide them, and
are saved as plaintext. Output, privacy, and
shortcut settings remain global in the Settings page.

Each post-processing model has its own service, credentials, Prompt, and
ordered Prompt children. Add, remove, enable, or reorder models and Prompts.
In AI mode, enabled models run sequentially, with each receiving the previous
model's output. Prompt children append to their model's system instructions.
Local mode ignores AI nodes; disabling all AI models passes through the raw
transcript. Existing single-model TOML continues to load; hidden language,
environment-variable, template, and fallback settings retain their saved values.

Each speech or LLM service offers a connection test using its current fields.
The top-bar Test button defaults to a microphone Voice Input test: start
recording, speak, then stop to run recognition and the configured post-processing
chain. It captures the selected profile and global settings, including unsaved
edits, when recording starts. The speech card's recording test runs ASR alone;
text-only post-processing is also available in the dialog. No audio upload is
needed. Results show the recognized text and final output without saving
configuration, inserting text, or writing history. Closing or cancelling releases
the microphone; a test recording is limited to two minutes and 15 MiB.
Connection tests report actual service failures; input tests show a warning if
the configured local fallback is used. External-request blocking also applies
to tests. Browser preview asks for microphone access and makes direct service
requests, requiring a service that permits CORS. The native app uses its existing
microphone recorder and is not subject to browser CORS restrictions.

Export saves one JSON profile, including directly configured API keys. Import
accepts JSON profiles or configuration documents, and the Desktop App also
accepts existing TOML. An import becomes a new draft before saving. Browser
preview persists separately in local storage and requires the native app for
TOML parsing. The CLI retains its single-model flow. See the
[editor storage decision](docs/adr/0008-edit-self-contained-model-profiles.md).

By default Orally uses the built-in local cleaner. Set:

```toml
[postprocess]
mode = "llm"
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
api_key_env = "ORALLY_OPENAI_COMPAT_API_KEY"
```

In the Desktop flow, the AI postprocessor sends the ASR transcript to an
OpenAI-compatible `/chat/completions` endpoint and expects only the final text in
response. Use `postprocess.system_prompt` and `postprocess.user_template` to
customize the cleanup behavior. The user template supports `{{transcript}}` and
`{{locale}}`.

Set `output.raw = true` to bypass both built-in and AI post-processing.

If `postprocess.fallback_to_builtin = true`, an error returned by the Desktop AI
processor falls back to Orally's local cleaner. A missing AI credential or
processor-construction error currently fails before that fallback. CLI voice
commands ignore `postprocess.mode`; `process --ai` is an independent developer
command and does not use this fallback setting.

## JSONL History Prototype And Clipboard Privacy

The current default is `privacy.history_enabled = true`. After successful
insertion, the Desktop flow therefore appends local JSONL history beside the
active config file as `history.jsonl`. Each entry stores both the raw ASR text
and the final inserted text, plus a millisecond timestamp and the `desktop`
provider marker. Set `privacy.history_path` to use a custom file, or set
`privacy.history_enabled = false` to disable this retention. CLI voice commands
do not write this file.

In the Desktop flow, set `privacy.allow_external_requests = false` to block
remote ASR and AI post-processing requests. CLI network commands do not
currently enforce this setting.

On Windows, Orally uses the clipboard fallback insertion path. When
`output.restore_clipboard = true`, Orally restores the previous text clipboard
after paste and removes the generated text from the clipboard. This first
implementation preserves previous text clipboard content; non-text clipboard
formats are not restored yet.

## Executable-local Windows Prototype

Orally uses executable-local configuration without registering as a Windows
input method. `config.toml` always lives beside the executable, including on
first save. Legacy locations are migration sources only, and directory write
errors are reported without redirecting configuration elsewhere.

This is a config-local prototype, not the complete Portable Installation
contract defined by the deferred product design.

Build a portable folder:

```powershell
just build
```

This compiles an optimized release executable with the UI embedded, so it does
not need the Vite development server. A failed build stops packaging.

The script writes:

```text
dist\portable\Orally\Orally.exe
dist\portable\Orally\config.toml
dist\portable\Orally\config.example.toml
```

Run `Orally.exe` from that folder to keep settings local to the portable
directory. You can also create the same executable-directory config from the CLI:

```powershell
cargo run -p orally-cli -- config init --provider openai-compatible --portable
```

The packaging script does not overwrite an existing `config.toml`; it always
writes the latest `config.example.toml` for reference.

## Direction

The behavior-preserving shared-speech milestone in the architecture baseline is
implemented in the current working tree. CLI and Desktop keep their current
outer behavior while sharing audio-to-Final-Text processing through
`orally-speech`.

The continuing refactor is backend-first and documentation-gated. English
documents remain canonical and Simplified Chinese paired documents are updated
with them. Each later capability enters the Active baseline as its own minimal
milestone before implementation; the broader product notes are not one combined
backlog to implement at once.

TSF, Android, macOS, Linux, local ASR, sync, plugins, and the complete Portable
Installation design remain deferred possibilities rather than current
milestones.
