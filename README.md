# Orally

Orally is an early prototype for a cross-platform AI voice input layer.

The intended product is a privacy-first dictation tool that works across Windows,
macOS, Linux, and Android. Users should be able to press a hotkey or use a system
input method, speak naturally, and receive polished text at the current cursor.

## Current Prototype

This repository currently contains:

- `crates/orally-core`: shared Rust domain types and a minimal speech-to-text
  post-processing pipeline.
- `crates/orally-audio`: cross-platform microphone capture, PCM16 metrics, and
  WAV encoding.
- `apps/orally-cli`: a small command-line demo that simulates ASR output and
  runs it through the post-processing pipeline, plus an audio recording command.
- `apps/orally-desktop`: a Tauri desktop shell with a Windows tray icon and a
  Material-style settings UI backed by the local TOML config.
- `product`: product definition, architecture notes, privacy model, and roadmap.
- `docs/codegraph.md`: code structure notes intended to help future codegraph
  indexing and review.

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
cargo run -p orally-cli -- config init --provider dashscope
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

For Alibaba Cloud Model Studio/DashScope Qwen ASR:

```powershell
$env:ORALLY_ASR_API_KEY="..."
$env:ORALLY_ASR_BASE_URL="https://dashscope.aliyuncs.com/compatible-mode/v1"
$env:ORALLY_ASR_MODEL="qwen3-asr-flash"
$env:ORALLY_ASR_PROTOCOL="chat-audio"
cargo run -p orally-cli -- transcribe --file orally-recording.wav
```

DashScope chat audio requests use `input_audio.data` with a WAV data URL.

## Configuration

Orally can load local configuration from `%APPDATA%\Orally\config.toml` on
Windows. Set `ORALLY_CONFIG` to use a custom path.

Initialize a provider preset:

```powershell
cargo run -p orally-cli -- config init --provider dashscope
```

Supported presets:

- `dashscope`
- `openrouter`
- `openai`

Show the active config:

```powershell
cargo run -p orally-cli -- config show
```

Update one config value:

```powershell
cargo run -p orally-cli -- config set asr.model qwen3-asr-flash
cargo run -p orally-cli -- config set asr.api_key "sk-..."
cargo run -p orally-cli -- config set asr.api_key_env DASHSCOPE_API_KEY
cargo run -p orally-cli -- config set postprocess.mode llm
cargo run -p orally-cli -- config set postprocess.model gpt-4o-mini
cargo run -p orally-cli -- config set postprocess.fallback_to_builtin true
cargo run -p orally-cli -- config set audio.input_mode hold
cargo run -p orally-cli -- config set audio.auto_stop_enabled true
cargo run -p orally-cli -- config set audio.silence_timeout_ms 1200
cargo run -p orally-cli -- config set output.paste_delay_ms 300
cargo run -p orally-cli -- config set output.restore_clipboard true
cargo run -p orally-cli -- config set output.show_changes true
```

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
the key in your shell or OS environment. For DashScope, set:

```powershell
$env:DASHSCOPE_API_KEY="..."
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

The desktop app also registers `Ctrl+Alt+Space` as the Windows dictation hotkey.
Press once to start recording. A small always-on-top prompt appears near the
bottom of the screen. Press the same hotkey again, or click `停止`, to stop
recording, transcribe, post-process, and paste the final text back into the
window that was active when recording started.

The desktop hotkey supports three input modes through `audio.input_mode`:

- `toggle`: press once to start and press again to stop.
- `hold`: hold the hotkey while speaking and release it to transcribe.
- `fixed-window`: press once and let Orally stop after `audio.dictate_seconds`.

When `audio.auto_stop_enabled = true`, Orally also watches recent voice activity
and stops automatically after `audio.silence_timeout_ms` of silence, bounded by
`audio.min_record_ms` and `audio.max_record_ms`.

The tray menu can also start or stop dictation, pause or resume the hotkey, open
settings, and quit Orally.

Supported hotkey presets:

- `ctrl-alt-space`
- `ctrl-shift-space`
- `alt-space`
- `f9`
- `f10`
- `f11`
- `f12`
- `ctrl-alt-f9`
- `ctrl-alt-f10`
- `ctrl-alt-f11`
- `ctrl-alt-f12`

Hotkey changes are read when Orally starts, so restart the app after saving a new
hotkey.

The settings UI can edit:

- API provider preset, base URL, model, protocol, API key, API key environment
  variable, language hint, and ASR prompt.
- AI post-processing mode, OpenAI-compatible LLM endpoint/model/API key, system
  prompt, and user template.
- Hotkey preset, default recording duration, paste delay, output locale, default
  insert behavior, change display, and clipboard restoration.
- Raw transcript mode, external-request privacy switch, and local history.

Saving writes the same local config used by the CLI, usually
`%APPDATA%\Orally\config.toml` on Windows. Leave the API Key field blank to use
only an environment variable reference.

## AI Post-Processing

By default Orally uses the built-in local cleaner. Set:

```toml
[postprocess]
mode = "llm"
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
api_key_env = "ORALLY_LLM_API_KEY"
```

The AI postprocessor sends the ASR transcript to an OpenAI-compatible
`/chat/completions` endpoint and expects only the final text in response. Use
`postprocess.system_prompt` and `postprocess.user_template` to customize the
cleanup behavior. The user template supports `{{transcript}}` and `{{locale}}`.

Set `output.raw = true` to bypass both built-in and AI post-processing.

If `postprocess.fallback_to_builtin = true`, a remote AI post-processing failure
falls back to Orally's local cleaner so the dictation can still complete.

## Local History And Clipboard Privacy

If `privacy.history_enabled = true`, Orally appends local JSONL history beside
the active config file as `history.jsonl`. Each entry stores the raw ASR text and
the final inserted text. Set `privacy.history_path` to use a custom file, or set
`privacy.history_enabled = false` to disable history.

Set `privacy.allow_external_requests = false` to block remote ASR and AI
post-processing requests.

On Windows, Orally uses the clipboard fallback insertion path. When
`output.restore_clipboard = true`, Orally restores the previous text clipboard
after paste and removes the generated text from the clipboard. This first
implementation preserves previous text clipboard content; non-text clipboard
formats are not restored yet.

## Portable Windows App

Orally supports a portable mode without registering as a Windows input method.
If a `config.toml` file exists beside `Orally.exe`, Orally reads that file before
falling back to `%APPDATA%\Orally\config.toml`.

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
cargo run -p orally-cli -- config init --provider dashscope --portable
```

The packaging script does not overwrite an existing `config.toml`; it always
writes the latest `config.example.toml` for reference.

## Direction

Orally will use a shared Rust core with platform-native shells:

- Windows: tray app, hotkey, text insertion, later TSF integration.
- macOS: menu bar app, Accessibility/InputMethodKit where appropriate.
- Linux: daemon plus IBus/Fcitx5 integration.
- Android: Kotlin `InputMethodService` with Rust core over JNI/UniFFI.
