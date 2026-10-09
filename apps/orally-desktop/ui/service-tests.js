const REQUEST_TIMEOUT_MS = 30_000;
const MAX_AUDIO_BYTES = 15 * 1024 * 1024;
const MAX_TEXT_LENGTH = 100_000;

function now() {
  return globalThis.performance?.now() ?? Date.now();
}

function secrets(config) {
  const values = new Set();
  function visit(value) {
    if (!value || typeof value !== "object") return;
    for (const [key, item] of Object.entries(value)) {
      if (key === "api_key" && typeof item === "string" && item.trim()) {
        values.add(item);
        values.add(item.trim());
      }
      else if (item && typeof item === "object") visit(item);
    }
  }
  visit(config);
  return [...values].sort((left, right) => right.length - left.length);
}

function safeMessage(error, config) {
  let message = error instanceof Error ? error.message : String(error);
  for (const key of secrets(config)) {
    message = message.replaceAll(key, "[已隐藏]");
    message = message.replaceAll(encodeURIComponent(key), "[已隐藏]");
    message = message.replaceAll(JSON.stringify(key).slice(1, -1), "[已隐藏]");
  }
  return message.replace(/Bearer\s+[^\s"']+/gi, "Bearer [已隐藏]").slice(0, 800);
}

async function safely(config, operation) {
  try {
    return await operation();
  } catch (error) {
    throw new Error(safeMessage(error, config));
  }
}

function allowExternalRequests(config) {
  if (config.privacy?.allow_external_requests === false) {
    throw new Error("隐私设置已禁止外部请求，请先允许外部请求后再测试服务。");
  }
}

function checkCancelled(signal) {
  if (signal?.aborted) throw new Error("测试已取消");
}

function validateService(service, label) {
  if (!service || typeof service.base_url !== "string" || !service.base_url.trim()) {
    throw new Error(`${label}的服务地址不能为空`);
  }
  let url;
  try { url = new URL(service.base_url.trim()); } catch { throw new Error(`${label}的服务地址格式不正确`); }
  if (!["http:", "https:"].includes(url.protocol)) throw new Error(`${label}的服务地址需要使用 HTTP 或 HTTPS`);
  if (typeof service.model !== "string" || !service.model.trim()) throw new Error(`${label}的模型不能为空`);
  if (typeof service.api_key !== "string" || !service.api_key.trim()) {
    throw new Error(`${label}缺少 API Key。浏览器预览无法读取环境变量，请填写 API Key 或在原生应用中测试。`);
  }
}

function endpoint(baseUrl, path) {
  const url = new URL(baseUrl.trim());
  const basePath = url.pathname.replace(/\/+$/, "").replace(/\/(?:audio\/transcriptions|chat\/completions)$/, "");
  url.pathname = `${basePath}${path}`;
  url.hash = "";
  return url.toString();
}

class ServiceHttpError extends Error {
  constructor(label, response) {
    super(`${label}返回 HTTP ${response.status}${response.statusText ? `：${response.statusText}` : ""}`);
    this.status = response.status;
  }
}

async function requestJson(service, path, body, { label, signal, multipart = false }) {
  checkCancelled(signal);
  const controller = new AbortController();
  let timedOut = false;
  const cancel = () => controller.abort();
  signal?.addEventListener("abort", cancel, { once: true });
  const timeout = setTimeout(() => { timedOut = true; controller.abort(); }, REQUEST_TIMEOUT_MS);
  try {
    let response;
    let text;
    try {
      response = await fetch(endpoint(service.base_url, path), {
        method: "POST",
        headers: {
          Authorization: `Bearer ${service.api_key}`,
          ...(!multipart ? { "Content-Type": "application/json" } : {}),
        },
        body: multipart ? body : JSON.stringify(body),
        signal: controller.signal,
      });
      text = await response.text();
    } catch (error) {
      if (signal?.aborted) throw new Error("测试已取消");
      if (timedOut) throw new Error(`${label}请求超时（30 秒）`);
      throw new Error(`${label}网络请求失败：${error.message ?? error}。浏览器可能受到 CORS 限制，可在原生应用中测试。`);
    }
    checkCancelled(signal);
    if (!response.ok) throw new ServiceHttpError(label, response);
    try { return JSON.parse(text); } catch { throw new Error(`${label}返回的内容不是有效 JSON`); }
  } finally {
    clearTimeout(timeout);
    signal?.removeEventListener("abort", cancel);
  }
}

function silentWav() {
  const sampleRate = 16_000;
  const sampleBytes = sampleRate; // Half a second of mono PCM16 audio.
  const bytes = new Uint8Array(44 + sampleBytes);
  const view = new DataView(bytes.buffer);
  function tag(offset, value) { for (let index = 0; index < value.length; index += 1) bytes[offset + index] = value.charCodeAt(index); }
  tag(0, "RIFF");
  view.setUint32(4, bytes.length - 8, true);
  tag(8, "WAVE");
  tag(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  tag(36, "data");
  view.setUint32(40, sampleBytes, true);
  return bytes;
}

function audioBytes(value) {
  let bytes;
  if (value instanceof ArrayBuffer) bytes = new Uint8Array(value);
  else if (ArrayBuffer.isView(value)) bytes = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  else if (Array.isArray(value)) {
    if (value.length > MAX_AUDIO_BYTES) throw new Error("测试音频不能超过 15 MiB");
    if (!value.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)) throw new Error("音频数据格式不正确");
    bytes = Uint8Array.from(value);
  } else throw new Error("请选择 WAV 音频进行测试");
  if (bytes.byteLength > MAX_AUDIO_BYTES) throw new Error("测试音频不能超过 15 MiB");
  const tag = (offset, name) => [...name].every((character, index) => bytes[offset + index] === character.charCodeAt(0));
  if (bytes.length < 44 || !tag(0, "RIFF") || !tag(8, "WAVE")) throw new Error("测试音频需要是有效的 WAV 文件");
  return bytes;
}

async function recognize(asr, bytes, name, signal, allowEmpty = false) {
  validateService(asr, "语音识别服务");
  const form = new FormData();
  form.append("model", asr.model);
  form.append("response_format", "json");
  form.append("file", new Blob([bytes], { type: "audio/wav" }), (name || "orally-test.wav").split(/[\\/]/).at(-1));
  if (asr.language != null) form.append("language", asr.language);
  let data;
  try {
    data = await requestJson(asr, "/audio/transcriptions", form, { label: "语音识别服务", signal, multipart: true });
  } catch (error) {
    if (!(error instanceof ServiceHttpError) || ![404, 405].includes(error.status)) throw error;
    return recognizeChatAudio(asr, bytes, signal, allowEmpty);
  }
  return transcript(data?.text, allowEmpty);
}

function transcript(text, allowEmpty) {
  if (typeof text !== "string") throw new Error("语音识别服务返回的转录格式不正确");
  if (!text.trim() && !allowEmpty) throw new Error("语音识别服务返回空转录文本");
  return text.trim();
}

function encodedAudio(bytes) {
  const chunks = [];
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    chunks.push(String.fromCharCode(...bytes.subarray(offset, offset + 0x8000)));
  }
  return btoa(chunks.join(""));
}

async function recognizeChatAudio(asr, bytes, signal, allowEmpty) {
  const encoded = encodedAudio(bytes);
  const context = asr.language?.trim() ? `Language hint: ${asr.language.trim()}` : "";
  const messages = context ? [{ role: "system", content: context }] : [];
  messages.push({ role: "user", content: [{ type: "input_audio", input_audio: { data: `data:audio/wav;base64,${encoded}` } }] });
  let data;
  try {
    data = await requestJson(asr, "/chat/completions", { model: asr.model, messages, stream: false }, { label: "语音识别服务", signal });
  } catch (error) {
    if (!(error instanceof ServiceHttpError) || ![400, 422].includes(error.status)) throw error;
    let instruction = "Please transcribe this audio exactly. Return only the transcript text.";
    if (asr.language?.trim()) instruction += `\nLanguage hint: ${asr.language.trim()}`;
    data = await requestJson(asr, "/chat/completions", {
      model: asr.model, stream: false,
      messages: [{ role: "user", content: [
        { type: "text", text: instruction },
        { type: "input_audio", input_audio: { data: encoded, format: "wav" } },
      ] }],
    }, { label: "语音识别服务", signal });
  }
  return transcript(data?.choices?.[0]?.message?.content, allowEmpty);
}

function composedPrompt(model) {
  return [model.system_prompt ?? "", ...(model.prompts ?? []).filter((prompt) => prompt.enabled !== false).map((prompt) => prompt.content ?? "")]
    .filter((content) => content.trim()).join("\n\n");
}

function sanitizeModelText(value) {
  let text = value.trim();
  if (text.startsWith("```")) {
    text = text.replace(/^`+/, "").trim();
    for (const language of ["text", "markdown"]) {
      if (text.startsWith(language)) text = text.slice(language.length).replace(/^[\r\n ]+/, "");
    }
    text = text.replace(/`+$/, "").trim();
  }
  for (const prefix of ["Final text:", "Final:", "Output:", "Text:", "Result:", "最终文本：", "输出：", "结果："]) {
    if (text.startsWith(prefix)) { text = text.slice(prefix.length).trim(); break; }
  }
  if ((text.startsWith('"') && text.endsWith('"')) || (text.startsWith("“") && text.endsWith("”"))) text = text.slice(1, -1);
  return text.trim();
}

async function chat(model, messages, signal) {
  const data = await requestJson(model, "/chat/completions", {
    model: model.model, messages, temperature: 0.1, stream: false,
  }, { label: "模型服务", signal });
  const content = data.choices?.[0]?.message?.content;
  const text = typeof content === "string" ? sanitizeModelText(content) : "";
  if (!text) throw new Error("模型服务返回空文本");
  return text;
}

function localCleanup(value, locale) {
  const fillers = new Set(["嗯", "呃", "那个", "就是", "uh", "um", "er"]);
  let text = value.split(/\s+/u).filter((word) => word && !fillers.has(word)).join(" ");
  for (const [spoken, written] of [["visual studio code", "Visual Studio Code"], ["orally", "Orally"]]) text = text.replaceAll(spoken, written);
  if (text && !/[.!?。！？]$/u.test(text)) text += locale.startsWith("zh") ? "。" : ".";
  return text;
}

function activeModels(config) {
  const postprocess = config.postprocess;
  return postprocess.models?.length ? postprocess.models.filter((model) => model.enabled !== false) : [postprocess];
}

async function refine(config, rawText, signal) {
  checkCancelled(signal);
  const locale = config.output?.locale ?? "zh-CN";
  if (config.output?.raw) return { text: rawText, warnings: [] };
  if (!["llm", "ai"].includes(config.postprocess?.mode)) return { text: localCleanup(rawText, locale), warnings: [] };
  const models = activeModels(config);
  if (models.length) allowExternalRequests(config);
  const warnings = [];
  let text = rawText;
  for (const model of models) {
    checkCancelled(signal);
    // Configuration errors do not use runtime fallback; only a failed request does.
    validateService(model, "模型服务");
    try {
      text = await chat(model, [
        { role: "system", content: composedPrompt(model) },
        { role: "user", content: (model.user_template ?? "{{transcript}}").replaceAll("{{transcript}}", text).replaceAll("{{locale}}", locale) },
      ], signal);
    } catch (error) {
      if (signal?.aborted || model.fallback_to_builtin === false) throw error;
      text = localCleanup(text, locale);
      warnings.push(safeMessage(`${model.name || model.model} 请求失败，已使用本地基础清理：${error.message ?? error}`, config));
    }
  }
  return { text, warnings };
}

export async function testServiceConnection(config, modelId = null, { signal = null } = {}) {
  return safely(config, async () => {
    const started = now();
    checkCancelled(signal);
    allowExternalRequests(config);
    if (modelId === null) {
      await recognize(config.asr, silentWav(), "orally-connection-test.wav", signal, true);
      return { message: "语音识别服务连接成功（已发送 0.5 秒静音 WAV）", elapsed_ms: Math.round(now() - started) };
    }
    const model = config.postprocess.models?.length
      ? config.postprocess.models.find((item) => item.id === modelId)
      : modelId === "" ? config.postprocess : null;
    if (!model) throw new Error("要测试的模型节点不存在");
    validateService(model, "模型服务");
    await chat(model, [
      { role: "system", content: "This is a service connection test. Return only OK." },
      { role: "user", content: "Return OK." },
    ], signal);
    return { message: "模型服务连接成功（已完成真实请求）", elapsed_ms: Math.round(now() - started) };
  });
}

export async function testConfigInput(config, { inputText = null, audioBytes: suppliedAudio = null, audioName = null, signal = null } = {}) {
  return safely(config, async () => {
    const started = now();
    checkCancelled(signal);
    if ((inputText !== null) === (suppliedAudio !== null)) throw new Error("请提供文字或 WAV 音频进行测试，不能同时提供两者");
    let rawText;
    if (suppliedAudio !== null) {
      allowExternalRequests(config);
      rawText = await recognize(config.asr, audioBytes(suppliedAudio), audioName, signal);
    } else {
      if (typeof inputText !== "string" || !inputText.trim()) throw new Error("测试文字不能为空");
      if (inputText.length > MAX_TEXT_LENGTH) throw new Error("测试文字不能超过 100,000 个字符");
      rawText = inputText;
    }
    const result = await refine(config, rawText, signal);
    return { raw_transcript: rawText, final_text: result.text, elapsed_ms: Math.round(now() - started), warnings: result.warnings };
  });
}
