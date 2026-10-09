import test from "node:test";
import assert from "node:assert/strict";
import { ConfigEditor, DEFAULT_CONFIG, clone, normalizeConfig } from "../ui/editor-state.js";

test("new configurations start with generic OpenAI-Compatible services and no stored credential", () => {
  const editor = new ConfigEditor();
  for (const profile of [editor.activeProfile, editor.newProfile()]) {
    assert.equal(profile.asr.base_url, "https://api.openai.com/v1");
    assert.equal(profile.asr.model, "whisper-1");
    assert.equal(profile.asr.protocol, "auto");
    assert.equal(profile.asr.api_key, null);
    assert.equal(profile.asr.api_key_env, "ORALLY_OPENAI_COMPAT_API_KEY");
    assert.equal(profile.postprocess.models[0].base_url, "https://api.openai.com/v1");
    assert.equal(profile.postprocess.models[0].model, "gpt-4o-mini");
    assert.equal(profile.postprocess.models[0].api_key, null);
  }
});

test("legacy single-model configuration becomes one real editable profile without losing hidden settings", () => {
  const legacy = clone(DEFAULT_CONFIG);
  delete legacy.profiles;
  delete legacy.active_profile_id;
  delete legacy.postprocess.models;
  legacy.asr.api_key = "asr-secret";
  legacy.asr.prompt = "Deprecated recognition instructions.";
  legacy.asr.language = "zh-Hant";
  legacy.asr.api_key_env = "CUSTOM_ASR_API_KEY";
  legacy.postprocess.api_key = "postprocess-secret";
  legacy.postprocess.api_key_env = "CUSTOM_POSTPROCESS_API_KEY";
  legacy.postprocess.user_template = "Keep the original language: {{transcript}}";
  legacy.postprocess.fallback_to_builtin = false;
  legacy.audio.auto_stop_enabled = true;
  legacy.audio.input_mode = "custom-mode";
  legacy.audio.silence_threshold = 0.05;
  legacy.output.unknown_option = "preserve";
  legacy.extension = { keep: true };
  const editor = new ConfigEditor(legacy);
  assert.equal(editor.config.profiles.length, 1);
  assert.equal(editor.activeProfile.postprocess.models.length, 1);
  assert.equal(editor.activeProfile.postprocess.models[0].api_key, "postprocess-secret");
  assert.equal(editor.dirty, false);
  assert.equal(editor.profileDirty(editor.activeProfile.id), false);
  Object.assign(editor.activeProfile.asr, {
    model: "updated-asr", api_key: "fake-asr-key-visible", prompt: "保留专有名词",
  });
  Object.assign(editor.activeProfile.postprocess.models[0], {
    model: "updated-postprocess", api_key: "fake-postprocess-key-visible", system_prompt: "整理标点和段落",
  });
  const saved = editor.snapshot();
  assert.equal(saved.asr.model, "updated-asr");
  assert.equal(saved.asr.api_key, "fake-asr-key-visible");
  assert.ok(!Object.hasOwn(saved.asr, "prompt"));
  assert.equal(saved.asr.language, "zh-Hant");
  assert.equal(saved.asr.api_key_env, "CUSTOM_ASR_API_KEY");
  assert.equal(saved.postprocess.model, "updated-postprocess");
  assert.equal(saved.postprocess.api_key, "fake-postprocess-key-visible");
  assert.equal(saved.postprocess.system_prompt, "整理标点和段落");
  assert.equal(saved.postprocess.api_key_env, "CUSTOM_POSTPROCESS_API_KEY");
  assert.equal(saved.postprocess.user_template, "Keep the original language: {{transcript}}");
  assert.equal(saved.postprocess.fallback_to_builtin, false);
  assert.deepEqual(saved.audio, legacy.audio);
  assert.equal(saved.output.unknown_option, "preserve");
  assert.deepEqual(saved.extension, { keep: true });
  const persisted = JSON.stringify(saved);
  assert.ok(persisted.includes('"api_key":"fake-asr-key-visible"'));
  assert.ok(persisted.includes('"api_key":"fake-postprocess-key-visible"'));
  const restored = new ConfigEditor(JSON.parse(persisted));
  assert.equal(restored.activeProfile.asr.api_key, "fake-asr-key-visible");
  assert.equal(restored.activeProfile.asr.language, "zh-Hant");
  assert.equal(restored.activeProfile.asr.api_key_env, "CUSTOM_ASR_API_KEY");
  assert.ok(!Object.hasOwn(restored.activeProfile.asr, "prompt"));
  const restoredModel = restored.activeProfile.postprocess.models[0];
  assert.equal(restoredModel.model, "updated-postprocess");
  assert.equal(restoredModel.api_key, "fake-postprocess-key-visible");
  assert.equal(restoredModel.system_prompt, "整理标点和段落");
  assert.equal(restoredModel.api_key_env, "CUSTOM_POSTPROCESS_API_KEY");
  assert.equal(restoredModel.user_template, "Keep the original language: {{transcript}}");
  assert.equal(restoredModel.fallback_to_builtin, false);
  assert.equal(restored.dirty, false);
});

