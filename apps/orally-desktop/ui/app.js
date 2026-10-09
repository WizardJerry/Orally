import { ConfigEditor, DEFAULT_CONFIG } from "./editor-state.js";
import { testServiceConnection, testConfigInput } from "./service-tests.js";
import { startBrowserRecording } from "./test-recorder.js";
import { TestSession } from "./test-session.js";
import { HotkeyCaptureSession, formatHotkey } from "./hotkey-capture.js";

const tauriInvoke = window.__TAURI__?.core?.invoke;
const query = (selector) => document.querySelector(selector);
const fields = {
  baseUrl: query("#asr-base-url"), model: query("#asr-model"),
  apiKey: query("#asr-api-key"),
  postprocessMode: query("#postprocess-mode"),
  raw: query("#output-raw"),
  showChanges: query("#output-show-changes"), insert: query("#output-insert"),
  pasteDelayMs: query("#output-paste-delay-ms"), restoreClipboard: query("#output-restore-clipboard"),
  restoreClipboardDelayMs: query("#output-restore-clipboard-delay-ms"),
  allowExternalRequests: query("#privacy-allow-external-requests"),
  historyEnabled: query("#privacy-history-enabled"), historyPath: query("#privacy-history-path"),
};
const PREVIEW_STORAGE_KEY = "orally-config-editor-v1";
let editor = null;
let selectedModelId = null;
let saving = false;
let loading = false;
const collapsedModels = new Set();
const connectionTests = new Map();
const visibilityButtons = new WeakSet();
const configTestSession = new TestSession(renderConfigTestSession);
let recordingTimer = null;
let recordingLimitMs = 120000;
let recordingNotice = "";
let recordingLimitReason = null;
let pendingProfileSync = false;
let syncingProfile = false;
let profileSyncVersion = 0;
let configOperationVersion = 0;
let stopProfileEvents = null;
const hotkeyCapture = new HotkeyCaptureSession({
  setActive: (active) => tauriInvoke ? tauriInvoke("set_hotkey_capture", { active }) : Promise.resolve(),
  onCommit: (value) => {
    if (!editor) return;
    editor.config.hotkey.preset = value;
    setStatus("快捷键已录入");
  },
  onChange: (session) => {
    renderConfigState();
    if (session.error) setStatus(session.restorationFailed
      ? `原快捷键恢复失败：${session.error.message ?? session.error}。可保存新组合或重新录入。`
      : `快捷键录入失败：${session.error.message ?? session.error}`, true);
  },
});

function queueSavedProfileSync(profileId) {
  if (typeof profileId !== "string" || !profileId) return;
  profileSyncVersion += 1;
  pendingProfileSync = true;
  void syncSavedProfile();
}

async function syncSavedProfile() {
  if (!tauriInvoke || loading || saving || syncingProfile || !pendingProfileSync) return;
  syncingProfile = true;
  try {
    while (pendingProfileSync && !loading && !saving) {
      pendingProfileSync = false;
      const version = profileSyncVersion;
      const operation = configOperationVersion;
      const config = await tauriInvoke("get_config");
      if (version !== profileSyncVersion || operation !== configOperationVersion || loading || saving) {
        pendingProfileSync = true;
        continue;
      }
      if (editor) {
        collectForm();
        editor.acceptExternalSavedConfig(config);
      } else editor = new ConfigEditor(config);
      fillForm();
      setStatus(`托盘已切换为“${editor.activeProfile.name}”${editor.dirty ? "，未保存的草稿已保留" : ""}`);
    }
  } catch (error) {
    setStatus(`同步托盘配置失败：${redactTestError(error, editor?.snapshot())}`, true);
  } finally {
    syncingProfile = false;
    if (pendingProfileSync && !loading && !saving) void syncSavedProfile();
  }
}

async function initializeProfileEvents() {
  if (!tauriInvoke) return null;
  if (!window.__TAURI__?.event?.listen) return "当前运行时没有配置切换事件接口";
  try {
    stopProfileEvents = await window.__TAURI__.event.listen("active-profile-changed", (event) => {
      queueSavedProfileSync(event.payload?.profile_id);
    });
    return null;
  } catch (error) {
    return redactTestError(error, editor?.snapshot());
  }
}

function redactTestError(error, snapshot) {
  let message = error instanceof Error ? error.message : String(error);
  const sections = [snapshot?.asr, snapshot?.postprocess];
  for (const profile of snapshot?.profiles ?? []) {
    sections.push(profile.asr, profile.postprocess, ...(profile.postprocess?.models ?? []));
  }
  sections.push(...(snapshot?.postprocess?.models ?? []));
  const keys = [...new Set(sections.map((section) => section?.api_key).filter((key) => typeof key === "string" && key.length > 0))];
  for (const key of keys.sort((left, right) => right.length - left.length)) message = message.split(key).join("[API Key 已隐藏]");
  return message;
}

