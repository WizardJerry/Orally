const invoke = window.__TAURI__.core.invoke;

const fields = {
  baseUrl: document.querySelector("#asr-base-url"),
  model: document.querySelector("#asr-model"),
  protocol: document.querySelector("#asr-protocol"),
  apiKey: document.querySelector("#asr-api-key"),
  apiKeyEnv: document.querySelector("#asr-api-key-env"),
  language: document.querySelector("#asr-language"),
  prompt: document.querySelector("#asr-prompt"),
  locale: document.querySelector("#output-locale"),
  raw: document.querySelector("#output-raw"),
  showChanges: document.querySelector("#output-show-changes"),
  insert: document.querySelector("#output-insert"),
  pasteDelayMs: document.querySelector("#output-paste-delay-ms"),
  dictateSeconds: document.querySelector("#audio-dictate-seconds"),
  recordOutput: null,
  hotkeyPreset: document.querySelector("#hotkey-preset"),
  providerPreset: document.querySelector("#provider-preset"),
};

const status = document.querySelector("#status");
const configPath = document.querySelector("#config-path");
const configPathDetail = document.querySelector("#config-path-detail");
const portableConfigPath = document.querySelector("#portable-config-path");

let currentConfig = null;

function setStatus(message, error = false) {
  status.textContent = message;
  status.classList.toggle("error", error);
}

function optional(value) {
  const trimmed = value.trim();
  return trimmed.length === 0 ? null : trimmed;
}

function applyPreset(preset) {
  if (preset === "dashscope") {
    fields.baseUrl.value = "https://dashscope.aliyuncs.com/compatible-mode/v1";
    fields.model.value = "qwen3-asr-flash";
    fields.protocol.value = "chat-audio";
    fields.apiKey.value = "";
    fields.apiKeyEnv.value = "DASHSCOPE_API_KEY";
  }

  if (preset === "openrouter") {
    fields.baseUrl.value = "https://openrouter.ai/api/v1";
    fields.model.value = "qwen/qwen3-asr-flash-2026-02-10";
    fields.protocol.value = "chat-audio";
    fields.apiKey.value = "";
    fields.apiKeyEnv.value = "OPENROUTER_API_KEY";
  }

  if (preset === "openai") {
    fields.baseUrl.value = "https://api.openai.com/v1";
    fields.model.value = "whisper-1";
    fields.protocol.value = "openai-transcriptions";
    fields.apiKey.value = "";
    fields.apiKeyEnv.value = "OPENAI_API_KEY";
  }
}

function fillForm(config) {
  currentConfig = config;
  fields.baseUrl.value = config.asr.base_url;
  fields.model.value = config.asr.model;
  fields.protocol.value = config.asr.protocol;
  fields.apiKey.value = config.asr.api_key ?? "";
  fields.apiKeyEnv.value = config.asr.api_key_env;
  fields.language.value = config.asr.language ?? "";
  fields.prompt.value = config.asr.prompt ?? "";
  fields.locale.value = config.output.locale;
  fields.raw.checked = config.output.raw;
  fields.showChanges.checked = config.output.show_changes;
  fields.insert.checked = config.output.insert;
  fields.pasteDelayMs.value = config.output.paste_delay_ms;
  fields.dictateSeconds.value = config.audio.dictate_seconds;
  fields.hotkeyPreset.value = config.hotkey.preset;
}

function collectForm() {
  return {
    asr: {
      base_url: fields.baseUrl.value.trim(),
      model: fields.model.value.trim(),
      protocol: fields.protocol.value,
      api_key: optional(fields.apiKey.value),
      api_key_env: fields.apiKeyEnv.value.trim(),
      language: optional(fields.language.value),
      prompt: optional(fields.prompt.value),
    },
    output: {
      locale: fields.locale.value.trim() || "zh-CN",
      raw: fields.raw.checked,
      show_changes: fields.showChanges.checked,
      insert: fields.insert.checked,
      paste_delay_ms: Number(fields.pasteDelayMs.value || 0),
    },
    audio: {
      dictate_seconds: Number(fields.dictateSeconds.value || 3),
      record_output: currentConfig?.audio?.record_output ?? "orally-recording.wav",
    },
    hotkey: {
      preset: fields.hotkeyPreset.value,
    },
  };
}

async function loadConfig() {
  try {
    const [path, portablePath, config] = await Promise.all([
      invoke("get_config_path"),
      invoke("get_portable_config_path"),
      invoke("get_config"),
    ]);
    configPath.textContent = path;
    configPathDetail.textContent = path;
    portableConfigPath.textContent = portablePath ?? "当前平台无法确定 portable 配置路径";
    fillForm(config);
    setStatus("配置已载入");
  } catch (error) {
    setStatus(String(error), true);
  }
}

async function saveConfig() {
  try {
    await invoke("save_config", { config: collectForm() });
    setStatus("配置已保存；热键变更将在重启 Orally 后生效");
  } catch (error) {
    setStatus(String(error), true);
  }
}

async function enablePortableConfig() {
  try {
    const path = await invoke("enable_portable_config", { config: collectForm() });
    configPath.textContent = path;
    configPathDetail.textContent = path;
    portableConfigPath.textContent = path;
    setStatus("已启用 portable 配置；重启 Orally 后优先读取同目录 config.toml");
  } catch (error) {
    setStatus(String(error), true);
  }
}

document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((item) => item.classList.remove("active"));
    document.querySelectorAll(".panel").forEach((panel) => panel.classList.remove("active"));
    tab.classList.add("active");
    document.querySelector(`#${tab.dataset.tab}`).classList.add("active");
  });
});

fields.providerPreset.addEventListener("change", () => applyPreset(fields.providerPreset.value));
document.querySelector("#reload").addEventListener("click", loadConfig);
document.querySelector("#save").addEventListener("click", saveConfig);
document.querySelector("#enable-portable").addEventListener("click", enablePortableConfig);

await loadConfig();
