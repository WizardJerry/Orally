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
cargo run -p orally-cli -- config set output.paste_delay_ms 300
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
- `output.locale`
- `output.raw`
- `output.show_changes`
- `output.insert`
- `output.paste_delay_ms`
- `audio.dictate_seconds`
- `audio.record_output`
- `hotkey.preset`

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

The settings UI can edit:

- API provider preset, base URL, model, protocol, API key, API key environment
  variable, language hint, and ASR prompt.
- Hotkey preset, default recording duration, paste delay, output locale, default
  insert behavior, and change display.
- Raw transcript mode and privacy/storage notes.

Saving writes the same local config used by the CLI, usually
`%APPDATA%\Orally\config.toml` on Windows. Leave the API Key field blank to use
only an environment variable reference.

## Direction

Orally will use a shared Rust core with platform-native shells:

- Windows: tray app, hotkey, text insertion, later TSF integration.
- macOS: menu bar app, Accessibility/InputMethodKit where appropriate.
- Linux: daemon plus IBus/Fcitx5 integration.
- Android: Kotlin `InputMethodService` with Rust core over JNI/UniFFI.
