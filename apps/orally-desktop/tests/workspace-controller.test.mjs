import test from "node:test";
import assert from "node:assert/strict";
import { ConfigEditor, DEFAULT_CONFIG } from "../ui/editor-state.js";
import { PREVIEW_STORAGE_KEY, WorkspaceController } from "../ui/workspace-controller.js";

const tick = () => new Promise((resolve) => setImmediate(resolve));
const clone = (value) => structuredClone(value);

test("output preferences auto-save and survive switching methods and reloading", async () => {
  const storage = memoryStorage(JSON.stringify(savedConfig()));
  const controller = new WorkspaceController({ storage });
  await controller.load();
  controller.editSettings((config) => { config.output.auto_copy = true; });
  await controller.settingsTask;
  controller.editSettings((config) => { config.output.direct_output = false; });
  await controller.settingsTask;
  await controller.load();
  assert.equal(controller.snapshot().output.direct_output, false);
  assert.equal(controller.snapshot().output.auto_copy, true);
  controller.editSettings((config) => { config.output.direct_output = true; });
  await controller.settingsTask;
  await controller.load();
  assert.equal(controller.snapshot().output.auto_copy, true);
});

test("settings auto-save independently while model credentials and an unsaved selection remain drafts", async () => {
  const storage = memoryStorage(JSON.stringify(savedConfig()));
  const controller = new WorkspaceController({ storage });
  await controller.load();
  controller.edit((editor) => { editor.activeProfile.asr.api_key = "fake-draft-key"; });
  const draft = controller.newProfile();
  controller.editSettings((config) => { config.appearance.dark_mode = false; });
  await controller.settingsTask;
  const stored = JSON.parse(storage.getItem(PREVIEW_STORAGE_KEY));
  assert.equal(stored.appearance.dark_mode, false);
  assert.equal(stored.profiles.length, 2);
  assert.ok(!JSON.stringify(stored).includes("fake-draft-key"));
  assert.equal(stored.active_profile_id, "a");
  assert.equal(controller.snapshot().active_profile_id, draft.id);
  assert.equal(controller.state.editor.dirty, true);
  controller.queueConfigSync();
  await tick();
  assert.equal(controller.snapshot().active_profile_id, draft.id, "own setting writes cannot switch away from a draft");
});

test("rapid setting edits coalesce and writes are serialized without credentials", async () => {
  const first = deferred();
  const second = deferred();
  const writes = [];
  const controller = new WorkspaceController({ invoke: async (command, args) => {
    if (command === "get_config") return savedConfig();
    assert.equal(command, "save_settings");
    writes.push(args);
    return writes.length === 1 ? first.promise : second.promise;
  } });
  await controller.load();
  controller.editSettings((config) => { config.appearance.dark_mode = false; });
  controller.editSettings((config) => { config.output.locale = "en-US"; });
  await tick();
  assert.equal(writes.length, 1);
  assert.equal(writes[0].settings.appearance.dark_mode, false);
  assert.equal(writes[0].settings.output.locale, "en-US");
  assert.ok(!("profiles" in writes[0].settings));
  controller.editSettings((config) => { config.appearance.dark_mode = true; });
  await tick();
  assert.equal(writes.length, 1);
  first.resolve();
  await tick();
  assert.equal(writes.length, 2);
  assert.equal(writes[1].settings.appearance.dark_mode, true);
  assert.equal(controller.state.settingsSaving, true);
  second.resolve();
  await controller.settingsTask;
  assert.equal(controller.state.settingsSaving, false);
  assert.equal(controller.state.editor.dirty, false);
});

test("a failed setting write restores failed values and still saves later independent edits", async () => {
  const first = deferred();
  const writes = [];
  const controller = new WorkspaceController({ invoke: async (command, args) => {
    if (command === "get_config") return savedConfig();
    writes.push(args.settings);
    if (writes.length === 1) return first.promise;
  } });
  await controller.load();
  controller.editSettings((config) => { config.hotkey.preset = "ctrl-shift-a"; });
  await tick();
  controller.editSettings((config) => { config.appearance.dark_mode = false; });
  first.reject(new Error("快捷键被占用"));
  await controller.settingsTask;
  assert.equal(writes.length, 2);
  assert.equal(writes[1].hotkey.preset, "ctrl-alt-space");
  assert.equal(writes[1].appearance.dark_mode, false);
  assert.equal(controller.snapshot().hotkey.preset, "ctrl-alt-space");
  assert.equal(controller.snapshot().appearance.dark_mode, false);
  assert.equal(controller.state.editor.dirty, false);
});