test("switching profiles preserves each draft and saving selects the active profile for execution", () => {
  const editor = new ConfigEditor();
  const first = editor.activeProfile;
  first.asr.model = "first-draft";
  const second = editor.newProfile();
  second.asr.model = "second-draft";
  second.postprocess.models[0].model = "second-refinement";
  editor.config.output.locale = "en-US";
  editor.selectProfile(first.id);
  assert.equal(editor.activeProfile.asr.model, "first-draft");
  editor.selectProfile(second.id);
  const snapshot = editor.snapshot();
  assert.equal(snapshot.active_profile_id, second.id);
  assert.equal(snapshot.asr.model, "second-draft");
  assert.equal(snapshot.postprocess.model, "second-refinement");
  assert.equal(snapshot.profiles[0].asr.model, "first-draft");
  assert.equal(snapshot.output.locale, "en-US");
  editor.markSaved(snapshot);
  assert.equal(editor.dirty, false);
  assert.equal(editor.profileDirty(second.id), false);
  const restored = new ConfigEditor(snapshot);
  assert.equal(restored.activeProfile.id, second.id);
  assert.equal(restored.activeProfile.postprocess.models[0].model, "second-refinement");
});

test("an asynchronous save acknowledgement keeps later edits dirty", () => {
  const editor = new ConfigEditor();
  editor.activeProfile.postprocess.models[0].model = "submitted";
  const submitted = editor.snapshot();
  editor.activeProfile.postprocess.models[0].model = "typed-during-save";
  editor.markSaved(submitted);
  assert.equal(editor.activeProfile.postprocess.models[0].model, "typed-during-save");
  assert.equal(editor.dirty, true);
  assert.equal(editor.profileDirty(editor.activeProfile.id), true);
  editor.markSaved(editor.snapshot());
  assert.equal(editor.dirty, false);
  assert.equal(editor.profileDirty(editor.activeProfile.id), false);
});

test("model and Prompt edits, enable flags, order and removal survive serialization", () => {
  const editor = new ConfigEditor();
  const first = editor.activeProfile.postprocess.models[0];
  first.api_key = "do-not-copy";
  const second = editor.addModel();
  second.model = "later-model";
  second.enabled = false;
  assert.equal(second.api_key, null);
  const one = editor.addPrompt(first.id);
  one.content = "保留名称 <script>example</script>";
  const two = editor.addPrompt(first.id);
  two.content = "再整理段落";
  two.enabled = false;
  assert.equal(editor.movePrompt(first.id, two.id, -1), true);
  assert.equal(editor.moveModel(second.id, -1), true);
  const serialized = editor.snapshot();
  assert.equal(serialized.postprocess.model, second.model, "legacy fields always mirror the first model, including a disabled one");
  assert.equal(serialized.postprocess.models[0].id, second.id);
  const restored = new ConfigEditor(serialized);
  assert.deepEqual(restored.model(first.id).prompts.map((prompt) => [prompt.id, prompt.content, prompt.enabled]), [
    [two.id, "再整理段落", false], [one.id, "保留名称 <script>example</script>", true],
  ]);
  assert.equal(restored.removePrompt(first.id, two.id), true);
  assert.equal(restored.removeModel(second.id), true);
  assert.equal(restored.removeModel(first.id), false, "keep one editable model");
  restored.model(first.id).enabled = false;
  assert.equal(restored.snapshot().postprocess.models[0].enabled, false);
});

