export const OPENAI_BASE_URL = "https://api.openai.com/v1";
export const OPENAI_COMPAT_API_KEY_ENV = "ORALLY_OPENAI_COMPAT_API_KEY";
export const ASR_MODEL = "whisper-1";
export const POSTPROCESS_MODEL = "gpt-4o-mini";

export const DEFAULT_CONFIG = {
  asr: {
    base_url: OPENAI_BASE_URL,
    model: ASR_MODEL,
    protocol: "auto",
    api_key: null,
    api_key_env: OPENAI_COMPAT_API_KEY_ENV,
    language: null,
  },
  postprocess: {
    mode: "llm",
    base_url: OPENAI_BASE_URL,
    model: POSTPROCESS_MODEL,
    api_key: null,
    api_key_env: OPENAI_COMPAT_API_KEY_ENV,
    system_prompt:
      "You are Orally's AI postprocessor for raw speech-to-text transcripts. Produce text that is ready to paste into the user's active app. Preserve the speaker's meaning, intent, language, names, product terms, URLs, and code identifiers. Remove filler words, repeated fragments, false starts, and self-corrections unless they change the meaning. Add only punctuation and lightweight structure that are clearly implied by the transcript. Do not invent facts, explanations, headings, labels, quotes, or markdown fences. Return only the final text.",
    user_template:
      "Locale: {{locale}}\nTask: cleanup\nTranscript:\n{{transcript}}\n\nClean the transcript into polished text in the original language. Keep normal prose unless the speaker explicitly asks for a list, translation, or another format.",
    fallback_to_builtin: true,
    models: [],
  },
  output: {
    locale: "zh-CN",
    raw: false,
    show_changes: false,
    insert: false,
    paste_delay_ms: 300,
    restore_clipboard: true,
    restore_clipboard_delay_ms: 250,
  },
  audio: {
    dictate_seconds: 3,
    record_output: "orally-recording.wav",
    input_mode: "toggle",
    auto_stop_enabled: false,
    min_record_ms: 450,
    max_record_ms: 120000,
    silence_timeout_ms: 1200,
    silence_threshold: 0.02,
  },
  hotkey: { preset: "ctrl-alt-space" },
  privacy: {
    allow_external_requests: true,
    history_enabled: true,
    history_path: null,
  },
  profiles: [],
  active_profile_id: null,
};

const MODEL_FIELDS = [
  "base_url", "model", "api_key", "api_key_env", "system_prompt",
  "user_template", "fallback_to_builtin",
];

export function clone(value) {
  return structuredClone(value);
}

function newId(prefix) {
  return `${prefix}-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`}`;
}

function modelDefaults(postprocess = DEFAULT_CONFIG.postprocess) {
  return Object.fromEntries(MODEL_FIELDS.map((key) => [key, clone(postprocess[key]) ]));
}

function normalizePostprocess(value = {}) {
  const postprocess = { ...clone(DEFAULT_CONFIG.postprocess), ...clone(value) };
  if (postprocess.mode === "ai") postprocess.mode = "llm";
  const models = postprocess.models?.length
    ? postprocess.models
    : [{ ...modelDefaults(postprocess), id: "model-1", name: "后处理模型 1" }];
  postprocess.models = models.map((model, index) => ({
    ...modelDefaults(),
    enabled: true,
    ...model,
    id: model.id || `model-${index + 1}`,
    name: model.name || `后处理模型 ${index + 1}`,
    prompts: (model.prompts ?? []).map((prompt, promptIndex) => ({
      enabled: true,
      content: "",
      ...prompt,
      id: prompt.id || `prompt-${promptIndex + 1}`,
      name: prompt.name || `Prompt ${promptIndex + 1}`,
    })),
  }));
  return postprocess;
}

function normalizeProfile(profile, index = 0) {
  const normalized = {
    ...clone(profile),
    id: profile.id || `profile-${index + 1}`,
    name: profile.name || (index === 0 ? "默认配置" : `配置 ${index + 1}`),
    asr: normalizeAsr(profile.asr),
    postprocess: normalizePostprocess(profile.postprocess),
  };
  return normalized;
}

function normalizeAsr(value = {}) {
  const asr = { ...clone(DEFAULT_CONFIG.asr), ...clone(value), protocol: "auto" };
  delete asr.prompt;
  return asr;
}

export function normalizeConfig(value = DEFAULT_CONFIG) {
  const config = { ...clone(DEFAULT_CONFIG), ...clone(value) };
  for (const section of ["output", "audio", "hotkey", "privacy"]) {
    config[section] = { ...clone(DEFAULT_CONFIG[section]), ...clone(value[section] ?? {}) };
  }
  config.asr = normalizeAsr(value.asr);
  config.postprocess = normalizePostprocess(value.postprocess);
  config.profiles = value.profiles?.length
    ? value.profiles.map(normalizeProfile)
    : [normalizeProfile({ asr: config.asr, postprocess: config.postprocess })];
  if (!config.profiles.some((profile) => profile.id === config.active_profile_id)) {
    config.active_profile_id = config.profiles[0].id;
  }
  return config;
}

function object(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} 必须是一个对象`);
  }
}

