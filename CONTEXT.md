# Orally Product

Orally is a voice-input product that turns natural speech into polished text in the user's current writing context.

## Language

**Windows Desktop App**:
The primary end-user product: a portable Windows application for triggering voice input, reviewing its status, and configuring its behavior.
_Avoid_: Windows IME, CLI, prototype runner

**Portable Installation**:
A Windows Desktop App directory that carries all persistent Orally data—including Config, Service Connections, Workflows, Modules, Local History, and retained audio—when copied to another Windows computer. Only temporary processing files may live elsewhere, and the Windows MVP does not create a system startup entry.
_Avoid_: config-only portable mode

**CLI**:
A developer-facing tool for exercising, inspecting, and troubleshooting Orally capabilities outside the end-user product.
_Avoid_: desktop app, end-user interface

**Windows IME**:
A possible future native Windows input-method integration; it is not the current product.
_Avoid_: Windows Desktop App

**Toggle Mode**:
The primary recording interaction in which one trigger starts voice input and the next trigger ends it.
_Avoid_: hold-to-talk, fixed-duration recording

**Global Trigger**:
The user-recorded Windows key combination that starts and stops Toggle Mode. It is validated and registered immediately when saved; Orally does not constrain it to a preset list.
_Avoid_: hotkey preset

**Automatic Stop**:
An optional recording behavior that ends voice input after sustained silence. It is disabled by default so Toggle Mode remains explicitly controlled by the user.
_Avoid_: default stop behavior

**Direct Insertion**:
The primary delivery behavior in which Orally places the final text into the writing target that was active when voice input started.
_Avoid_: preview-first output, transcript-only output

**Insertion Recovery**:
The fallback state in which generated text remains on the clipboard and in Local History after Direct Insertion fails, with actions to retry insertion or open history.
_Avoid_: successful insertion, transcript retry

**Status Overlay**:
A compact always-on-top indicator for recording and processing state and available actions. It never displays the raw transcript or final text.
_Avoid_: result preview, history view

**AI Post-processing**:
The preferred text-refinement path that turns a raw transcript into paste-ready text while preserving the speaker's meaning.
_Avoid_: transcription, local basic cleanup

**Local Basic Cleanup**:
The degraded text-refinement path used when AI post-processing is unavailable, disallowed, or fails. A fallback result is still inserted, and the user is informed that the preferred refinement path was not used.
_Avoid_: AI post-processing

**Local History**:
A user-controlled local record of recent voice-input results. It is enabled by default, retains the latest ten final texts, timestamps, Workflow identities, and processing states, and allows the user to disable it or choose a larger retention count; retaining raw transcripts is a separate opt-in behavior.
_Avoid_: audio archive, cloud history

**Audio Retention**:
A disabled-by-default behavior that persists voice-input audio as an attachment to its Local History entry. Retained audio follows the same deletion and retention-count rules as that entry.
_Avoid_: temporary in-memory audio

**Audio Segment**:
A temporary WAV fragment written progressively for each minute of a long recording and sent to speech recognition in order. Segments are deleted after success, discard, or exit unless Audio Retention moves them into the related history attachment.
_Avoid_: retained recording, transcript segment

**First-run Setup**:
The guided configuration required when the Windows Desktop App starts without a valid existing configuration. It validates the speech service and microphone, offers AI post-processing setup, and enables the global trigger only after completion.
_Avoid_: settings screen, normal startup

**Provider Credential**:
The API secret used to access a configured speech or AI service. The current portable product stores it as plaintext in the local SQLite database only after an explicit risk acknowledgement; it is masked in the interface and excluded from logs, history, and shareable TOML files. Application-managed, cross-platform encryption is a future upgrade.
_Avoid_: provider preset, account

**External-request Blocking**:
A privacy control that prevents voice input from starting when external requests are disabled and no local speech service is available.
_Avoid_: offline transcription

**Pending Voice Input**:
A completed recording held temporarily in memory after speech recognition fails or processing is cancelled so the user can retry without speaking again. It is cleared after success, explicit discard, a new recording, or application exit.
_Avoid_: Local History, Audio Retention

**Pending Refinement**:
A raw transcript held temporarily during the current application run after AI post-processing falls back, allowing an immediate retry. It remains available from Local History only when raw-transcript retention is enabled.
_Avoid_: final text, Local History entry

**Workflow**:
A named, user-adjustable definition of the complete voice-input process for a scenario. It requires one speech-recognition stage and may add one AI post-processing request assembled from an ordered selection of Modules; without that request, Local Basic Cleanup prepares the transcript for insertion.
_Avoid_: configuration, config, document, preset, voice profile

**Built-in Workflow**:
A read-only, non-deletable Workflow supplied and replaced by Orally during software updates. Customization requires copying it into a Custom Workflow.
_Avoid_: built-in Module, custom Workflow

**Custom Workflow**:
A user-created Workflow that can be adjusted and deleted.
_Avoid_: built-in Workflow, Module

**Config**:
A software-level setting that controls application behavior, such as a hotkey, window preference, privacy choice, or default limit.
_Avoid_: Workflow, Module

**Speech-recognition Stage**:
The single first stage of a Workflow, defined by its speech connection, model, and one ASR Prompt that supplies recognition language, vocabulary, or context. It turns recorded audio into a raw transcript but does not define final formatting or translation.
_Avoid_: text refinement, AI post-processing

**Module**:
A reusable static Prompt instruction stored as one TOML file and included in a Workflow's ordered post-processing prompt. Built-in Modules live in the program-managed, read-only `module/builtin/` directory; user-created Modules live in editable `module/custom/`. Workflows keep live references to Modules, so editing a custom Module updates every Workflow that uses it; deleting a referenced Module requires removing or replacing every reference. Built-in Modules can be copied under a new Module Identifier for customization. All selected Modules are composed into one AI post-processing request, while parameterized Modules are a future extension.
_Avoid_: node, Prompt Module, LLM stage, model, request

**Module Identifier**:
A required, immutable, unique lowercase ASCII kebab-case name stored alongside the editable localized display name and used as the Module's TOML filename and Workflow reference. Changing it requires copying the Module under a new identifier.
_Avoid_: display name, Workflow name

**Composed Prompt**:
The single post-processing prompt assembled from Orally's non-overridable base rules, the Active Workflow's ordered Modules, and one copy of the raw transcript. Later Modules take precedence over earlier Modules when their user instructions conflict.
_Avoid_: Module, raw transcript

**Output-language Module**:
A Module that overrides the default language-preservation behavior and requires all translatable natural-language content to use one target language while leaving code, commands, URLs, paths, variables, and other non-translatable identifiers intact.
_Avoid_: language detection, ASR language hint

**Service Connection**:
A reusable provider connection containing an API endpoint, protocol, and Provider Credential. Workflows reference connections while selecting their own ASR or LLM model and instructions.
_Avoid_: Workflow, model, Module

**Default Service Slot**:
A global binding from the default ASR or default LLM role to a user-selected Service Connection and model. Built-in Workflows use these slots; Custom Workflows may inherit them or select explicit services.
_Avoid_: Service Connection, Workflow

**Active Workflow**:
The one Workflow selected for the next global voice-input trigger. It can be changed from the tray or settings, and its name remains visible in the tray interface.
_Avoid_: default provider, global configuration