function renderKeyVisibility(input, button, visible = false) {
  input.type = visible ? "text" : "password";
  button.setAttribute("aria-label", visible ? "隐藏 API Key" : "显示 API Key");
  button.setAttribute("aria-pressed", String(visible));
  button.title = visible ? "隐藏 API Key" : "显示 API Key";
  const namespace = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(namespace, "svg");
  for (const [name, value] of Object.entries({ viewBox: "0 0 24 24", width: "18", height: "18", fill: "none", stroke: "currentColor", "stroke-width": "1.7", "stroke-linecap": "round", "stroke-linejoin": "round", "aria-hidden": "true" })) svg.setAttribute(name, value);
  const outline = document.createElementNS(namespace, "path");
  outline.setAttribute("d", "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z");
  const pupil = document.createElementNS(namespace, "circle");
  pupil.setAttribute("cx", "12");
  pupil.setAttribute("cy", "12");
  pupil.setAttribute("r", "3");
  svg.append(outline, pupil);
  if (visible) {
    const slash = document.createElementNS(namespace, "path");
    slash.setAttribute("d", "m3 3 18 18");
    svg.append(slash);
  }
  button.replaceChildren(svg);
}

function bindKeyVisibility(input, button) {
  button.type = "button";
  renderKeyVisibility(input, button);
  if (visibilityButtons.has(button)) return;
  visibilityButtons.add(button);
  button.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    renderKeyVisibility(input, button, input.type === "password");
  });
}

function connectionKey(modelId) {
  return `${editor.config.active_profile_id}:${modelId ?? "asr"}`;
}

function renderConnectionTest(modelId = null) {
  const card = modelId === null ? null : [...query("#postprocess-nodes").children].find((item) => item.dataset.modelId === modelId);
  const button = modelId === null ? query("#test-asr-connection") : card?.querySelector(".test-service-button");
  const status = modelId === null ? query("#asr-test-result") : card?.querySelector(".service-test-result");
  if (!button || !status) return;
  const test = editor ? connectionTests.get(connectionKey(modelId)) : null;
  button.disabled = loading || test?.busy === true;
  button.textContent = test?.busy ? "测试中…" : "测试连接";
  status.textContent = test?.message ?? "";
  status.classList.toggle("error", test?.error === true);
  status.classList.toggle("success", test?.success === true);
}

function invalidateConnectionTest(modelId = null) {
  if (!editor) return;
  const key = connectionKey(modelId);
  connectionTests.get(key)?.controller.abort();
  connectionTests.delete(key);
  renderConnectionTest(modelId);
}

function clearConnectionTests() {
  for (const test of connectionTests.values()) test.controller.abort();
  connectionTests.clear();
  renderConnectionTest();
  for (const model of editor?.activeProfile.postprocess.models ?? []) renderConnectionTest(model.id);
}

async function runConnectionTest(modelId = null) {
  if (!editor || loading) return;
  collectForm();
  const key = connectionKey(modelId);
  if (connectionTests.get(key)?.busy) return;
  const snapshot = editor.snapshot();
  const profileId = editor.config.active_profile_id;
  const test = { busy: true, error: false, success: false, message: "正在验证服务和模型…", controller: new AbortController() };
  connectionTests.set(key, test);
  renderConnectionTest(modelId);
  const current = () => editor.config.active_profile_id === profileId && connectionTests.get(key) === test;
  try {
    if (!snapshot.privacy.allow_external_requests) throw new Error("当前隐私设置已禁止外部请求，请先在应用设置中允许外部 AI 请求。");
    const result = tauriInvoke
      ? await tauriInvoke("test_service_connection", { config: snapshot, modelId })
      : await testServiceConnection(snapshot, modelId, { signal: test.controller.signal });
    if (!current()) return;
    test.success = true;
    test.message = `${redactTestError(result.message, snapshot)} · ${Math.round(result.elapsed_ms)} ms`;
  } catch (error) {
    if (!current()) return;
    test.error = true;
    test.message = `连接失败：${redactTestError(error, snapshot)}`;
  } finally {
    if (current()) { test.busy = false; renderConnectionTest(modelId); }
  }
}

function renderConfigTestMode() {
  const mode = query("#config-test-mode").value;
  const text = mode === "text";
  const asrOnly = mode === "asr";
  query("#config-test-text-field").hidden = !text;
  query("#config-test-recording-field").hidden = text;
  query("#config-test-raw-field").hidden = text;
  query("#config-test-final-field").hidden = asrOnly;
  query("#config-test-raw-label").textContent = asrOnly ? "识别结果" : "识别文本";
  query("#config-test-final-label").textContent = text ? "后处理结果" : "最终输出";
  query("#config-test-title").textContent = text ? "文本后处理测试" : asrOnly ? "语音识别测试" : "语音输入测试";
}

function renderConfigTestBusy() {
  const phase = configTestSession.phase;
  const mode = query("#config-test-mode").value;
  query("#run-config-test").disabled = loading || ["starting", "processing"].includes(phase);
  query("#run-config-test").textContent = phase === "starting" ? "开启麦克风…"
    : phase === "recording" ? "停止并测试"
      : phase === "processing" ? mode === "asr" ? "识别中…" : "处理中…"
        : mode === "text" ? "开始测试" : "开始录音";
  query("#cancel-config-test").hidden = !configTestSession.busy;
  for (const id of ["config-test-mode", "config-test-text"]) query(`#${id}`).disabled = configTestSession.busy;
}