test("a failed theme write restores its saved theme and reports failure without a false dirty draft", async () => {
  const controller = new WorkspaceController({ invoke: async (command) => {
    if (command === "get_config") return savedConfig();
    throw new Error("disk write failed");
  } });
  await controller.load();
  controller.editSettings((config) => { config.appearance.dark_mode = false; });
  await controller.settingsTask;
  assert.equal(controller.snapshot().appearance.dark_mode, true);
  assert.match(controller.state.settingsError, /disk write failed/);
  assert.equal(controller.state.settingsSaving, false);
  assert.equal(controller.state.editor.dirty, false);
});

test("external settings sync preserves model drafts and does not close a current test context", async () => {
  let saved = savedConfig();
  const controller = new WorkspaceController({ invoke: async () => clone(saved) });
  await controller.load();
  controller.edit((editor) => { editor.activeProfile.asr.model = "unsaved-model"; });
  const context = controller.state.contextVersion;
  saved.appearance.dark_mode = false;
  saved.output.auto_copy = true;
  controller.queueConfigSync();
  await tick();
  assert.equal(controller.snapshot().appearance.dark_mode, false);
  assert.equal(controller.snapshot().output.auto_copy, true);
  assert.equal(controller.snapshot().asr.model, "unsaved-model");
  assert.equal(controller.state.contextVersion, context);
  assert.equal(controller.state.editor.profileDirty("a"), true);
});

test("auto-save merges an external settings edit that the window has not received yet", async () => {
  const storage = memoryStorage(JSON.stringify(savedConfig()));
  const controller = new WorkspaceController({ storage });
  await controller.load();
  const external = savedConfig();
  external.output.locale = "en-GB";
  external.hotkey.preset = "ctrl-shift-a";
  storage.setItem(PREVIEW_STORAGE_KEY, JSON.stringify(external));
  controller.editSettings((config) => { config.appearance.dark_mode = false; });
  await controller.settingsTask;
  const saved = JSON.parse(storage.getItem(PREVIEW_STORAGE_KEY));
  assert.equal(saved.output.locale, "en-GB");
  assert.equal(saved.hotkey.preset, "ctrl-shift-a");
  assert.equal(saved.appearance.dark_mode, false);
  assert.equal(controller.snapshot().output.locale, "en-GB");
  assert.equal(controller.snapshot().hotkey.preset, "ctrl-shift-a");
  assert.equal(controller.state.editor.dirty, false);
});

test("external updates arriving during settings auto-save wait for completion and merge afterward", async () => {
  const gate = deferred();
  let saved = savedConfig();
  let reads = 0;
  const controller = new WorkspaceController({ invoke: async (command, args) => {
    if (command === "get_config") { reads += 1; return clone(saved); }
    await gate.promise;
    Object.assign(saved, args.settings);
    saved.profiles[1].asr.model = "externally-updated";
  } });
  await controller.load();
  controller.editSettings((config) => { config.appearance.dark_mode = false; });
  await tick();
  controller.queueConfigSync();
  await tick();
  assert.equal(reads, 1);
  gate.resolve();
  await controller.settingsTask;
  await tick();
  assert.equal(reads, 2);
  assert.equal(controller.snapshot().profiles[1].asr.model, "externally-updated");
  assert.equal(controller.snapshot().appearance.dark_mode, false);
});

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function savedConfig(active = "a", version = "saved") {
  const config = new ConfigEditor().snapshot();
  const template = config.profiles[0];
  config.profiles = ["a", "b"].map((id) => ({
    ...clone(template), id, name: `Profile ${id}`,
    asr: { ...clone(template.asr), model: `${id}-${version}` },
  }));
  config.active_profile_id = active;
  return new ConfigEditor(config).snapshot();
}

function memoryStorage(initial) {
  const values = new Map(initial === undefined ? [] : [[PREVIEW_STORAGE_KEY, initial]]);
  return {
    reads: [], writes: [],
    getItem(key) { this.reads.push(key); return values.get(key) ?? null; },
    setItem(key, value) { this.writes.push({ key, value }); values.set(key, value); },
  };
}

