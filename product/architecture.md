# Orally Architecture

## Shape

Orally should be built as a shared Rust core with platform-native shells.

```text
platform trigger -> audio capture -> VAD -> ASR -> postprocess -> insert text
                                             |        |
                                             |        +-> dictionary/context
                                             +----------> local history
```

## Repository Layout

```text
crates/
  orally-core/       domain types and processing pipeline
  orally-audio/      microphone capture, VAD metrics, WAV encoding
  orally-asr/        OpenAI-compatible ASR provider
  orally-config/     local TOML configuration
  orally-llm/        OpenAI-compatible AI post-processing provider
  orally-storage/    local JSONL history
  orally-windows/    Windows clipboard paste insertion prototype
  orally-sync/       future WebDAV/folder sync
apps/
  orally-cli/        current prototype runner
  orally-desktop/    Tauri tray app and settings UI
  windows-ime/       future TSF text service DLL
  android/           future Kotlin InputMethodService
product/             product and planning documents
docs/                code structure notes
```

## Platform Plan

| Platform | Shell | Notes |
| --- | --- | --- |
| Windows | Rust tray app plus `windows-rs` | Start with hotkey and text insertion. Later explore TSF for deeper input method integration. |
| macOS | Swift/SwiftUI plus Rust FFI | Menu bar app, microphone permission, optional Accessibility for insertion. |
| Linux | Rust daemon plus IBus/Fcitx5 | Support X11 first. Wayland requires compositor-aware compatibility choices. |
| Android | Kotlin plus Rust over JNI/UniFFI | Use `InputMethodService` as the primary input path. |

## Core Interfaces

The core crate currently defines these important traits:

- `AsrProvider`: converts audio into a transcript.
- `TextProcessor`: converts raw transcript into polished text.
- `TextInserter`: sends the processed text to a target.
- `OrallyPipeline`: coordinates the end-to-end flow.

This keeps the first prototype small while leaving room for local Whisper,
OpenAI-compatible ASR, custom LLM providers, and OS-specific insertion engines.

## AI Post-Processing

`orally-llm` implements the first AI post-processing provider using an
OpenAI-compatible chat completions endpoint. The desktop app selects it when
`[postprocess].mode` is `llm` or `ai`; otherwise it uses the built-in local
cleaner. The LLM prompt is configured through `[postprocess].system_prompt` and
`[postprocess].user_template`, with `{{transcript}}` and `{{locale}}`
placeholders.

## Audio Input Prototype

The current audio layer uses CPAL for cross-platform microphone capture. It
records from the default input device into PCM16, computes basic signal metrics,
and can write a WAV file for manual inspection or later ASR submission.

Current validation command:

```powershell
cargo run -p orally-cli -- record --seconds 3 --output orally-recording.wav
```

This is intentionally separate from ASR. The next step is to pass the resulting
`AudioInput { format: Pcm16 }` into a real transcription provider.

## ASR Provider Prototype

`orally-asr` implements two early ASR request protocols.

OpenAI transcription protocol accepts PCM16 or WAV input and sends multipart
audio to:

```text
{base_url}/audio/transcriptions
```

Chat audio protocol accepts PCM16 or WAV input, base64-encodes it, and sends JSON
audio content to:

```text
{base_url}/chat/completions
```

OpenRouter audio input models use the chat audio protocol.

DashScope Qwen ASR also uses the chat audio protocol, but expects
`input_audio.data` to be a data URL such as `data:audio/wav;base64,...`.

The first CLI surfaces are:

```powershell
cargo run -p orally-cli -- transcribe --file orally-recording.wav --model <model>
cargo run -p orally-cli -- dictate --seconds 3 --model <model>
```

API keys are read from environment variables by default, not from command-line
arguments.

## Windows Insertion Prototype

The first Windows insertion path uses the clipboard plus synthetic `Ctrl+V`.
This avoids installer-level input method registration and keeps the prototype
portable. The CLI exposes it through:

```powershell
cargo run -p orally-cli -- dictate --seconds 3 --insert --paste-delay-ms 1200
cargo run -p orally-cli -- listen --paste-delay-ms 300
```

This is a prototype path. A later Windows shell should move the same insertion
adapter behind a tray app and global press-to-talk hotkey.

The clipboard adapter now restores the previous text clipboard content after
paste when `output.restore_clipboard` is enabled. This removes the generated
dictation text from the clipboard after insertion while preserving the previous
text clipboard value. Full non-text clipboard format preservation is still a
future improvement.

The current listener registers `Ctrl+Alt+Space` through the Win32 global hotkey
API and runs in the foreground terminal process. The same hotkey toggles
recording: first press starts capture, second press stops capture and submits
the audio for transcription.

## Desktop Settings Shell

`apps/orally-desktop` is the first Tauri v2 desktop shell. It provides a Windows
tray icon and a Material-style settings window. The UI edits the same local TOML
configuration used by the CLI:

- ASR provider preset, base URL, model, request protocol, optional local API
  key, API key environment variable, language hint, and ASR prompt.
- Hotkey preset, default recording duration, paste delay, output locale, default
  insertion behavior, and change display.
- Raw transcript mode and privacy/storage notes.

The shell can run with only an API key environment variable name, or store an
optional local API key in the user config for portable/testing scenarios. The
next step is to move the existing Windows hotkey listener and clipboard
insertion loop behind this tray process so users can run Orally without a
terminal.

The first tray implementation already owns the Windows `Ctrl+Alt+Space` hotkey.
Recording starts and stops with the same hotkey. While recording, a small
bottom-of-screen Tauri overlay shows the active dictation state and exposes a
mouse-click stop button. Because clicking the overlay moves focus away from the
target app, the tray process records the foreground window at recording start
and restores it before clipboard paste insertion.

The tray app supports a portable layout. If `config.toml` exists beside the
running executable, Orally uses it before the normal user config path. This lets
the Windows portable build run as a background app without registering as an
input method and without writing settings into `%APPDATA%`.

The first hotkey customization surface is preset-based. The selected preset is
stored in `[hotkey].preset`, read when the tray process starts, and used for the
global Win32 hotkey registration. Changing the preset requires restarting the
tray app.

The tray process can append local history through `orally-storage`. By default
history is stored as `history.jsonl` next to the active config file and contains
the raw ASR text plus the final inserted text. `privacy.history_enabled` disables
this, and `privacy.allow_external_requests` blocks remote ASR/LLM calls.

## Windows IME Direction

The formal Windows input method should use Text Services Framework (TSF). The
TSF component should be a thin in-process COM DLL that owns TSF activation,
profile registration, composition, and final text commit. The heavier Orally
workloads should remain in a separate process or shared core: microphone capture,
ASR, post-processing, settings, and history.

Development should not require a full reinstall loop. The planned developer loop
is per-user registration of the current debug DLL, scriptable unregister/register
steps, and debugger attachment to the text input host process.

## Provider Strategy

Providers should be configured through a common schema:

```toml
[providers.asr.default]
type = "openai-compatible-asr"
base_url = "https://api.example.com/v1"
model = "transcribe-model"
api_key_ref = "example"

[providers.llm.default]
type = "openai-compatible-chat"
base_url = "https://api.example.com/v1"
model = "cleanup-model"
api_key_ref = "example"
```

Local providers should use the same internal interface so users can switch
between cloud and local execution without changing the rest of the app.

The current CLI reads this shape from a local TOML file. On Windows the default
path is `%APPDATA%\Orally\config.toml`; `ORALLY_CONFIG` can point to a portable
or test-specific config file.
