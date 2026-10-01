# Windows IME Development

[English](windows-ime.md) | [简体中文](windows-ime.zh-CN.md)

> Status: Deferred
>
> Document type: Research note
>
> Windows TSF work is not part of the active backend-first refactor. Re-read the
> [Refactor Architecture Baseline](refactor-baseline.md) before activating this
> work.

Orally's current Windows prototype uses a foreground process, a global hotkey,
the clipboard, and synthetic `Ctrl+V`. A real Windows input method should be a
Text Services Framework (TSF) text service.

## Target Shape

```text
TSF text service DLL
  -> receives activation and composition callbacks
  -> talks to Orally background service
  -> commits final text into the focused TSF context

Orally background service
  -> records audio
  -> calls ASR
  -> runs post-processing
  -> stores settings/history
```

The TSF DLL should stay thin. Audio capture, network providers, prompts, history,
and settings should remain outside the DLL so that the input method integration
is easier to debug and less likely to destabilize host applications.

## Debugging Without Reinstalling

During development you should not repeatedly run a full installer.

Recommended loop:

1. Build the TSF DLL.
2. Register the debug build per user.
3. Restart the text input host or sign out/in if needed.
4. Select the Orally input method from the language/input switcher.
5. Attach Visual Studio, WinDbg, or another debugger to the host process.
6. Rebuild, unregister, and register the new debug DLL when COM/TSF exports
   change.

The final installer should do machine-level or user-level registration, but the
development loop should be scriptable.

## Registration Model

A TSF input method has two layers of registration:

- COM in-process server registration for the DLL.
- TSF text service/profile registration through input processor profile APIs.

Microsoft's TSF registration docs describe this as standard COM registration
plus TSF registration through `ITfInputProcessorProfiles::Register`.

For development, prefer current-user registration under `HKCU` where possible.
That limits registration to the current user and avoids requiring administrator
rights for every edit. Registry-based registration is not Portable
Installation data.

## Current Repository State

- `crates/orally-windows`: foreground hotkey and clipboard paste prototype.
- `apps/windows-ime`: placeholder TSF DLL crate exporting standard COM DLL entry
  points. It is not a working input method yet.

## Research Checklist For A Future Review

1. Implement COM class factory for the TSF text service.
2. Implement `ITfTextInputProcessorEx::ActivateEx` and shutdown paths.
3. Add per-user registration scripts.
4. Add a minimal language profile so Windows can list Orally as an input method.
5. Commit fixed text into the current TSF context.
6. Bridge TSF trigger events to the existing Orally background pipeline.

## Useful Commands

Build the placeholder DLL:

```powershell
cargo build -p orally-windows-ime
```

The registration exports are stubs for now, so do not use `regsvr32` until the
registration TODOs are implemented.