test("saving acknowledges its captured draft while later profile and settings edits stay dirty", async () => {
  const saving = deferred();
  let submitted;
  const controller = new WorkspaceController({
    invoke: async (command, args) => {
      if (command === "get_config") return savedConfig();
      if (command === "save_config") { submitted = args.config; return saving.promise; }
    },
  });
  await controller.load();
  controller.edit((editor) => { editor.activeProfile.asr.model = "submitted model"; }, null);
  const pending = controller.save();
  controller.edit((editor) => {
    editor.activeProfile.asr.model = "later model";
    editor.config.output.auto_copy = true;
  }, null);
  saving.resolve();
  assert.equal(await pending, true);
  assert.equal(submitted.asr.model, "submitted model");
  assert.equal(submitted.output.auto_copy, false);
  assert.equal(controller.snapshot().asr.model, "later model");
  assert.equal(controller.snapshot().output.auto_copy, true);
  assert.equal(controller.getState().editor.dirty, true);
  assert.equal(controller.getState().editor.saved.asr.model, "submitted model");
});

for (const operation of ["save", "reload", "import"]) {
  test(`tray selection waits for ${operation} then reads the current saved configuration`, async () => {
    const gate = deferred();
    let reads = 0;
    const controller = new WorkspaceController({
      invoke: async (command) => {
        if (command === "save_config") return gate.promise;
        if (command === "get_config") {
          reads += 1;
          if (reads === 1) return savedConfig();
          if (operation === "reload" && reads === 2) return gate.promise;
          return savedConfig("b", "tray");
        }
      },
    });
    await controller.load();
    let pending;
    if (operation === "save") pending = controller.save();
    else if (operation === "reload") pending = controller.load();
    else pending = controller.importFile({ name: "incoming.json", text: () => gate.promise });
    controller.queueSavedProfileSync("b");
    await tick();
    assert.equal(reads, operation === "reload" ? 2 : 1, "tray reads must wait for the active operation");
    if (operation === "import") {
      const profile = clone(savedConfig().profiles[0]);
      profile.name = "Imported draft";
      gate.resolve(JSON.stringify({ format: "orally-profile", version: 1, profile }));
    } else gate.resolve(savedConfig("a", "operation"));
    await pending;
    await tick();
    assert.equal(controller.snapshot().active_profile_id, "b");
    assert.equal(controller.snapshot().asr.model, "b-tray");
    assert.equal(reads, operation === "reload" ? 3 : 2);
    if (operation === "import") {
      assert.ok(controller.snapshot().profiles.some((profile) => profile.name === "Imported draft"));
      assert.equal(controller.getState().editor.dirty, true, "a tray selection must retain the imported unsaved profile");
    }
  });
}

test("a newer tray event supersedes an outstanding read without exposing its stale configuration", async () => {
  const oldRead = deferred();
  const newestRead = deferred();
  let reads = 0;
  const controller = new WorkspaceController({
    invoke: async () => { reads += 1; return reads === 1 ? savedConfig() : reads === 2 ? oldRead.promise : newestRead.promise; },
  });
  await controller.load();
  controller.edit((editor) => { editor.config.output.auto_copy = true; });
  const publishedModels = [];
  controller.subscribe(() => publishedModels.push(controller.snapshot()?.asr.model));
  controller.queueSavedProfileSync("a");
  controller.queueSavedProfileSync("b");
  oldRead.resolve(savedConfig("a", "stale"));
  await tick();
  assert.equal(reads, 3);
  assert.equal(controller.snapshot().asr.model, "a-saved");
  newestRead.resolve(savedConfig("b", "latest"));
  await tick();
  assert.equal(controller.snapshot().asr.model, "b-latest");
  assert.equal(controller.snapshot().output.auto_copy, true);
  assert.equal(controller.getState().editor.dirty, true);
  assert.ok(!publishedModels.includes("a-stale"));
});

test("a tray read started before reload cannot overwrite the reloaded configuration", async () => {
  const oldRead = deferred();
  const reloadRead = deferred();
  let reads = 0;
  const controller = new WorkspaceController({
    invoke: async () => {
      reads += 1;
      if (reads === 1) return savedConfig();
      if (reads === 2) return oldRead.promise;
      if (reads === 3) return reloadRead.promise;
      return savedConfig("b", "latest");
    },
  });
  await controller.load();
  const publishedModels = [];
  controller.subscribe(() => publishedModels.push(controller.snapshot()?.asr.model));
  controller.queueSavedProfileSync("a");
  const reload = controller.load();
  reloadRead.resolve(savedConfig("b", "reloaded"));
  await reload;
  oldRead.resolve(savedConfig("a", "stale"));
  await tick();
  assert.equal(controller.snapshot().asr.model, "b-latest");
  assert.ok(publishedModels.includes("b-reloaded"));
  assert.ok(!publishedModels.includes("a-stale"));
});

