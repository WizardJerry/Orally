# Privacy And Permissions

## Privacy Model

Orally should default to local storage and explicit provider configuration.

- Audio should be discarded after processing unless the user enables retention.
- Transcripts and final text should be stored locally by default.
- API keys should be stored in the platform secret store.
- Portable mode should support encrypted local secrets protected by a user
  passphrase.
- Privacy mode should block all network providers and sync adapters.

## Data Classes

| Data | Default | Sync |
| --- | --- | --- |
| Settings | Local config | Optional |
| Prompt profiles | Local database or files | Optional |
| Dictionary | Local database | Optional |
| History | Local SQLite | Off by default |
| Audio cache | Temporary only | Never by default |
| API keys | System secret store | Never |

The current CLI configuration stores only the API key environment variable name,
not the secret value. This keeps early configuration files safe enough for local
sync experiments while leaving room for platform keychain integration later.

## Current Desktop Controls

- `privacy.allow_external_requests`: when false, the desktop dictation flow
  blocks remote ASR and AI post-processing calls.
- `privacy.history_enabled`: when true, Orally stores local JSONL history beside
  the active config file unless `privacy.history_path` is set.
- `output.restore_clipboard`: when true, Orally restores the previous text
  clipboard after paste so generated dictation text does not remain in the
  clipboard.

The current clipboard restoration preserves prior text clipboard content. Full
preservation of images, files, and rich clipboard formats remains future work.

## Permissions

| Permission | Required | Reason |
| --- | --- | --- |
| Microphone | Yes | Capture speech. |
| Network | Optional | Cloud ASR, LLM providers, and sync. |
| Accessibility | Optional | Desktop current-cursor insertion and selected-text editing. |
| Clipboard | Optional | Fallback insertion path. |
| Android overlay | Optional | Floating input control. |
| Android accessibility | Optional | Enhanced insertion in difficult apps. |
| Start at login | Optional | Always-ready dictation. |

Each permission request should explain the feature it enables and the degraded
behavior when it is disabled.

## Sync Direction

The first sync implementation should avoid a proprietary account system:

- Folder sync for iCloud Drive, OneDrive, Dropbox, or Syncthing.
- WebDAV for self-hosted users.
- End-to-end encrypted bundles for settings, prompts, and dictionaries.
- History sync disabled by default, with favorites-only sync as a safer option.