test("import creates fresh profiles and node identities without overwriting existing drafts or global settings", () => {
  const editor = new ConfigEditor();
  const existing = editor.activeProfile;
  existing.asr.model = "unsaved-local-model";
  editor.config.privacy.allow_external_requests = false;
  const prompt = editor.addPrompt(existing.postprocess.models[0].id);
  prompt.content = "原有 Prompt";
  const exported = editor.exportProfile();
  assert.equal(editor.import(exported), 1);
  assert.notEqual(editor.activeProfile.id, existing.id);
  assert.notEqual(editor.activeProfile.postprocess.models[0].id, existing.postprocess.models[0].id);
  assert.notEqual(editor.activeProfile.postprocess.models[0].prompts[0].id, prompt.id);
  assert.equal(editor.config.profiles[0].asr.model, "unsaved-local-model");
  assert.equal(editor.activeProfile.name, "默认配置 2");
  assert.equal(editor.config.privacy.allow_external_requests, false);
  assert.equal(editor.dirty, true);
});

test("complete configuration import preserves all imported profiles and its selected identity", () => {
  const source = new ConfigEditor();
  const second = source.newProfile();
  second.name = "Selected imported config";
  second.asr.model = "selected-imported-model";
  source.config.audio.auto_stop_enabled = true;
  const target = new ConfigEditor();
  assert.equal(target.import(source.snapshot()), 2);
  assert.equal(target.config.profiles.length, 3);
  assert.equal(target.activeProfile.name, "Selected imported config");
  assert.equal(target.activeProfile.asr.model, "selected-imported-model");
  assert.equal(target.config.audio.auto_stop_enabled, false, "global settings belong to the current installation");
});

test("invalid import leaves every existing draft and selected profile untouched", () => {
  const editor = new ConfigEditor();
  editor.activeProfile.name = "未保存的名称";
  const before = editor.snapshot();
  const invalid = editor.exportProfile();
  invalid.profile.postprocess.models[0].prompts = [{ id: "bad", name: "invalid", content: 42 }];
  assert.throws(() => editor.import(invalid), /Prompt/);
  assert.deepEqual(editor.snapshot(), before);
  assert.throws(() => editor.import({ profiles: {} }), /数组/);
  assert.throws(() => editor.import({ format: "orally-profile", version: 2, profile: {} }), /版本/);
  const invalidGlobal = clone(DEFAULT_CONFIG);
  invalidGlobal.output.paste_delay_ms = -1;
  assert.throws(() => editor.import(invalidGlobal), /paste_delay_ms/);
  assert.deepEqual(editor.snapshot(), before);
});

test("the legacy ai mode is normalized to the editable llm mode", () => {
  const legacy = clone(DEFAULT_CONFIG);
  legacy.postprocess.mode = "ai";
  assert.equal(normalizeConfig(legacy).postprocess.mode, "llm");
  const editor = new ConfigEditor();
  assert.equal(editor.import(legacy), 1);
  assert.equal(editor.activeProfile.postprocess.mode, "llm");
});