test("a failed superseded tray read still processes the newer queued selection", async () => {
  const oldRead = deferred();
  let reads = 0;
  const controller = new WorkspaceController({
    invoke: async () => { reads += 1; return reads === 1 ? savedConfig() : reads === 2 ? oldRead.promise : savedConfig("b", "latest"); },
  });
  await controller.load();
  controller.queueSavedProfileSync("a");
  controller.queueSavedProfileSync("b");
  oldRead.reject(new Error("old IPC read failed"));
  await tick();
  assert.equal(reads, 3, "the new tray event must not remain stranded behind the failed old read");
  assert.equal(controller.snapshot().asr.model, "b-latest");
  assert.equal(controller.getState().status.error, false);
});

test("a current tray read failure waits for another event instead of retrying indefinitely", async () => {
  let reads = 0;
  const controller = new WorkspaceController({
    invoke: async () => {
      reads += 1;
      if (reads === 1) return savedConfig();
      if (reads === 2) throw new Error("current config unavailable");
      return savedConfig("b", "recovered");
    },
  });
  await controller.load();
  controller.queueSavedProfileSync("a");
  await tick();
  await tick();
  assert.equal(reads, 2);
  assert.equal(controller.getState().status.error, true);
  assert.equal(controller.snapshot().asr.model, "a-saved");
  controller.queueSavedProfileSync("b");
  await tick();
  assert.equal(reads, 3);
  assert.equal(controller.snapshot().asr.model, "b-recovered");
});

test("native connection tests capture all unsaved settings and edited services discard late replies", async () => {
  const oldResult = deferred();
  const newResult = deferred();
  const submitted = [];
  const controller = new WorkspaceController({
    invoke: async (command, args) => {
      if (command === "get_config") return savedConfig();
      submitted.push(args);
      return submitted.length === 1 ? oldResult.promise : newResult.promise;
    },
  });
  await controller.load();
  const modelId = controller.getState().editor.activeProfile.postprocess.models[0].id;
  controller.edit((editor) => {
    editor.activeProfile.asr.model = "unsaved speech";
    editor.model(modelId).system_prompt = "unsaved instructions";
    editor.config.output.auto_copy = true;
  }, modelId);
  const first = controller.runConnection(modelId);
  controller.edit((editor) => { editor.model(modelId).model = "new model"; }, modelId);
  const second = controller.runConnection(modelId);
  oldResult.resolve({ message: "old connection", elapsed_ms: 1 });
  await first;
  assert.equal(submitted[0].config.asr.model, "unsaved speech");
  assert.equal(submitted[0].config.postprocess.models[0].system_prompt, "unsaved instructions");
  assert.equal(submitted[0].config.output.auto_copy, true);
  assert.notEqual(submitted[0].config.postprocess.model, "new model");
  assert.equal(submitted[1].config.postprocess.model, "new model");
  assert.equal(controller.connection(modelId).busy, true);
  assert.ok(!controller.connection(modelId).message.includes("old connection"));
  newResult.resolve({ message: "new connection", elapsed_ms: 2 });
  await second;
  assert.match(controller.connection(modelId).message, /new connection/);
});

test("browser service invalidation aborts its request and ignores a late rejection", async () => {
  const response = deferred();
  let signal;
  const controller = new WorkspaceController({
    storage: memoryStorage(JSON.stringify(savedConfig())),
    testConnection: async (_snapshot, _id, options) => { signal = options.signal; return response.promise; },
  });
  await controller.load();
  const pending = controller.runConnection();
  controller.edit((editor) => { editor.activeProfile.asr.base_url = "https://changed.example/v1"; }, null);
  assert.equal(signal.aborted, true);
  response.reject(new Error("late obsolete error"));
  await pending;
  assert.equal(controller.connection(), undefined);
  assert.equal(controller.getState().status, null);
});

test("switching profiles cancels connection tests without moving their results to the new profile", async () => {
  const response = deferred();
  let signal;
  const controller = new WorkspaceController({
    storage: memoryStorage(JSON.stringify(savedConfig())),
    testConnection: async (_snapshot, _id, options) => { signal = options.signal; return response.promise; },
  });
  await controller.load();
  const pending = controller.runConnection();
  controller.selectProfile("b");
  assert.equal(signal.aborted, true);
  response.resolve({ message: "old profile connected", elapsed_ms: 1 });
  await pending;
  assert.equal(controller.snapshot().active_profile_id, "b");
  assert.equal(controller.connection(), undefined);
  controller.selectProfile("a");
  assert.equal(controller.connection(), undefined);
});