function validateFields(value, types, label) {
  object(value, label);
  for (const [key, type] of Object.entries(types)) {
    if (!(key in value)) continue;
    const item = value[key];
    if (type.endsWith("?") && item === null) continue;
    const expected = type.replace("?", "");
    if (typeof item !== expected || (expected === "number" && (!Number.isFinite(item) || item < 0))) {
      throw new Error(`${label}.${key} 的格式不正确`);
    }
  }
}

function validateAsr(asr) {
  validateFields(asr, {
    base_url: "string", model: "string", protocol: "string", api_key: "string?",
    api_key_env: "string", language: "string?",
  }, "语音识别配置");
  if (asr.protocol && !["auto", "chat-audio", "openai-transcriptions", "multipart", "chat-completions"].includes(asr.protocol)) {
    throw new Error("语音识别协议不受支持");
  }
}

function validatePostprocess(postprocess) {
  const fields = {
    mode: "string", base_url: "string", model: "string", api_key: "string?",
    api_key_env: "string", system_prompt: "string", user_template: "string",
    fallback_to_builtin: "boolean",
  };
  validateFields(postprocess, fields, "后处理配置");
  if (postprocess.mode && !["builtin", "llm", "ai"].includes(postprocess.mode)) {
    throw new Error("后处理模式不受支持");
  }
  if (postprocess.models === undefined) return;
  if (!Array.isArray(postprocess.models)) throw new Error("模型节点必须是一个数组");
  for (const model of postprocess.models) {
    validateFields(model, { ...fields, id: "string", name: "string", enabled: "boolean" }, "模型节点");
    if (model.prompts === undefined) continue;
    if (!Array.isArray(model.prompts)) throw new Error("Prompt 节点必须是一个数组");
    for (const prompt of model.prompts) {
      validateFields(prompt, { id: "string", name: "string", content: "string", enabled: "boolean" }, "Prompt 节点");
    }
  }
}

function validateProfile(profile) {
  validateFields(profile, { id: "string", name: "string" }, "配置");
  if (!profile.asr || !profile.postprocess) throw new Error("配置需要包含 asr 和 postprocess");
  validateAsr(profile.asr);
  validatePostprocess(profile.postprocess);
}