test("new, saved and imported ASR profiles use Auto negotiation without replacing existing service settings", () => {
  for (const protocol of ["multipart", "chat-completions", "openai-transcriptions", "chat-audio", "auto"]) {
    const legacy = clone(DEFAULT_CONFIG);
    legacy.asr.protocol = protocol;
    legacy.asr.base_url = "https://existing-service.example/compat/v1";
    legacy.asr.model = "existing-speech-model";
    legacy.asr.api_key = "fake-existing-key";
    legacy.asr.prompt = "Unsaved recognition context.";
    legacy.asr.language = "zh-Hant";
    legacy.asr.api_key_env = "CUSTOM_ASR_KEY";
    const editor = new ConfigEditor(legacy);
    assert.equal(editor.activeProfile.asr.protocol, "auto");
    assert.equal(editor.config.asr.protocol, "auto");
    assert.equal(editor.snapshot().asr.protocol, "auto");
    assert.equal(editor.dirty, false);
    assert.equal(editor.import(legacy), 1);
    assert.equal(editor.activeProfile.asr.protocol, "auto");
    editor.activeProfile.asr.protocol = protocol;
    const saved = editor.snapshot();
    editor.markSaved(saved);
    assert.equal(saved.asr.protocol, "auto");
    assert.ok(saved.profiles.every((profile) => profile.asr.protocol === "auto"));
    for (const profile of saved.profiles) {
      assert.equal(profile.asr.base_url, "https://existing-service.example/compat/v1");
      assert.equal(profile.asr.model, "existing-speech-model");
      assert.equal(profile.asr.api_key, "fake-existing-key");
      assert.ok(!Object.hasOwn(profile.asr, "prompt"));
      assert.equal(profile.asr.language, "zh-Hant");
      assert.equal(profile.asr.api_key_env, "CUSTOM_ASR_KEY");
    }
    assert.equal(new ConfigEditor(saved).activeProfile.asr.protocol, "auto");
    assert.equal(editor.newProfile().asr.protocol, "auto");
  }
});

test("legacy ASR Prompt is ignored on import and export while ordered model Prompts are retained", () => {
  for (const legacyPrompt of ["Deprecated recognition instructions.", { ignored: "obsolete format" }]) {
    const legacy = clone(DEFAULT_CONFIG);
    legacy.asr.prompt = legacyPrompt;
    legacy.postprocess.system_prompt = "Keep the model's original instruction.";
    const source = new ConfigEditor(legacy);
    const node = source.addPrompt(source.activeProfile.postprocess.models[0].id);
    node.content = "Keep the child instruction too.";
    const document = source.exportProfile();
    document.profile.asr.prompt = legacyPrompt;
    const target = new ConfigEditor();
    target.import(document);
    assert.ok(!Object.hasOwn(DEFAULT_CONFIG.asr, "prompt"));
    assert.ok(!Object.hasOwn(target.activeProfile.asr, "prompt"));
    target.activeProfile.asr.prompt = "Late obsolete field from an older caller.";
    const saved = target.snapshot();
    assert.ok(!Object.hasOwn(saved.asr, "prompt"));
    assert.ok(saved.profiles.every((profile) => !Object.hasOwn(profile.asr, "prompt")));
    const exported = target.exportProfile();
    assert.ok(!Object.hasOwn(exported.profile.asr, "prompt"));
    const restored = new ConfigEditor(saved);
    const model = restored.activeProfile.postprocess.models[0];
    assert.equal(model.system_prompt, "Keep the model's original instruction.");
    assert.equal(model.prompts[0].content, "Keep the child instruction too.");
    assert.equal(exported.profile.postprocess.models[0].system_prompt, model.system_prompt);
    assert.equal(exported.profile.postprocess.models[0].prompts[0].content, model.prompts[0].content);
  }
});

test("a recovery draft remains unsaved until a successful explicit save", () => {
  const editor = new ConfigEditor();
  editor.markUnsaved();
  assert.equal(editor.dirty, true);
  assert.equal(editor.profileDirty(editor.activeProfile.id), true);
  editor.newProfile();
  assert.equal(editor.config.profiles.length, 2);
  editor.markSaved(editor.snapshot());
  assert.equal(editor.dirty, false);
  assert.equal(editor.profileDirty(editor.activeProfile.id), false);
});