test("browser preview reads and writes the existing storage key and accepts its legacy single profile", async () => {
  const legacy = clone(DEFAULT_CONFIG);
  delete legacy.profiles;
  delete legacy.active_profile_id;
  legacy.asr.model = "previous preview model";
  legacy.asr.language = "zh-Hant";
  legacy.asr.api_key = "fake-legacy-key";
  const storage = memoryStorage(JSON.stringify(legacy));
  const controller = new WorkspaceController({ storage });
  await controller.load();
  assert.deepEqual(storage.reads, ["orally-config-editor-v1"]);
  assert.equal(controller.snapshot().asr.model, "previous preview model");
  assert.equal(controller.snapshot().asr.language, "zh-Hant");
  assert.equal(controller.snapshot().asr.api_key, "fake-legacy-key");
  controller.edit((editor) => { editor.activeProfile.asr.model = "new preview model"; }, null);
  assert.equal(await controller.save(), true);
  assert.equal(storage.writes[0].key, "orally-config-editor-v1");
  assert.equal(JSON.parse(storage.writes[0].value).asr.model, "new preview model");
  assert.equal(controller.getState().editor.dirty, false);
});

test("failed reload retains the current draft and a first-load failure provides an unsaved recoverable draft", async () => {
  let failing = false;
  const controller = new WorkspaceController({
    storage: { getItem: () => { if (failing) throw new Error("storage unavailable"); return JSON.stringify(savedConfig()); } },
  });
  await controller.load();
  controller.edit((editor) => { editor.activeProfile.asr.model = "keep my draft"; }, null);
  failing = true;
  await controller.load();
  assert.equal(controller.snapshot().asr.model, "keep my draft");
  assert.equal(controller.getState().editor.dirty, true);
  assert.equal(controller.getState().loading, false);
  assert.equal(controller.getState().status.error, true);

  const firstLoad = new WorkspaceController({ storage: memoryStorage("invalid JSON") });
  await firstLoad.load();
  assert.ok(firstLoad.getState().editor);
  assert.equal(firstLoad.getState().editor.dirty, true);
  assert.equal(firstLoad.getState().loading, false);
  firstLoad.edit((editor) => { editor.activeProfile.asr.model = "recoverable"; }, null);
  assert.equal(firstLoad.snapshot().asr.model, "recoverable");
});

test("closing the workspace prevents late configuration and service replies from publishing", async () => {
  const response = deferred();
  let reads = 0;
  let signal;
  const controller = new WorkspaceController({
    invoke: async () => { reads += 1; return reads === 1 ? savedConfig() : response.promise; },
  });
  await controller.load();
  const snapshots = [];
  controller.subscribe(() => snapshots.push(controller.snapshot()));
  const reloading = controller.load();
  controller.dispose();
  const publishedBeforeReply = snapshots.length;
  response.resolve(savedConfig("b", "late"));
  await reloading;
  assert.equal(snapshots.length, publishedBeforeReply);
  assert.equal(controller.snapshot().asr.model, "a-saved");

  const serviceReply = deferred();
  const service = new WorkspaceController({
    storage: memoryStorage(JSON.stringify(savedConfig())),
    testConnection: async (_snapshot, _id, options) => { signal = options.signal; return serviceReply.promise; },
  });
  await service.load();
  const pending = service.runConnection();
  service.dispose();
  assert.equal(signal.aborted, true);
  serviceReply.resolve({ message: "late service success", elapsed_ms: 1 });
  await pending;
  assert.equal(service.connection(), undefined);
});


test("legacy logging defaults and edited cleanup settings persist through auto-save and reload", async () => {
  const legacy = savedConfig();
  delete legacy.logging;
  const storage = memoryStorage(JSON.stringify(legacy));
  const controller = new WorkspaceController({ storage });
  await controller.load();
  assert.deepEqual(controller.snapshot().logging, { auto_cleanup: true, retention_days: 1 });
  controller.editSettings((config) => {
    config.logging.auto_cleanup = false;
    config.logging.retention_days = 7;
  });
  await controller.settingsTask;
  await controller.load();
  assert.deepEqual(controller.snapshot().logging, { auto_cleanup: false, retention_days: 7 });
});