export function validateImport(value) {
  object(value, "导入文件");
  if (value.format === "orally-profile") {
    if (value.version !== 1) throw new Error("不支持此配置导出版本");
    validateProfile(value.profile);
    return { profiles: [normalizeProfile(value.profile)], activeIndex: 0 };
  }
  if (value.profiles !== undefined && !Array.isArray(value.profiles)) {
    throw new Error("配置列表必须是一个数组");
  }
  if (value.asr !== undefined) validateAsr(value.asr);
  if (value.postprocess !== undefined) validatePostprocess(value.postprocess);
  const globalTypes = {
    output: {
      locale: "string", raw: "boolean", show_changes: "boolean", insert: "boolean",
      paste_delay_ms: "number", restore_clipboard: "boolean", restore_clipboard_delay_ms: "number",
    },
    audio: {
      dictate_seconds: "number", record_output: "string", input_mode: "string", auto_stop_enabled: "boolean",
      min_record_ms: "number", max_record_ms: "number", silence_timeout_ms: "number", silence_threshold: "number",
    },
    hotkey: { preset: "string" },
    privacy: { allow_external_requests: "boolean", history_enabled: "boolean", history_path: "string?" },
  };
  for (const [section, types] of Object.entries(globalTypes)) {
    if (value[section] !== undefined) validateFields(value[section], types, section);
  }
  const profiles = value.profiles?.length ? value.profiles : [{
    name: value.name || "导入配置", asr: value.asr, postprocess: value.postprocess,
  }];
  profiles.forEach(validateProfile);
  const activeIndex = Math.max(0, profiles.findIndex((profile) => profile.id === value.active_profile_id));
  return { profiles: profiles.map(normalizeProfile), activeIndex };
}

function move(items, id, offset) {
  const index = items.findIndex((item) => item.id === id);
  const target = index + offset;
  if (index < 0 || target < 0 || target >= items.length) return false;
  [items[index], items[target]] = [items[target], items[index]];
  return true;
}

export class ConfigEditor {
  constructor(config = DEFAULT_CONFIG) {
    this.config = normalizeConfig(config);
    this.saved = this.snapshot();
    this.hasSavedBaseline = true;
  }

  get activeProfile() {
    return this.config.profiles.find((profile) => profile.id === this.config.active_profile_id);
  }

  get dirty() {
    return !this.hasSavedBaseline || JSON.stringify(this.snapshot()) !== JSON.stringify(this.saved);
  }

  profileDirty(id) {
    if (!this.hasSavedBaseline) return true;
    const profile = this.snapshot().profiles.find((item) => item.id === id);
    const saved = this.saved.profiles.find((item) => item.id === id);
    return JSON.stringify(profile) !== JSON.stringify(saved);
  }

  selectProfile(id) {
    if (!this.config.profiles.some((profile) => profile.id === id)) throw new Error("配置不存在");
    this.config.active_profile_id = id;
  }

  newProfile() {
    const profile = normalizeProfile({
      id: newId("profile"), name: this.uniqueName("新配置"),
      asr: clone(DEFAULT_CONFIG.asr), postprocess: clone(DEFAULT_CONFIG.postprocess),
    });
    this.config.profiles.push(profile);
    this.selectProfile(profile.id);
    return profile;
  }

  uniqueName(name) {
    const names = new Set(this.config.profiles.map((profile) => profile.name));
    let candidate = name;
    for (let count = 2; names.has(candidate); count += 1) candidate = `${name} ${count}`;
    return candidate;
  }

  import(value) {
    const imported = validateImport(value);
    // Complete validation before changing existing drafts, and assign fresh identities.
    const profiles = imported.profiles.map((source) => {
      const profile = clone(source);
      profile.id = newId("profile");
      profile.name = this.uniqueName(profile.name);
      profile.postprocess.models.forEach((model) => {
        model.id = newId("model");
        model.prompts.forEach((prompt) => { prompt.id = newId("prompt"); });
      });
      return profile;
    });
    for (const profile of profiles) {
      profile.name = this.uniqueName(profile.name);
      this.config.profiles.push(profile);
    }
    this.selectProfile(profiles[imported.activeIndex].id);
    return profiles.length;
  }

