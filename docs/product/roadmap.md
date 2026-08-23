# Orally Roadmap

## Phase 0: Core Prototype

- Rust workspace.
- Shared domain types.
- Minimal ASR, post-processing, and insertion traits.
- CLI demo.
- Product and architecture documents.
- Completed: microphone recording to WAV.
- Completed: remote ASR providers for multipart transcription and chat-audio
  style APIs.
- Completed: ASR output flows through built-in post-processing by default.
- Completed: Windows clipboard paste insertion prototype.
- Completed: Windows foreground global hotkey listener with same-key start/stop
  recording.
- Started: Windows TSF IME DLL scaffold and development notes.
- Completed: local TOML configuration with provider presets.
- Completed: initial Tauri desktop settings shell with Windows tray icon.
- Completed: tray-owned Windows dictation hotkey with bottom overlay and stop
  button.
- Completed: portable Windows app layout using executable-directory
  `config.toml`.
- Replace the prototype hotkey preset list with direct shortcut capture,
  immediate conflict validation, and registration without restarting.
- Completed: tray menu controls for start/stop dictation and pause/resume
  hotkey.
- Completed: OpenAI-compatible AI post-processing provider.
- Completed: local JSONL history with privacy toggles.
- Completed: text clipboard restoration after paste.

## Phase 1: Useful Desktop MVP

- Audio capture on Windows.
- OpenAI-compatible ASR provider.
- Chat-audio ASR provider for OpenRouter and DashScope Qwen ASR.
- Clipboard fallback insertion.
- Press-to-talk global hotkey.
- Completed: local history.
- Completed: OpenAI-compatible LLM post-processing provider.
- Completed: basic settings UI.
- Completed: settings UI backed by the local configuration file.
- Completed: tray-resident Windows dictation loop using the existing same-key
  start/stop hotkey path.
- Completed: portable package script for tray-app mode without IME
  registration.
- Completed: privacy switches for external requests and local history.
- TSF text service COM class factory and per-user registration scripts.

## Phase 2: Android IME

- Kotlin `InputMethodService`.
- Press-and-hold microphone key.
- Provider configuration.
- Workflow support.
- Optional floating control.

## Phase 3: Local-First Power Features

- Local Whisper or sherpa-onnx provider.
- Privacy mode enforcement.
- App-specific Workflow selection.
- Optional personal dictionary and import/export, deferred from the Windows MVP.
- Local encrypted backup.

## Phase 4: Cross-Platform Shells

- macOS menu bar app and insertion.
- Linux IBus/Fcitx5 integration.
- Windows TSF research prototype.
- Shared desktop settings UI.

## Phase 5: Sync And Extensibility

- Folder sync.
- WebDAV sync.
- Prompt module conflict resolution.
- Provider plugin API.
- Codegraph-assisted architecture tracking.