function setConfigTestStatus(message = "", error = false, warning = false) {
  const status = query("#config-test-status");
  status.textContent = message;
  status.classList.toggle("error", error);
  status.classList.toggle("warning", warning);
  status.classList.remove("success");
}

function resetConfigTest(close = false) {
  recordingNotice = "";
  recordingLimitReason = null;
  configTestSession.reset();
  if (close && query("#config-test-dialog").open) query("#config-test-dialog").close();
}

function openConfigTest(mode = "voice") {
  if (!editor || loading) return;
  resetConfigTest();
  query("#config-test-mode").value = mode;
  renderConfigTestMode();
  renderConfigTestSession();
  query("#config-test-dialog").showModal();
}

function recordingElapsedMs() {
  return configTestSession.phase === "recording" ? Date.now() - configTestSession.startedAt : configTestSession.recordedMs;
}

function updateRecordingTimer(autoStop = true) {
  const elapsed = recordingElapsedMs();
  const seconds = Math.floor(elapsed / 1000);
  query("#config-test-timer").value = `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
  if (autoStop && configTestSession.phase === "recording" && elapsed >= recordingLimitMs) handleRecordingLimit("duration");
}

function handleRecordingLimit(reason) {
  if (reason === "cancelled") {
    resetConfigTest(true);
    return;
  }
  recordingLimitReason = reason;
  recordingNotice = {
    size: "录音已达到大小上限，已自动停止。",
    duration: "录音已达到时长上限，已自动停止。",
    "recording-error": "录音设备出现异常，正在结束测试。",
    expired: "录音测试已过期，正在结束测试。",
  }[reason] ?? "录音已停止，正在处理测试。";
  if (configTestSession.phase === "recording") void configTestSession.stopRecording();
}

function renderConfigTestSession() {
  const mode = query("#config-test-mode").value;
  const phase = configTestSession.phase;
  const recordingPanel = query("#config-test-recording-field");
  recordingPanel.classList.toggle("recording", phase === "recording");
  query("#config-test-recording-status").textContent = {
    idle: "准备录音", starting: "正在开启麦克风", recording: "正在录音",
    processing: "录音已停止", complete: "测试完成", error: "测试未完成",
  }[phase];
  if (phase === "recording" && !recordingTimer) recordingTimer = setInterval(updateRecordingTimer, 200);
  if (phase !== "recording" && recordingTimer) { clearInterval(recordingTimer); recordingTimer = null; }
  updateRecordingTimer(false);
  query("#config-test-results").hidden = phase !== "complete";
  query("#config-test-raw").value = configTestSession.result?.raw_transcript ?? "";
  query("#config-test-final").value = configTestSession.result?.final_text ?? "";
  renderConfigTestBusy();
  if (phase === "idle") setConfigTestStatus();
  else if (phase === "starting") setConfigTestStatus(`正在开启麦克风，请允许录音权限。${recordingNotice}`);
  else if (phase === "recording") {
    setConfigTestStatus(recordingNotice, false, Boolean(recordingNotice));
    if (recordingLimitReason) void configTestSession.stopRecording();
  } else if (phase === "processing") setConfigTestStatus(`${recordingNotice}${mode === "asr" ? "正在使用语音识别模型转写…" : mode === "voice" ? "正在识别音频并执行后处理…" : "正在使用当前配置处理文本…"}`);
  else if (phase === "error") setConfigTestStatus(`测试失败：${redactTestError(configTestSession.error, configTestSession.snapshot)}`, true);
  else if (phase === "complete") {
    const result = configTestSession.result;
    const warnings = [...new Set([...(result.warnings ?? []), ...(recordingNotice ? [recordingNotice] : [])])].map((warning) => redactTestError(warning, configTestSession.snapshot));
    setConfigTestStatus(`测试完成 · ${Math.round(result.elapsed_ms)} ms${warnings.length ? `。${warnings.join("；")}` : ""}`, false, warnings.length > 0);
    query("#config-test-status").classList.toggle("success", warnings.length === 0);
  }
}

async function startNativeTestRecording(snapshot, asrOnly, signal) {
  let sessionId = null;
  let unlisten = null;
  const pendingLimits = [];
  const releaseListener = async () => {
    const stop = unlisten;
    unlisten = null;
    signal.removeEventListener("abort", releaseListener);
    if (stop) { try { await stop(); } catch {} }
  };
  try {
    if (window.__TAURI__?.event?.listen) {
      try {
        unlisten = await window.__TAURI__.event.listen("test-recording-captured", (event) => {
          const payload = event.payload;
          if (signal.aborted || !payload?.session_id) return;
          if (!sessionId) pendingLimits.push(payload);
          else if (payload.session_id === sessionId) handleRecordingLimit(payload.reason === "size-limit" ? "size" : payload.reason === "duration-limit" ? "duration" : payload.reason);
        });
      } catch { recordingNotice = "录音状态通知不可用，仍可手动停止或等待时长上限。"; }
    } else recordingNotice = "录音状态通知不可用，仍可手动停止或等待时长上限。";
    if (signal.aborted) throw new Error("录音测试已取消");
    signal.addEventListener("abort", releaseListener, { once: true });
    sessionId = await tauriInvoke("start_test_recording", { config: snapshot, asrOnly });
    const limit = pendingLimits.find((event) => event.session_id === sessionId);
    if (limit && !signal.aborted) handleRecordingLimit(limit.reason === "size-limit" ? "size" : limit.reason === "duration-limit" ? "duration" : limit.reason);
    return {
      stop: async () => { await releaseListener(); return tauriInvoke("stop_test_recording", { sessionId }); },
      cancel: async () => { await releaseListener(); await tauriInvoke("cancel_test_recording", { sessionId }); },
    };
  } catch (error) { await releaseListener(); throw error; }
}

async function runConfigTest() {
  if (!editor || loading || ["starting", "processing"].includes(configTestSession.phase)) return;
  if (configTestSession.phase === "recording") { await configTestSession.stopRecording(); return; }
  collectForm();
  const snapshot = editor.snapshot();
  const mode = query("#config-test-mode").value;
  recordingNotice = "";
  recordingLimitReason = null;
  if (mode === "text") {
    const inputText = query("#config-test-text").value;
    resetConfigTest();
    if (!inputText.trim()) { setConfigTestStatus("请输入要测试的文本。", true); query("#config-test-text").focus(); return; }
    if (inputText.length > 100000) { setConfigTestStatus("测试文本不能超过 100,000 个字符。", true); return; }
    await configTestSession.runText(snapshot, inputText, (config, text, { signal }) => tauriInvoke
      ? tauriInvoke("test_config_input", { config, inputText: text, audioBytes: null, audioName: null })
      : testConfigInput(config, { inputText: text, signal }));
    return;
  }
  resetConfigTest();
  if (!snapshot.privacy.allow_external_requests) { setConfigTestStatus("当前隐私设置已禁止外部请求，请先在应用设置中允许外部 AI 请求。", true); return; }
  recordingLimitMs = 120000;
  const asrOnly = mode === "asr";
  await configTestSession.startRecording(snapshot, async (config, { signal }) => {
    if (tauriInvoke) return startNativeTestRecording(config, asrOnly, signal);
    const recording = await startBrowserRecording({ signal, maxDurationMs: recordingLimitMs, onLimit: ({ reason }) => { if (!signal.aborted) handleRecordingLimit(reason); } });
    return {
      cancel: () => recording.cancel(),
      stop: async () => {
        const audio = await recording.stop();
        const processingConfig = structuredClone(config);
        if (asrOnly) processingConfig.output.raw = true;
        return testConfigInput(processingConfig, { ...audio, signal });
      },
    };
  });
}

function setStatus(message, error = false) {
  query("#status").textContent = message;
  query("#status").classList.toggle("error", error);
}

function optional(value) {
  return value.trim() || null;
}

function renderConfigState() {
  for (const id of ["save", "config-select", "reload", "new-config", "import-config", "test-config"]) {
    query(`#${id}`).disabled = loading || saving || hotkeyCapture.busy;
  }
  query("#add-model").disabled = loading;
  query("#test-asr-recording").disabled = loading || saving;
  document.querySelectorAll(".nav-item").forEach((item) => { item.disabled = loading; });
  renderConnectionTest();
  for (const model of editor?.activeProfile.postprocess.models ?? []) renderConnectionTest(model.id);
  renderConfigTestBusy();
  renderHotkeyCapture();
  if (!editor) return;
  const state = query("#config-state");
  const dirty = editor.dirty;
  state.textContent = loading ? "载入中…" : saving ? "保存中…" : dirty ? "有未保存的修改" : "已保存";
  state.classList.toggle("dirty", dirty);
  const options = query("#config-select").options;
  editor.config.profiles.forEach((profile, index) => {
    if (options[index]) options[index].textContent = `${profile.name || "未命名配置"}${editor.profileDirty(profile.id) ? " · 未保存" : ""}`;
  });
  const count = query("#model-count");
  if (count) count.textContent = `${editor.activeProfile.postprocess.models.filter((model) => model.enabled).length} 个模型已启用`;
}

