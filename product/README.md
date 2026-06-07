# Orally Product Definition

Orally is a cross-platform AI voice input method for people who want to speak
naturally and receive polished text in the place they are already writing.

## Positioning

Orally is not only a recorder or transcription app. It is a system-level input
layer:

1. The user triggers recording from the current writing context.
2. Speech is transcribed by a local model or a user-configured API.
3. The transcript is cleaned by an AI post-processing pipeline.
4. The final text is inserted at the current cursor or copied as a fallback.

## Target Users

- Knowledge workers writing messages, notes, emails, and documents.
- Developers who want voice-to-text with technical vocabulary and code terms.
- Multilingual users who mix Chinese, English, and domain-specific terms.
- Privacy-conscious users who want local storage and bring-your-own API keys.

## Product Principles

- Input first: speaking into the current text field should be the primary path.
- Local first: history, prompts, dictionaries, and settings stay local by default.
- User controlled: ASR providers, LLM providers, prompts, and dictionaries are
  configurable.
- Minimal permissions: each permission must map to a clear feature.
- Platform native: shared core behavior, native input integration per operating
  system.

## MVP Scope

- Press-to-talk recording trigger.
- Voice activity detection abstraction.
- ASR provider abstraction.
- LLM/text post-processing abstraction.
- Custom prompt profiles.
- Personal dictionary.
- Local history.
- Privacy mode that disables external requests.
- Current-cursor insertion with clipboard fallback.

## Windows MVP Shape

The first usable Windows package should ship as a tray-resident desktop app
before becoming a formal TSF input method. The tray app can be portable or
installed for the current user, owns the global hotkey, records arbitrary-length
speech with the same start/stop hotkey, calls the configured ASR/post-processing
pipeline, and inserts text through the current clipboard paste adapter.

The settings window should stay simple and Material-style: provider, optional
local API key, API key environment reference, hotkey, recording delay, insertion
behavior, ASR prompt, raw-output mode, post-processing options, and
privacy/storage status. A TSF IME can later reuse the same core pipeline and
configuration while adding native input-method registration and composition
support.

The tray MVP should provide explicit in-progress feedback. When dictation is
active, Orally shows a compact bottom overlay with a recording indicator and a
mouse-click stop button, while preserving the original target window for final
text insertion.

For the first portable Windows build, Orally stays a normal background tray app
rather than registering as an input method. Portable mode is activated by placing
`config.toml` beside `Orally.exe`; this keeps API settings, hotkey presets, and
output behavior local to the portable folder.

## Differentiation

Compared with closed dictation tools, Orally should focus on:

- Custom API endpoints.
- Custom post-processing prompts.
- Local-first storage.
- Portable or current-user installation on Windows.
- Cross-platform architecture.
- Clear permission boundaries.
- Optional WebDAV, iCloud Drive, or OneDrive sync without a required account
  system.