test("a tray selection adopts the saved active profile while retaining dirty profiles, new drafts and global sections", () => {
  const original = new ConfigEditor();
  const firstId = original.activeProfile.id;
  const secondId = original.newProfile().id;
  original.selectProfile(firstId);
  const baseline = original.snapshot();
  const editor = new ConfigEditor(baseline);
  editor.activeProfile.name = "Local unsaved profile";
  editor.activeProfile.asr.model = "local-unsaved-asr";
  editor.config.output.locale = "en-US";
  const newDraft = editor.newProfile();
  newDraft.name = "Keep this unsaved draft";
  const incoming = new ConfigEditor(baseline);
  incoming.selectProfile(secondId);
  incoming.activeProfile.asr.model = "remote-saved-asr";
  incoming.config.privacy.history_enabled = false;
  incoming.config.output.paste_delay_ms = 500;
  const saved = incoming.snapshot();
  editor.acceptExternalSavedConfig(saved);
  assert.equal(editor.activeProfile.id, secondId);
  assert.equal(editor.activeProfile.asr.model, "remote-saved-asr");
  const retained = editor.config.profiles.find((profile) => profile.id === firstId);
  assert.equal(retained.name, "Local unsaved profile");
  assert.equal(retained.asr.model, "local-unsaved-asr");
  assert.equal(editor.config.profiles.find((profile) => profile.id === newDraft.id).name, "Keep this unsaved draft");
  assert.equal(editor.config.output.locale, "en-US");
  assert.equal(editor.config.output.paste_delay_ms, baseline.output.paste_delay_ms, "a locally edited global section remains a complete draft");
  assert.equal(editor.config.privacy.history_enabled, false, "unchanged sections accept saved updates");
  assert.deepEqual(editor.saved, saved);
  assert.equal(editor.profileDirty(firstId), true);
  assert.equal(editor.profileDirty(secondId), false);
  assert.equal(editor.profileDirty(newDraft.id), true);
  assert.equal(editor.dirty, true);
  editor.markSaved(editor.snapshot());
  assert.equal(editor.dirty, false);
});

test("a clean editor follows the latest saved selection and becomes clean against the new baseline", () => {
  const original = new ConfigEditor();
  const firstId = original.activeProfile.id;
  const secondId = original.newProfile().id;
  original.selectProfile(firstId);
  const editor = new ConfigEditor(original.snapshot());
  original.selectProfile(secondId);
  original.config.audio.auto_stop_enabled = true;
  original.activeProfile.asr.model = "selected-saved-model";
  editor.acceptExternalSavedConfig(original.snapshot());
  assert.equal(editor.activeProfile.id, secondId);
  assert.equal(editor.config.audio.auto_stop_enabled, true);
  assert.equal(editor.activeProfile.asr.model, "selected-saved-model");
  assert.equal(editor.dirty, false);
  assert.equal(editor.profileDirty(secondId), false);
  original.selectProfile(firstId);
  editor.acceptExternalSavedConfig(original.snapshot());
  assert.equal(editor.activeProfile.id, firstId, "the latest saved selection wins over earlier notifications");
  assert.equal(editor.dirty, false);
});

test("external saved updates retain edits made after a submitted save snapshot", () => {
  const editor = new ConfigEditor();
  const firstId = editor.activeProfile.id;
  const secondId = editor.newProfile().id;
  editor.selectProfile(firstId);
  const submitted = editor.snapshot();
  editor.activeProfile.asr.api_key = "fake-key-typed-during-save";
  editor.config.privacy.history_path = "local-unsaved-history.jsonl";
  editor.markSaved(submitted);
  const external = new ConfigEditor(submitted);
  external.selectProfile(secondId);
  editor.acceptExternalSavedConfig(external.snapshot());
  assert.equal(editor.activeProfile.id, secondId);
  assert.equal(editor.config.profiles.find((profile) => profile.id === firstId).asr.api_key, "fake-key-typed-during-save");
  assert.equal(editor.config.privacy.history_path, "local-unsaved-history.jsonl");
  assert.equal(editor.saved.active_profile_id, secondId);
  assert.equal(editor.saved.profiles.find((profile) => profile.id === firstId).asr.api_key, null);
  assert.equal(editor.dirty, true);
});