function renderConfigSelect() {
  const select = query("#config-select");
  select.replaceChildren();
  editor.config.profiles.forEach((profile) => {
    const option = document.createElement("option");
    option.value = profile.id;
    option.textContent = profile.name;
    select.append(option);
  });
  select.value = editor.config.active_profile_id;
  renderConfigState();
}

function fillForm() {
  void hotkeyCapture.cancel();
  clearConnectionTests();
  resetConfigTest(true);
  const profile = editor.activeProfile;
  const config = editor.config;
  query("#config-name").value = profile.name;
  fields.baseUrl.value = profile.asr.base_url;
  fields.model.value = profile.asr.model;
  fields.apiKey.value = profile.asr.api_key ?? "";
  renderKeyVisibility(fields.apiKey, query("#toggle-asr-api-key"));
  fields.postprocessMode.value = profile.postprocess.mode;
  fields.raw.checked = config.output.raw;
  fields.showChanges.checked = config.output.show_changes;
  fields.insert.checked = config.output.insert;
  fields.pasteDelayMs.value = config.output.paste_delay_ms;
  fields.restoreClipboard.checked = config.output.restore_clipboard;
  fields.restoreClipboardDelayMs.value = config.output.restore_clipboard_delay_ms;
  fields.allowExternalRequests.checked = config.privacy.allow_external_requests;
  fields.historyEnabled.checked = config.privacy.history_enabled;
  fields.historyPath.value = config.privacy.history_path ?? "";
  selectedModelId = profile.postprocess.models[0]?.id ?? null;
  renderConfigSelect();
  renderModels();
}

