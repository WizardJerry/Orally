const tauriInvoke = window.__TAURI__?.core?.invoke;

const fields = {
  baseUrl: document.querySelector("#asr-base-url"),
  model: document.querySelector("#asr-model"),
  protocol: document.querySelector("#asr-protocol"),
  apiKey: document.querySelector("#asr-api-key"),
  apiKeyEnv: document.querySelector("#asr-api-key-env"),
  language: document.querySelector("#asr-language"),
  prompt: document.querySelector("#asr-prompt"),
  postprocessMode: document.querySelector("#postprocess-mode"),
  postprocessBaseUrl: document.querySelector("#postprocess-base-url"),
  postprocessModel: document.querySelector("#postprocess-model"),
  postprocessApiKey: document.querySelector("#postprocess-api-key"),
  postprocessApiKeyEnv: document.querySelector("#postprocess-api-key-env"),
  postprocessSystemPrompt: document.querySelector("#postprocess-system-prompt"),
  postprocessUserTemplate: document.querySelector("#postprocess-user-template"),
  postprocessFallbackToBuiltin: document.querySelector("#postprocess-fallback-to-builtin"),
  locale: document.querySelector("#output-locale"),
  raw: document.querySelector("#output-raw"),
  showChanges: document.querySelector("#output-show-changes"),
  insert: document.querySelector("#output-insert"),
  pasteDelayMs: document.querySelector("#output-paste-delay-ms"),
  restoreClipboard: document.querySelector("#output-restore-clipboard"),
  restoreClipboardDelayMs: document.querySelector("#output-restore-clipboard-delay-ms"),
  recordOutput: null,
  hotkeyPreset: document.querySelector("#hotkey-preset"),
  providerPreset: document.querySelector("#provider-preset"),
  allowExternalRequests: document.querySelector("#privacy-allow-external-requests"),
  historyEnabled: document.querySelector("#privacy-history-enabled"),
  historyPath: document.querySelector("#privacy-history-path"),
};

const status = document.querySelector("#status");
const configPath = document.querySelector("#config-path");
const configPathDetail = document.querySelector("#config-path-detail");
const portableConfigPath = document.querySelector("#portable-config-path");
const pageTitle = document.querySelector("#page-title");
const documentCarousel = document.querySelector(".document-carousel");
const carouselProgress = document.querySelector("#carousel-progress");
const pipelineTitle = document.querySelector("#pipeline-title");

const ALIYUN_OPENAI_BASE_URL =
  "https://ws-xzr3kkbjij82s72f.cn-beijing.maas.aliyuncs.com/compatible-mode/v1";
const OPENAI_COMPAT_API_KEY_ENV = "ORALLY_OPENAI_COMPAT_API_KEY";
const ALIYUN_ASR_MODEL = "qwen3-asr-flash";
const ALIYUN_POSTPROCESS_MODEL = "deepseek-v4-flash-0731";

const shortcutDefaults = {
  pause: "Ctrl + Shift + P",
  addModel: "Ctrl + M",
  addPrompt: "Ctrl + Enter",
  toggleWindow: "Alt + Space",
  save: "Ctrl + S",
};

