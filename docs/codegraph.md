# Codegraph Notes

This repository is prepared for later codegraph-based structure tracking.

## Current Structure

- `Cargo.toml` declares the Rust workspace.
- `crates/orally-core/src/lib.rs` contains the first domain model and pipeline.
- `crates/orally-audio/src/lib.rs` contains microphone capture, PCM16 metrics,
  and WAV encoding.
- `crates/orally-asr/src/lib.rs` contains OpenAI multipart transcription and
  chat-completions audio providers.
- `crates/orally-config/src/lib.rs` contains TOML config presets and load/save
  helpers.
- `crates/orally-windows/src/lib.rs` contains Windows clipboard paste insertion
  and foreground global hotkey listening.
- `apps/orally-cli/src/main.rs` records audio, calls ASR providers, and passes
  transcripts through the built-in post-processing pipeline.
- `apps/orally-desktop/src-tauri/src/main.rs` exposes Tauri config commands and
  owns the tray icon/window lifecycle, Windows dictation hotkey service, overlay
  state, ASR call, post-processing, and paste insertion.
- `apps/orally-desktop/ui/` contains the Material-style settings frontend.
- `apps/orally-desktop/ui/overlay.html` contains the bottom dictation prompt and
  stop button.
- `apps/windows-ime/src/lib.rs` contains the placeholder Windows TSF DLL exports.
- `product/` contains product definition and implementation planning.

## Tracking Intent

When codegraph tooling is available in the active session, use it to record:

- crate boundaries
- trait implementations
- platform-specific adapters
- provider implementations
- data flow from audio capture to text insertion

## Architectural Anchors

- `AsrProvider`
- `TextProcessor`
- `TextInserter`
- `OrallyPipeline`
- `CpalAudioRecorder`
- `RecordingConfig`
- `AudioMetrics`
- `OpenAiCompatibleAsrProvider`
- `OpenAiCompatibleAsrConfig`
- `ChatAudioAsrProvider`
- `ChatAudioAsrConfig`
- `AppConfig`
- `ProviderPreset`
- `set_value`
- `get_config_path`
- `get_config`
- `save_config`
- `stop_recording`
- `build_tray`
- `show_settings`
- `start_dictation_service`
- `run_dictation_processor`
- `finish_dictation`
- `show_recording_overlay`
- `capture_foreground_window`
- `restore_foreground_window`
- `WindowsClipboardPasteInserter`
- `WindowsPasteConfig`
- `Hotkey`
- `run_hotkey_loop`
- `DllRegisterServer`
- `DllGetClassObject`
- `AudioInput`
- `Transcript`
- `ProcessedText`