function collectForm() {
  if (!editor) return;
  const profile = editor.activeProfile;
  profile.name = query("#config-name").value;
  Object.assign(profile.asr, {
    base_url: fields.baseUrl.value.trim(), model: fields.model.value.trim(), protocol: "auto",
    api_key: optional(fields.apiKey.value),
  });
  profile.postprocess.mode = fields.postprocessMode.value;
  Object.assign(editor.config.output, {
    raw: fields.raw.checked,
    show_changes: fields.showChanges.checked, insert: fields.insert.checked,
    paste_delay_ms: Number(fields.pasteDelayMs.value || 0),
    restore_clipboard: fields.restoreClipboard.checked,
    restore_clipboard_delay_ms: Number(fields.restoreClipboardDelayMs.value || 0),
  });
  Object.assign(editor.config.privacy, {
    allow_external_requests: fields.allowExternalRequests.checked,
    history_enabled: fields.historyEnabled.checked, history_path: optional(fields.historyPath.value),
  });
}

function selectModel(id) {
  selectedModelId = id;
  query("#postprocess-nodes").querySelectorAll(".model-card").forEach((card) => {
    card.classList.toggle("selected", card.dataset.modelId === id);
  });
}

function element(tag, className = "", text = "") {
  const item = document.createElement(tag);
  item.className = className;
  if (text) item.textContent = text;
  return item;
}

function actionButton(label, title, action, disabled = false) {
  const button = element("button", "icon-button mini", label);
  button.type = "button";
  button.title = title;
  button.setAttribute("aria-label", title);
  button.disabled = disabled;
  button.addEventListener("click", (event) => { event.stopPropagation(); action(); });
  return button;
}

function enabledControl(checked, labelText, onChange) {
  const label = element("label", "node-enabled");
  const input = element("input");
  input.type = "checkbox";
  input.checked = checked;
  input.setAttribute("aria-label", labelText);
  input.addEventListener("change", () => onChange(input.checked));
  label.append(input, element("span", "", "启用"));
  return label;
}

function textField(labelText, value, onInput, options = {}) {
  const label = element(options.apiKey ? "div" : "label", ["service-field", options.fieldClass, options.multiline ? "textarea-field" : ""].filter(Boolean).join(" "));
  const control = element(options.multiline ? "textarea" : "input");
  if (options.multiline) control.rows = options.rows ?? 4;
  else control.type = options.apiKey ? "password" : "text";
  control.value = value ?? "";
  control.spellcheck = false;
  if (options.autocomplete === false) control.autocomplete = "off";
  if (options.placeholder) control.placeholder = options.placeholder;
  control.addEventListener("input", () => {
    onInput(control.value);
    if (options.testModelId) invalidateConnectionTest(options.testModelId);
    renderConfigState();
  });
  label.append(element("span", "", labelText));
  if (options.apiKey) {
    control.setAttribute("aria-label", labelText);
    const wrapper = element("div", "api-key-control");
    const toggle = element("button", "key-visibility-button");
    bindKeyVisibility(control, toggle);
    wrapper.append(control, toggle);
    label.append(wrapper);
  } else label.append(control);
  return label;
}

function refreshModelHeader(card, model, index) {
  card.querySelector(".model-title").textContent = model.name || `后处理模型 ${index + 1}`;
  card.querySelector(".model-summary").textContent = model.model || "";
  card.classList.toggle("disabled", !model.enabled);
}