const previewConfig = {
  asr: {
    base_url: ALIYUN_OPENAI_BASE_URL,
    model: ALIYUN_ASR_MODEL,
    protocol: "chat-audio",
    api_key: null,
    api_key_env: OPENAI_COMPAT_API_KEY_ENV,
    language: "zh",
    prompt: "请保持专有名词和产品名原文。",
  },
  postprocess: {
    mode: "llm",
    base_url: ALIYUN_OPENAI_BASE_URL,
    model: ALIYUN_POSTPROCESS_MODEL,
    api_key: null,
    api_key_env: OPENAI_COMPAT_API_KEY_ENV,
    system_prompt:
      "You are Orally's AI postprocessor for raw speech-to-text transcripts. Produce text that is ready to paste into the user's active app. Preserve the speaker's meaning, intent, language, names, product terms, URLs, and code identifiers. Remove filler words, repeated fragments, false starts, and self-corrections unless they change the meaning. Add only punctuation and lightweight structure that are clearly implied by the transcript. Do not invent facts, explanations, headings, labels, quotes, or markdown fences. Return only the final text.",
    user_template:
      "Locale: {{locale}}\nTask: cleanup\nTranscript:\n{{transcript}}\n\nClean the transcript into polished text in the original language. Keep normal prose unless the speaker explicitly asks for a list, translation, or another format.",
    fallback_to_builtin: true,
  },
  output: {
    locale: "zh-CN",
    raw: false,
    show_changes: true,
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
  hotkey: {
    preset: "ctrl-alt-space",
  },
  privacy: {
    allow_external_requests: true,
    history_enabled: true,
    history_path: null,
  },
};

let currentConfig = null;
let selectedDocumentIndex = 0;
let pendingShortcutButton = null;
let shortcutState = loadShortcutState();

const documents = [
  {
    name: "会议转写.toml",
    updated: "2 min ago",
    nodes: 3,
    model: ALIYUN_ASR_MODEL,
    status: "启用",
  },
  {
    name: "访谈整理.toml",
    updated: "Today",
    nodes: 5,
    model: "gpt-4.1-mini",
    status: "草稿",
  },
  {
    name: "字幕生成.toml",
    updated: "Yesterday",
    nodes: 4,
    model: "whisper-1",
    status: "启用",
  },
  {
    name: "多语言摘要.toml",
    updated: "Jun 12",
    nodes: 6,
    model: "openrouter/qwen",
    status: "停用",
  },
];

function setStatus(message, error = false) {
  status.textContent = message;
  status.classList.toggle("error", error);
}

function optional(value) {
  const trimmed = value.trim();
  return trimmed.length === 0 ? null : trimmed;
}

function providerLabel(config = currentConfig) {
  const baseUrl = config?.asr?.base_url?.toLowerCase() ?? "";
  if (baseUrl.includes("maas.aliyuncs.com")) return "阿里云百炼（OpenAI兼容）";
  if (baseUrl.includes("openrouter")) return "OpenRouter";
  if (baseUrl.includes("openai")) return "OpenAI-compatible";
  return "Custom";
}

function selectProviderPreset(config) {
  const baseUrl = config?.asr?.base_url?.toLowerCase() ?? "";
  if (baseUrl.includes("maas.aliyuncs.com")) return "aliyun-openai";
  if (baseUrl.includes("openrouter")) return "openrouter";
  if (baseUrl.includes("openai")) return "openai";
  return "custom";
}

function applyPreset(preset) {
  if (preset === "aliyun-openai") {
    fields.baseUrl.value = ALIYUN_OPENAI_BASE_URL;
    fields.model.value = ALIYUN_ASR_MODEL;
    fields.protocol.value = "chat-audio";
    fields.apiKey.value = "";
    fields.apiKeyEnv.value = OPENAI_COMPAT_API_KEY_ENV;
    fields.postprocessMode.value = "llm";
    fields.postprocessBaseUrl.value = ALIYUN_OPENAI_BASE_URL;
    fields.postprocessModel.value = ALIYUN_POSTPROCESS_MODEL;
    fields.postprocessApiKey.value = "";
    fields.postprocessApiKeyEnv.value = OPENAI_COMPAT_API_KEY_ENV;
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

  updatePipelineLabels();
  refreshDocumentsFromConfig();
  renderDocuments();
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
  fields.postprocessMode.value = config.postprocess?.mode ?? "builtin";
  fields.postprocessBaseUrl.value = config.postprocess?.base_url ?? ALIYUN_OPENAI_BASE_URL;
  fields.postprocessModel.value = config.postprocess?.model ?? ALIYUN_POSTPROCESS_MODEL;
  fields.postprocessApiKey.value = config.postprocess?.api_key ?? "";
  fields.postprocessApiKeyEnv.value =
    config.postprocess?.api_key_env ?? OPENAI_COMPAT_API_KEY_ENV;
  fields.postprocessSystemPrompt.value = config.postprocess?.system_prompt ?? "";
  fields.postprocessUserTemplate.value = config.postprocess?.user_template ?? "";
  fields.postprocessFallbackToBuiltin.checked = config.postprocess?.fallback_to_builtin ?? true;
  fields.locale.value = config.output.locale;
  fields.raw.checked = config.output.raw;
  fields.showChanges.checked = config.output.show_changes;
  fields.insert.checked = config.output.insert;
  fields.pasteDelayMs.value = config.output.paste_delay_ms;
  fields.restoreClipboard.checked = config.output.restore_clipboard ?? true;
  fields.restoreClipboardDelayMs.value = config.output.restore_clipboard_delay_ms ?? 250;
  fields.hotkeyPreset.value = config.hotkey.preset;
  fields.providerPreset.value = selectProviderPreset(config);
  fields.allowExternalRequests.checked = config.privacy?.allow_external_requests ?? true;
  fields.historyEnabled.checked = config.privacy?.history_enabled ?? true;
  fields.historyPath.value = config.privacy?.history_path ?? "";
  updatePipelineLabels();
  refreshDocumentsFromConfig();
  renderDocuments();
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
    postprocess: {
      mode: fields.postprocessMode.value,
      base_url: fields.postprocessBaseUrl.value.trim(),
      model: fields.postprocessModel.value.trim(),
      api_key: optional(fields.postprocessApiKey.value),
      api_key_env: fields.postprocessApiKeyEnv.value.trim(),
      system_prompt: fields.postprocessSystemPrompt.value.trim(),
      user_template: fields.postprocessUserTemplate.value.trim(),
      fallback_to_builtin: fields.postprocessFallbackToBuiltin.checked,
    },
    output: {
      locale: fields.locale.value.trim() || "zh-CN",
      raw: fields.raw.checked,
      show_changes: fields.showChanges.checked,
      insert: fields.insert.checked,
      paste_delay_ms: Number(fields.pasteDelayMs.value || 0),
      restore_clipboard: fields.restoreClipboard.checked,
      restore_clipboard_delay_ms: Number(fields.restoreClipboardDelayMs.value || 250),
    },
    audio: {
      dictate_seconds: currentConfig?.audio?.dictate_seconds ?? 3,
      record_output: currentConfig?.audio?.record_output ?? "orally-recording.wav",
      input_mode: "toggle",
      auto_stop_enabled: false,
      min_record_ms: currentConfig?.audio?.min_record_ms ?? 450,
      max_record_ms: currentConfig?.audio?.max_record_ms ?? 120000,
      silence_timeout_ms: currentConfig?.audio?.silence_timeout_ms ?? 1200,
      silence_threshold: currentConfig?.audio?.silence_threshold ?? 0.02,
    },
    hotkey: {
      preset: fields.hotkeyPreset.value,
    },
    privacy: {
      allow_external_requests: fields.allowExternalRequests.checked,
      history_enabled: fields.historyEnabled.checked,
      history_path: optional(fields.historyPath.value),
    },
  };
}

async function loadConfig() {
  try {
    if (!tauriInvoke) {
      configPath.textContent = "浏览器预览模式";
      configPathDetail.textContent = "Tauri 运行时未连接";
      portableConfigPath.textContent = "Tauri 运行时未连接";
      fillForm(previewConfig);
      setStatus("已载入预览配置");
      return;
    }

    const [path, portablePath, config] = await Promise.all([
      tauriInvoke("get_config_path"),
      tauriInvoke("get_portable_config_path"),
      tauriInvoke("get_config"),
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
    currentConfig = collectForm();
    saveShortcutState();

    if (!tauriInvoke) {
      setStatus("预览模式已保存到当前会话");
      return;
    }

    await tauriInvoke("save_config", { config: currentConfig });
    setStatus("配置已保存；全局热键变更将在重启 Orally 后生效");
  } catch (error) {
    setStatus(String(error), true);
  }
}

async function enablePortableConfig() {
  try {
    currentConfig = collectForm();

    if (!tauriInvoke) {
      setStatus("预览模式无法写入 portable 配置", true);
      return;
    }

    const path = await tauriInvoke("enable_portable_config", { config: currentConfig });
    configPath.textContent = path;
    configPathDetail.textContent = path;
    portableConfigPath.textContent = path;
    setStatus("已启用 portable 配置；重启 Orally 后优先读取同目录 config.toml");
  } catch (error) {
    setStatus(String(error), true);
  }
}

function showPage(page) {
  document.querySelectorAll(".nav-item").forEach((item) => {
    item.classList.toggle("active", item.dataset.page === page);
  });
  document.querySelectorAll(".page").forEach((panel) => {
    panel.classList.toggle("active", panel.id === `${page}-page`);
    if (panel.id === `${page}-page`) {
      pageTitle.textContent = panel.dataset.title;
    }
  });
}

function refreshDocumentsFromConfig() {
  if (!currentConfig) return;

  documents[0].model = currentConfig.asr.model || "未设置模型";
  documents[0].nodes = currentConfig.postprocess?.mode === "llm" ? 3 : 2;
  documents[0].status = currentConfig.privacy?.allow_external_requests ? "启用" : "离线";
}

function renderDocuments() {
  documentCarousel.innerHTML = "";
  const progressStep = 100 / documents.length;
  carouselProgress.style.width = `${progressStep}%`;
  carouselProgress.style.transform = `translateX(${selectedDocumentIndex * 100}%)`;

  documents.forEach((doc, index) => {
    const card = document.createElement("button");
    card.type = "button";
    card.className = "doc-card";
    card.dataset.docIndex = String(index);
    if (index === selectedDocumentIndex) {
      card.classList.add("active");
    } else {
      card.classList.add("peek");
    }

    card.innerHTML = `
      <header>
        <span class="chip">${doc.status}</span>
        <strong>${doc.name}</strong>
        <small>最后修改 ${doc.updated}</small>
      </header>
      <div class="doc-metrics">
        <div class="doc-metric"><span>管线节点</span><code>${doc.nodes}</code></div>
        <div class="doc-metric"><span>默认模型</span><code>${doc.model}</code></div>
        <div class="doc-metric"><span>快捷状态</span><code>${shortcutLabel(fields.hotkeyPreset.value)}</code></div>
      </div>
      <span class="filled doc-open">打开管线</span>
    `;

    card.addEventListener("click", () => {
      selectedDocumentIndex = index;
      pipelineTitle.textContent = doc.name;
      renderDocuments();
      showPage("pipeline");
    });

    documentCarousel.append(card);
  });
}

function moveDocument(delta) {
  selectedDocumentIndex = (selectedDocumentIndex + delta + documents.length) % documents.length;
  renderDocuments();
}

function updatePipelineLabels() {
  const config = collectForm();
  currentConfig = config;
  document.querySelector("#asr-provider-label").textContent = providerLabel(config);
  document.querySelector("#asr-model-label").textContent = config.asr.model || "未设置模型";
  document.querySelector("#asr-language-label").textContent = config.asr.language || config.output.locale;
  document.querySelector("#postprocess-mode-label").textContent = config.postprocess.mode;
  document.querySelector("#postprocess-model-label").textContent =
    config.postprocess.mode === "llm" ? config.postprocess.model || "未设置模型" : "本地基础清理";
  document.querySelector("#system-token-label").textContent = `${estimateTokens(config.postprocess.system_prompt)} tokens`;
  document.querySelector("#template-token-label").textContent = `${estimateTokens(config.postprocess.user_template)} tokens`;
}

function estimateTokens(text) {
  return Math.max(0, Math.ceil((text || "").trim().length / 4));
}

function selectNode(row) {
  document.querySelectorAll(".model-node, .prompt-row").forEach((node) => node.classList.remove("selected"));
  const modelNode = row.closest(".model-node");
  if (modelNode) modelNode.classList.add("selected");
  row.classList.add("selected");
}

function addModelNode() {
  const count = document.querySelectorAll(".model-node").length + 1;
  const article = document.createElement("article");
  article.className = "model-node expanded";
  article.dataset.nodeId = `custom-model-${count}`;
  article.innerHTML = `
    <button class="node-row model-row" type="button" data-expand="custom-model-${count}">
      <span class="chevron" aria-hidden="true">v</span>
      <span class="node-symbol model-symbol">AI</span>
      <span class="node-copy">
        <strong>AI 模型 ${count}</strong>
        <small>Custom · 未设置模型</small>
      </span>
      <span class="chip">AI</span>
      <span class="meta">draft</span>
      <span class="node-menu" aria-hidden="true">...</span>
    </button>
    <div class="prompt-children">
      <button class="add-prompt" data-parent="custom-model-${count}" type="button">+ 添加 Prompt</button>
    </div>
  `;
  document.querySelector("#pipeline-list").append(article);
  wirePipelineNode(article);
  setStatus("已添加 AI 模型节点");
}

function addPromptNode(parentButton) {
  const promptChildren = parentButton.closest(".prompt-children");
  const count = promptChildren.querySelectorAll(".prompt-row").length + 1;
  const row = document.createElement("div");
  row.className = "prompt-row";
  row.dataset.nodeId = `prompt-${Date.now()}`;
  row.innerHTML = `
    <span class="child-rail" aria-hidden="true"></span>
    <span class="node-symbol prompt-symbol">PR</span>
    <span class="node-copy">
      <strong>Prompt ${count}</strong>
      <small>新建子节点 · 0 tokens</small>
    </span>
    <span class="chip subtle">Prompt</span>
    <button class="icon-button mini" type="button" title="编辑" aria-label="编辑">E</button>
  `;
  promptChildren.insertBefore(row, parentButton);
  row.addEventListener("click", () => selectNode(row));
  setStatus("已添加 Prompt 子节点");
}

function wirePipelineNode(root = document) {
  root.querySelectorAll("[data-expand]").forEach((button) => {
    button.addEventListener("click", () => {
      const node = button.closest(".model-node");
      node.classList.toggle("expanded");
      selectNode(button);
    });
  });

  root.querySelectorAll(".prompt-row").forEach((row) => {
    row.addEventListener("click", () => selectNode(row));
  });

  root.querySelectorAll(".add-prompt").forEach((button) => {
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      addPromptNode(button);
    });
  });
}

function shortcutLabel(preset) {
  return preset
    .split("-")
    .map((part) => {
      if (part === "ctrl") return "Ctrl";
      if (part === "alt") return "Alt";
      if (part === "shift") return "Shift";
      return part.toUpperCase();
    })
    .join(" + ");
}

function loadShortcutState() {
  try {
    return {
      ...shortcutDefaults,
      ...JSON.parse(localStorage.getItem("orally-shortcuts") ?? "{}"),
    };
  } catch {
    return { ...shortcutDefaults };
  }
}

function saveShortcutState() {
  localStorage.setItem("orally-shortcuts", JSON.stringify(shortcutState));
}

function renderShortcuts() {
  document.querySelectorAll(".key-capture").forEach((button) => {
    button.textContent = shortcutState[button.dataset.shortcut] ?? shortcutDefaults[button.dataset.shortcut];
  });
  saveShortcutState();
}

function captureKeyLabel(event) {
  if (["Control", "Alt", "Shift", "Meta"].includes(event.key)) {
    return "";
  }

  const parts = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Win");

  const key = event.key.length === 1 ? event.key.toUpperCase() : event.key.replace("Arrow", "");
  parts.push(key);

  return parts.join(" + ");
}

function filterShortcuts(query) {
  const normalized = query.trim().toLowerCase();
  document.querySelectorAll(".shortcut-row").forEach((row) => {
    const text = row.dataset.shortcutName.toLowerCase();
    row.classList.toggle("hidden", normalized.length > 0 && !text.includes(normalized));
  });
}

async function exportShortcuts() {
  const text = JSON.stringify({ global: fields.hotkeyPreset.value, shortcuts: shortcutState }, null, 2);
  try {
    await navigator.clipboard.writeText(text);
    setStatus("快捷键配置已复制到剪贴板");
  } catch {
    const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = "orally-shortcuts.json";
    link.click();
    URL.revokeObjectURL(url);
    setStatus("快捷键配置已导出");
  }
}

function importShortcuts(file) {
  const reader = new FileReader();
  reader.addEventListener("load", () => {
    try {
      const parsed = JSON.parse(String(reader.result));
      if (parsed.global) fields.hotkeyPreset.value = parsed.global;
      shortcutState = { ...shortcutDefaults, ...(parsed.shortcuts ?? parsed) };
      renderShortcuts();
      setStatus("快捷键配置已导入");
    } catch (error) {
      setStatus(`导入失败：${error}`, true);
    }
  });
  reader.readAsText(file);
}

document.querySelectorAll(".nav-item").forEach((item) => {
  item.addEventListener("click", () => showPage(item.dataset.page));
});

document.querySelector("#prev-doc").addEventListener("click", () => moveDocument(-1));
document.querySelector("#next-doc").addEventListener("click", () => moveDocument(1));
document.querySelector("#add-model").addEventListener("click", addModelNode);

fields.providerPreset.addEventListener("change", () => applyPreset(fields.providerPreset.value));
fields.hotkeyPreset.addEventListener("change", renderDocuments);

document.querySelectorAll("input, select, textarea").forEach((control) => {
  control.addEventListener("input", () => {
    updatePipelineLabels();
    refreshDocumentsFromConfig();
    renderDocuments();
  });
});

document.querySelectorAll(".key-capture").forEach((button) => {
  button.addEventListener("click", () => {
    if (pendingShortcutButton) pendingShortcutButton.classList.remove("capturing");
    pendingShortcutButton = button;
    button.classList.add("capturing");
    button.textContent = "按下新的组合键";
  });
});

document.addEventListener("keydown", (event) => {
  if (!pendingShortcutButton) return;
  event.preventDefault();
  const shortcut = captureKeyLabel(event);
  if (!shortcut) return;
  shortcutState[pendingShortcutButton.dataset.shortcut] = shortcut;
  pendingShortcutButton.classList.remove("capturing");
  pendingShortcutButton = null;
  renderShortcuts();
  setStatus("快捷键已更新");
});

document.querySelector("#shortcut-search").addEventListener("input", (event) => {
  filterShortcuts(event.target.value);
});

document.querySelector("#restore-shortcuts").addEventListener("click", () => {
  shortcutState = { ...shortcutDefaults };
  fields.hotkeyPreset.value = "ctrl-alt-space";
  renderShortcuts();
  renderDocuments();
  setStatus("快捷键已恢复默认");
});

document.querySelector("#export-shortcuts").addEventListener("click", exportShortcuts);
document.querySelector("#import-shortcuts").addEventListener("click", () => {
  document.querySelector("#shortcut-import-file").click();
});
document.querySelector("#shortcut-import-file").addEventListener("change", (event) => {
  const [file] = event.target.files;
  if (file) importShortcuts(file);
});

document.querySelector("#reload").addEventListener("click", loadConfig);
document.querySelector("#save").addEventListener("click", saveConfig);
document.querySelector("#enable-portable").addEventListener("click", enablePortableConfig);

wirePipelineNode();
renderShortcuts();
await loadConfig();
