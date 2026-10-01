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
cargo run -p orally-cli -- config init --provider aliyun-openai
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

For ASR, configure an OpenAI-compatible transcription endpoint:

```powershell
$env:ORALLY_ASR_API_KEY="..."
$env:ORALLY_ASR_BASE_URL="https://openrouter.ai/api/v1"
$env:ORALLY_ASR_MODEL="<model>"
cargo run -p orally-cli -- transcribe --file orally-recording.wav
```

OpenRouter audio models use chat-completions audio input, so Orally auto-selects
the `chat-audio` protocol when the base URL is `https://openrouter.ai/api/v1`.
For Whisper-style multipart transcription APIs, use:

```powershell
$env:ORALLY_ASR_PROTOCOL="openai-transcriptions"
$env:ORALLY_ASR_BASE_URL="https://api.openai.com/v1"
```

For Alibaba Cloud Model Studio through its OpenAI-compatible endpoint:

```powershell
$env:ORALLY_ASR_API_KEY=$env:ORALLY_OPENAI_COMPAT_API_KEY
$env:ORALLY_ASR_BASE_URL="https://ws-xzr3kkbjij82s72f.cn-beijing.maas.aliyuncs.com/compatible-mode/v1"
$env:ORALLY_ASR_MODEL="qwen3-asr-flash"
$env:ORALLY_ASR_PROTOCOL="chat-audio"
cargo run -p orally-cli -- transcribe --file orally-recording.wav
```

Some OpenAI-compatible chat audio models, including `qwen3-asr-flash`, expect
`input_audio.data` as a WAV data URL. Orally selects that request shape from the
model name and compatible endpoint.

## Configuration

Orally can load local configuration from `%APPDATA%\Orally\config.toml` on
Windows. Set `ORALLY_CONFIG` to use a custom path.

Initialize a provider preset:

```powershell
cargo run -p orally-cli -- config init --provider aliyun-openai
```

Supported presets:

- `aliyun-openai`
- `openrouter`
- `openai`

Show the active config:

```powershell
cargo run -p orally-cli -- config show
```

Update one config value:

```powershell
cargo run -p orally-cli -- config set asr.model qwen3-asr-flash
cargo run -p orally-cli -- config set asr.api_key_env ORALLY_OPENAI_COMPAT_API_KEY
cargo run -p orally-cli -- config set postprocess.mode llm
cargo run -p orally-cli -- config set postprocess.model deepseek-v4-flash-0731
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
- `asr.prompt`
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
recording. A small always-on-top prompt appears near the bottom of the screen.
Press the same hotkey again, or click `停止`, to stop recording, transcribe,
post-process, and paste the final text back into the window that was active when
recording started.

The desktop app currently uses Toggle Mode exclusively. Silence, key release,
fixed duration, and maximum duration do not stop a recording; only another
hotkey press or an explicit stop action ends it. Legacy automatic-stop fields
remain in the config schema for compatibility but are ignored by the desktop
runtime.

The tray menu can also start or stop dictation, pause or resume the hotkey, open
settings, and quit Orally.

The current prototype still exposes a fixed hotkey preset list and reads changes
only at startup. Direct shortcut capture, immediate conflict validation, and
registration without restarting are deferred product improvements, not part of
the completed shared-speech milestone.

The settings UI can edit:

- API provider preset, base URL, model, protocol, API key, API key environment
  variable, language hint, and ASR prompt.
- AI post-processing mode, OpenAI-compatible LLM endpoint/model/API key, system
  prompt, and user template.
- Global shortcut, paste delay, output locale, Raw Transcript mode, and
  clipboard restoration.
- The shared `output.insert` and `output.show_changes` settings used by CLI
  voice commands. Desktop currently always inserts Final Text and does not show
  the processing-change list.
- External-request privacy switch and local history.

Saving writes the same local config used by the CLI, usually
`%APPDATA%\Orally\config.toml` on Windows. Leave the API Key field blank to use
only an environment variable reference.

## Desktop AI Post-Processing

By default Orally uses the built-in local cleaner. Set:

```toml
[postprocess]
mode = "llm"
base_url = "https://ws-xzr3kkbjij82s72f.cn-beijing.maas.aliyuncs.com/compatible-mode/v1"
model = "deepseek-v4-flash-0731"
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

Orally currently supports executable-local configuration without registering as
a Windows input method. Configuration path precedence is an explicit
`ORALLY_CONFIG` path first, then an existing `config.toml` beside `Orally.exe`,
then `%APPDATA%\Orally\config.toml`.

This is a config-local prototype, not the complete Portable Installation
contract defined by the deferred product design.

Build a portable folder:

```powershell
.\scripts\package-portable.ps1
```

The script writes:

```text
dist\portable\Orally\Orally.exe
dist\portable\Orally\config.toml
dist\portable\Orally\config.example.toml
```

Run `Orally.exe` from that folder to keep settings local to the portable
directory. You can also create the same executable-directory config from the CLI:

```powershell
cargo run -p orally-cli -- config init --provider aliyun-openai --portable
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