function renderModels() {
  renderKeyVisibility(fields.apiKey, query("#toggle-asr-api-key"));
  const container = query("#postprocess-nodes");
  container.replaceChildren();
  const models = editor.activeProfile.postprocess.models;
  models.forEach((model, index) => {
    const card = element("article", "model-card");
    card.dataset.modelId = model.id;
    card.classList.toggle("selected", model.id === selectedModelId);
    const header = element("header", "model-card-header");
    const toggle = element("button", "model-toggle");
    toggle.type = "button";
    toggle.setAttribute("aria-expanded", String(!collapsedModels.has(model.id)));
    const heading = element("span", "model-heading");
    heading.append(element("strong", "model-title"), element("small", "model-summary"));
    toggle.append(element("span", "node-badge", String(index + 1).padStart(2, "0")), heading,
      element("span", "chevron", collapsedModels.has(model.id) ? "+" : "−"));
    const body = element("div", "model-card-body");
    body.classList.toggle("hidden", collapsedModels.has(model.id));
    toggle.addEventListener("click", () => {
      selectModel(model.id);
      if (collapsedModels.has(model.id)) collapsedModels.delete(model.id);
      else collapsedModels.add(model.id);
      body.classList.toggle("hidden", collapsedModels.has(model.id));
      toggle.setAttribute("aria-expanded", String(!collapsedModels.has(model.id)));
      toggle.querySelector(".chevron").textContent = collapsedModels.has(model.id) ? "+" : "−";
    });
    const controls = element("div", "node-controls");
    controls.append(
      enabledControl(model.enabled, `启用 ${model.name}`, (checked) => {
        model.enabled = checked; refreshModelHeader(card, model, index); renderConfigState();
      }),
      actionButton("↑", "上移模型", () => { editor.moveModel(model.id, -1); renderModels(); }, index === 0),
      actionButton("↓", "下移模型", () => { editor.moveModel(model.id, 1); renderModels(); }, index === models.length - 1),
      actionButton("×", models.length === 1 ? "至少保留一个模型，可关闭启用开关" : "删除模型", () => {
        invalidateConnectionTest(model.id);
        editor.removeModel(model.id);
        if (selectedModelId === model.id) selectedModelId = models[0]?.id;
        renderModels();
        setStatus("模型节点已删除");
      }, models.length === 1),
    );
    header.append(toggle, controls);
    const grid = element("div", "service-fields model-service-fields");
    grid.append(
      textField("服务地址", model.base_url, (value) => { model.base_url = value; }, { fieldClass: "service-url", testModelId: model.id, placeholder: "https://api.example.com/v1" }),
      textField("模型", model.model, (value) => { model.model = value; refreshModelHeader(card, model, index); }, { fieldClass: "service-model", testModelId: model.id, placeholder: "模型名称" }),
      textField("API Key", model.api_key, (value) => { model.api_key = optional(value); }, { fieldClass: "service-key", testModelId: model.id, apiKey: true, autocomplete: false, placeholder: "API Key" }),
      textField("Prompt", model.system_prompt, (value) => { model.system_prompt = value; }, {
        fieldClass: "service-prompt", testModelId: model.id, multiline: true, rows: 4, placeholder: "提示词",
      }),
    );
    const connectionRow = element("div", "service-actions");
    const connectionButton = element("button", "ghost test-service-button", "测试连接");
    connectionButton.type = "button";
    connectionButton.addEventListener("click", () => runConnectionTest(model.id));
    const connectionStatus = element("span", "service-test-result");
    connectionStatus.setAttribute("role", "status");
    connectionStatus.setAttribute("aria-live", "polite");
    connectionRow.append(connectionStatus, connectionButton);
    const promptHeading = element("div", "prompt-section-heading");
    promptHeading.append(element("h3", "", "Prompt 节点"));
    promptHeading.hidden = model.prompts.length === 0;
    const promptList = element("div", "prompt-list");
    promptList.hidden = model.prompts.length === 0;
    renderPrompts(promptList, model);
    const addPrompt = element("button", "add-prompt tonal", "+ 添加 Prompt 节点");
    addPrompt.type = "button";
    addPrompt.addEventListener("click", () => addPromptNode(model.id));
    body.append(grid, connectionRow, promptHeading, promptList, addPrompt);
    card.append(header, body);
    card.addEventListener("focusin", () => selectModel(model.id));
    container.append(card);
    refreshModelHeader(card, model, index);
  });
  container.classList.toggle("inactive", editor.activeProfile.postprocess.mode !== "llm");
  renderConfigState();
}

function renderPrompts(container, model) {
  model.prompts.forEach((prompt, index) => {
    const card = element("section", "prompt-card");
    card.dataset.promptId = prompt.id;
    card.classList.toggle("disabled", !prompt.enabled);
    const header = element("header", "prompt-card-header");
    const name = element("input", "prompt-name");
    name.value = prompt.name;
    name.setAttribute("aria-label", `Prompt ${index + 1} 名称`);
    name.addEventListener("input", () => { prompt.name = name.value; renderConfigState(); });
    const controls = element("div", "node-controls");
    controls.append(
      enabledControl(prompt.enabled, `启用 ${prompt.name}`, (checked) => {
        prompt.enabled = checked; card.classList.toggle("disabled", !checked); renderConfigState();
      }),
      actionButton("↑", "上移 Prompt", () => { editor.movePrompt(model.id, prompt.id, -1); renderModels(); }, index === 0),
      actionButton("↓", "下移 Prompt", () => { editor.movePrompt(model.id, prompt.id, 1); renderModels(); }, index === model.prompts.length - 1),
      actionButton("×", "删除 Prompt", () => { editor.removePrompt(model.id, prompt.id); renderModels(); }),
    );
    header.append(name, controls);
    const content = element("textarea", "prompt-content");
    content.rows = 4;
    content.value = prompt.content;
    content.placeholder = "提示词";
    content.setAttribute("aria-label", `${prompt.name} 内容`);
    content.addEventListener("input", () => { prompt.content = content.value; renderConfigState(); });
    card.append(header, content);
    container.append(card);
  });
}

function addModelNode() {
  if (!editor || loading) return;
  collectForm();
  const model = editor.addModel();
  selectedModelId = model.id;
  fields.postprocessMode.value = "llm";
  renderModels();
  const card = [...query("#postprocess-nodes").children].find((item) => item.dataset.modelId === model.id);
  card?.querySelector(".service-model input")?.focus();
  setStatus("已添加后处理模型");
}