  addModel() {
    const postprocess = this.activeProfile.postprocess;
    const previous = postprocess.models.at(-1);
    const model = {
      ...modelDefaults(), id: newId("model"), name: `后处理模型 ${postprocess.models.length + 1}`,
      enabled: true, prompts: [], api_key: null,
    };
    if (previous) {
      for (const key of ["base_url", "model", "api_key_env"]) model[key] = previous[key];
    }
    postprocess.models.push(model);
    postprocess.mode = "llm";
    return model;
  }

  removeModel(id) {
    const models = this.activeProfile.postprocess.models;
    if (models.length <= 1) return false;
    const index = models.findIndex((model) => model.id === id);
    if (index < 0) return false;
    models.splice(index, 1);
    return true;
  }

  moveModel(id, offset) {
    return move(this.activeProfile.postprocess.models, id, offset);
  }

  model(id) {
    const model = this.activeProfile.postprocess.models.find((item) => item.id === id);
    if (!model) throw new Error("模型节点不存在");
    return model;
  }

  addPrompt(modelId) {
    const model = this.model(modelId);
    const prompt = { id: newId("prompt"), name: `Prompt ${model.prompts.length + 1}`, content: "", enabled: true };
    model.prompts.push(prompt);
    return prompt;
  }

  removePrompt(modelId, promptId) {
    const prompts = this.model(modelId).prompts;
    const index = prompts.findIndex((prompt) => prompt.id === promptId);
    if (index < 0) return false;
    prompts.splice(index, 1);
    return true;
  }

  movePrompt(modelId, promptId, offset) {
    return move(this.model(modelId).prompts, promptId, offset);
  }

  snapshot() {
    const config = clone(this.config);
    const active = config.profiles.find((profile) => profile.id === config.active_profile_id);
    // Keep legacy clients compatible while the complete chain stays on each profile.
    for (const profile of config.profiles) {
      profile.asr.protocol = "auto";
      delete profile.asr.prompt;
      const primary = profile.postprocess.models[0];
      if (primary) {
        for (const key of MODEL_FIELDS) profile.postprocess[key] = clone(primary[key]);
      }
    }
    config.asr = clone(active.asr);
    config.postprocess = clone(active.postprocess);
    return config;
  }

  markSaved(snapshot) {
    // A completed write acknowledges its snapshot, not edits made while awaiting it.
    this.saved = clone(snapshot);
    this.hasSavedBaseline = true;
  }

  acceptExternalSavedConfig(value) {
    const incoming = new ConfigEditor(value).snapshot();
    const local = this.snapshot();
    const previous = this.saved;
    const savedProfiles = new Map(previous.profiles.map((profile) => [profile.id, profile]));
    const draftProfiles = new Map(local.profiles
      .filter((profile) => !this.hasSavedBaseline || JSON.stringify(profile) !== JSON.stringify(savedProfiles.get(profile.id)))
      .map((profile) => [profile.id, profile]));
    const merged = clone(incoming);
    merged.profiles = incoming.profiles.map((profile) => clone(draftProfiles.get(profile.id) ?? profile));
    const incomingIds = new Set(incoming.profiles.map((profile) => profile.id));
    for (const [id, profile] of draftProfiles) {
      if (!incomingIds.has(id)) merged.profiles.push(clone(profile));
    }
    // Saved selection belongs to the tray. Locally edited profiles and global
    // sections remain drafts while unchanged sections receive saved updates.
    for (const key of Object.keys(local)) {
      if (["asr", "postprocess", "profiles", "active_profile_id"].includes(key)) continue;
      if (!this.hasSavedBaseline || JSON.stringify(local[key]) !== JSON.stringify(previous[key])) {
        merged[key] = clone(local[key]);
      }
    }
    this.config = merged;
    this.saved = clone(incoming);
    this.hasSavedBaseline = true;
  }

  markUnsaved() {
    this.hasSavedBaseline = false;
  }

  exportProfile() {
    const snapshot = this.snapshot();
    return {
      format: "orally-profile", version: 1,
      profile: clone(snapshot.profiles.find((profile) => profile.id === snapshot.active_profile_id)),
    };
  }
}