function addPromptNode(modelId = selectedModelId) {
  if (!editor || !modelId || loading) return;
  collectForm();
  const prompt = editor.addPrompt(modelId);
  selectedModelId = modelId;
  collapsedModels.delete(modelId);
  renderModels();
  const card = [...query("#postprocess-nodes").querySelectorAll(".prompt-card")].find((item) => item.dataset.promptId === prompt.id);
  card?.querySelector("textarea")?.focus();
  setStatus("已添加 Prompt 节点");
}

function validForm() {
  for (const control of document.querySelectorAll("input[type=number]")) {
    if (!control.checkValidity()) { control.reportValidity(); return false; }
  }
  if (!editor.activeProfile.name.trim()) {
    query("#config-name").focus();
    setStatus("请填写当前配置名称", true);
    return false;
  }
  return true;
}

async function loadConfig() {
  if (loading || saving) return;
  await hotkeyCapture.cancel();
  if (hotkeyCapture.busy || loading || saving) return;
  if (editor?.dirty && !window.confirm("重新载入会放弃所有未保存的修改，继续吗？")) return;
  loading = true;
  configOperationVersion += 1;
  renderConfigState();
  try {
    let config;
    if (tauriInvoke) {
      config = await tauriInvoke("get_config");
    } else {
      const saved = localStorage.getItem(PREVIEW_STORAGE_KEY);
      config = saved ? JSON.parse(saved) : DEFAULT_CONFIG;
    }
    editor = new ConfigEditor(config);
    collapsedModels.clear();
    fillForm();
    setStatus("");
  } catch (error) {
    if (!editor) {
      editor = new ConfigEditor();
      editor.markUnsaved();
      fillForm();
      setStatus(`载入失败：${error}。已创建未保存的恢复草稿，可新建或导入配置。`, true);
    } else setStatus(`载入失败：${error}。当前草稿已保留。`, true);
  } finally {
    loading = false;
    renderConfigState();
    void syncSavedProfile();
  }
}

async function saveConfig() {
  if (!editor || saving || loading || hotkeyCapture.busy) return;
  collectForm();
  if (!validForm()) return;
  const snapshot = editor.snapshot();
  saving = true;
  configOperationVersion += 1;
  renderConfigState();
  try {
    if (tauriInvoke) await tauriInvoke("save_config", { config: snapshot });
    else localStorage.setItem(PREVIEW_STORAGE_KEY, JSON.stringify(snapshot));
    editor.markSaved(snapshot);
    hotkeyCapture.clearError();
    setStatus("配置已保存");
  } catch (error) { setStatus(`保存失败：${error}`, true); }
  finally { saving = false; renderConfigState(); void syncSavedProfile(); }
}

function downloadJson(value, filename) {
  const url = URL.createObjectURL(new Blob([JSON.stringify(value, null, 2)], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

async function importConfig(file) {
  if (!editor) return;
  try {
    const content = await file.text();
    let value;
    if (file.name.toLowerCase().endsWith(".toml")) {
      if (!tauriInvoke) throw new Error("TOML 导入需要桌面版；浏览器预览支持 JSON 配置");
      value = await tauriInvoke("parse_config_document", { content });
    } else value = JSON.parse(content.replace(/^\uFEFF/, ""));
    collectForm();
    const count = editor.import(value);
    fillForm();
    showPage("home");
    setStatus(`已导入 ${count} 个配置`);
  } catch (error) { setStatus(`导入失败：${error}`, true); }
  finally { query("#config-import-file").value = ""; }
}

function showPage(page) {
  void hotkeyCapture.cancel();
  document.querySelectorAll(".nav-item").forEach((item) => item.classList.toggle("active", item.dataset.page === page));
  document.querySelectorAll(".page").forEach((panel) => {
    panel.classList.toggle("active", panel.id === `${page}-page`);
    if (panel.id === `${page}-page`) query("#page-title").textContent = panel.dataset.title;
  });
}

function renderHotkeyCapture() {
  const button = query("#hotkey-capture");
  const hint = query("#hotkey-capture-state");
  const phase = hotkeyCapture.phase;
  button.disabled = loading || saving || phase === "restoring";
  button.classList.toggle("capturing", hotkeyCapture.busy);
  button.setAttribute("aria-pressed", String(hotkeyCapture.busy));
  button.setAttribute("aria-busy", String(["preparing", "restoring"].includes(phase)));
  button.textContent = {
    preparing: "准备录入…", capturing: "按下组合键…", restoring: "完成录入…",
  }[phase] ?? formatHotkey(phase === "awaiting-release" ? hotkeyCapture.staged : editor?.config.hotkey.preset ?? DEFAULT_CONFIG.hotkey.preset);
  const combination = formatHotkey(hotkeyCapture.staged ?? editor?.config.hotkey.preset ?? DEFAULT_CONFIG.hotkey.preset);
  button.setAttribute("aria-label", `录入开始或停止听写的快捷键，当前组合：${combination}`);
  query("#restore-shortcuts").disabled = loading || saving || hotkeyCapture.busy;
  hint.classList.toggle("error", hotkeyCapture.restorationFailed);
  if (phase === "idle" && hotkeyCapture.restorationFailed) hint.textContent = "原快捷键恢复失败，可保存新组合或重新录入。";
  else if (phase === "preparing") hint.textContent = "";
  else if (phase === "capturing") hint.textContent = "按下新的组合键，Esc 取消。";
  else if (phase === "awaiting-release") hint.textContent = "松开按键完成录入，Esc 取消。";
  else if (phase === "restoring") hint.textContent = "";
  else {
    const dirty = editor && (!editor.hasSavedBaseline || editor.config.hotkey.preset !== editor.saved.hotkey.preset);
    hint.textContent = dirty ? "快捷键未保存" : "";
  }
}

document.querySelectorAll(".nav-item").forEach((item) => item.addEventListener("click", () => showPage(item.dataset.page)));
query("#config-select").addEventListener("change", () => {
  if (!editor) return;
  collectForm();
  editor.selectProfile(query("#config-select").value);
  fillForm();
  setStatus("已切换配置");
});
query("#new-config").addEventListener("click", () => {
  if (!editor) return;
  collectForm();
  editor.newProfile();
  fillForm();
  showPage("home");
  query("#config-name").focus();
  query("#config-name").select();
  setStatus("已新建配置");
});
query("#export-config").addEventListener("click", () => {
  if (!editor) return;
  collectForm();
  const filename = editor.activeProfile.name.trim().replace(/[<>:"/\\|?*]/g, "-") || "orally-config";
  downloadJson(editor.exportProfile(), `${filename}.json`);
  setStatus("当前配置已导出为 JSON，包含填写的 API Key，请妥善保管");
});
query("#import-config").addEventListener("click", () => query("#config-import-file").click());
query("#config-import-file").addEventListener("change", (event) => {
  const [file] = event.target.files;
  if (file) importConfig(file);
});
query("#add-model").addEventListener("click", addModelNode);
bindKeyVisibility(fields.apiKey, query("#toggle-asr-api-key"));
query("#test-asr-connection").addEventListener("click", () => runConnectionTest());
query("#test-asr-recording").addEventListener("click", () => openConfigTest("asr"));
query("#test-config").addEventListener("click", () => openConfigTest("voice"));
query("#run-config-test").addEventListener("click", runConfigTest);
query("#config-test-close").addEventListener("click", () => query("#config-test-dialog").close());
query("#cancel-config-test").addEventListener("click", () => {
  resetConfigTest();
  setConfigTestStatus("测试已取消，可以重新开始。");
});
query("#config-test-dialog").addEventListener("cancel", () => resetConfigTest());
query("#config-test-dialog").addEventListener("close", () => resetConfigTest());
query("#config-test-mode").addEventListener("change", () => {
  resetConfigTest();
  renderConfigTestMode();
});
query("#config-test-text").addEventListener("input", () => resetConfigTest());
for (const field of [query("#config-name"), ...Object.values(fields)]) {
  field.addEventListener("input", () => {
    collectForm();
    if ([fields.baseUrl, fields.model, fields.apiKey].includes(field)) invalidateConnectionTest();
    if (field === fields.allowExternalRequests) clearConnectionTests();
    if (field === fields.postprocessMode) query("#postprocess-nodes").classList.toggle("inactive", field.value !== "llm");
    renderConfigState();
  });
}
query("#hotkey-capture").addEventListener("click", () => {
  if (!editor || loading || saving) return;
  if (hotkeyCapture.busy) void hotkeyCapture.cancel();
  else void hotkeyCapture.start();
});
query("#hotkey-capture").addEventListener("blur", () => {
  if (["preparing", "capturing", "awaiting-release"].includes(hotkeyCapture.phase)) void hotkeyCapture.cancel();
});
document.addEventListener("keydown", (event) => {
  if (hotkeyCapture.busy) {
    event.preventDefault();
    event.stopImmediatePropagation();
    void hotkeyCapture.handleKey(event);
    return;
  }
  if (event.repeat || !editor || query("#config-test-dialog").open) return;
  if (event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey && (event.code === "KeyS" || event.key.toLowerCase() === "s")) {
    event.preventDefault();
    void saveConfig();
  }
}, { capture: true });
document.addEventListener("keyup", (event) => {
  if (!hotkeyCapture.busy) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  void hotkeyCapture.handleKeyUp(event);
}, { capture: true });
query("#restore-shortcuts").addEventListener("click", () => {
  if (!editor || hotkeyCapture.busy || loading || saving) return;
  collectForm();
  editor.config.hotkey.preset = DEFAULT_CONFIG.hotkey.preset;
  renderConfigState();
  setStatus("快捷键已恢复默认");
});
query("#reload").addEventListener("click", loadConfig);
query("#save").addEventListener("click", saveConfig);
window.addEventListener("blur", () => { void hotkeyCapture.cancel(); });
document.addEventListener("visibilitychange", () => { if (document.hidden) void hotkeyCapture.cancel(); });
window.addEventListener("beforeunload", (event) => {
  void hotkeyCapture.cancel();
  if (!editor?.dirty) return;
  event.preventDefault();
  event.returnValue = "";
});

renderHotkeyCapture();
const profileEventError = await initializeProfileEvents();
await loadConfig();
if (profileEventError) setStatus(`托盘配置同步未启用：${profileEventError}`, true);
window.addEventListener("pagehide", () => {
  void hotkeyCapture.cancel();
  resetConfigTest();
  clearConnectionTests();
  stopProfileEvents?.();
});
