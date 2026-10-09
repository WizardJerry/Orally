import test from "node:test";
import assert from "node:assert/strict";
import { ConfigEditor } from "../ui/editor-state.js";
import { testServiceConnection, testConfigInput } from "../ui/service-tests.js";

function configuration() {
  const editor = new ConfigEditor();
  Object.assign(editor.activeProfile.asr, {
    base_url: "https://asr.example/v1/", model: "speech-model", api_key: "fake-asr-key",
    protocol: "auto", language: "zh",
  });
  Object.assign(editor.activeProfile.postprocess.models[0], {
    base_url: "https://llm.example/v1/", model: "first-model", api_key: "fake-model-key",
    system_prompt: "Preserve meaning.", user_template: "Locale: {{locale}}\n{{transcript}}",
  });
  return editor.snapshot();
}

function json(value, status = 200) {
  return new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
}

function completion(text) {
  return json({ choices: [{ message: { content: text } }] });
}

function wav() {
  const bytes = new Uint8Array(46);
  const view = new DataView(bytes.buffer);
  for (const [offset, text] of [[0, "RIFF"], [8, "WAVE"], [12, "fmt "], [36, "data"]]) {
    [...text].forEach((character, index) => { bytes[offset + index] = character.charCodeAt(0); });
  }
  view.setUint32(4, 38, true);
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, 16000, true);
  view.setUint32(28, 32000, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  view.setUint32(40, 2, true);
  return bytes;
}

async function fakeFetch(responses, operation) {
  const original = globalThis.fetch;
  const calls = [];
  const queue = [...responses];
  globalThis.fetch = async (url, options) => {
    calls.push({ url, ...options });
    assert.ok(queue.length, "unexpected external request");
    const response = queue.shift();
    return typeof response === "function" ? response(url, options) : response;
  };
  try {
    await operation(calls);
    assert.equal(queue.length, 0, "not all expected requests were made");
  } finally {
    globalThis.fetch = original;
  }
}

test("ASR connection submits an authenticated half-second WAV and accepts a valid empty silence transcript", async () => {
  const config = configuration();
  config.asr.prompt = "Deprecated recognition instructions must not be sent.";
  const before = structuredClone(config);
  await fakeFetch([json({ text: "", language: "zh" })], async (calls) => {
    const result = await testServiceConnection(config);
    assert.match(result.message, /连接成功/);
    assert.ok(result.elapsed_ms >= 0);
    const request = calls[0];
    assert.equal(request.url, "https://asr.example/v1/audio/transcriptions");
    assert.equal(request.headers.Authorization, "Bearer fake-asr-key");
    assert.equal(request.headers["Content-Type"], undefined, "the browser supplies the multipart boundary");
    assert.equal(request.body.get("model"), "speech-model");
    assert.equal(request.body.get("language"), "zh");
    assert.equal(request.body.get("prompt"), null);
    assert.equal(request.body.get("response_format"), "json");
    const uploaded = await request.body.get("file").arrayBuffer();
    assert.equal(uploaded.byteLength, 16044);
    assert.equal(new DataView(uploaded).getUint32(24, true), 16000);
    assert.equal(new DataView(uploaded).getUint32(40, true), 16000);
    assert.deepEqual(config, before, "testing never changes the draft");
  });
});

test("a missing transcription endpoint negotiates WAV Data URL chat using the complete unsaved ASR draft", async () => {
  const editor = new ConfigEditor(configuration());
  Object.assign(editor.activeProfile.asr, {
    model: "unsaved-speech-model", api_key: "fake-draft-key",
  });
  editor.config.output.raw = true;
  assert.equal(editor.dirty, true);
  const config = editor.snapshot();
  config.asr.prompt = "Deprecated recognition instructions must not be sent.";
  const before = structuredClone(config);
  await fakeFetch([json({ error: {} }, 404), completion("recognized from draft")], async (calls) => {
    const result = await testConfigInput(config, { audioBytes: wav(), audioName: "draft.wav" });
    assert.equal(result.raw_transcript, "recognized from draft");
    assert.equal(result.final_text, "recognized from draft");
    assert.deepEqual(result.warnings, []);
    assert.equal(calls[0].url, "https://asr.example/v1/audio/transcriptions");
    assert.deepEqual(new Uint8Array(await calls[0].body.get("file").arrayBuffer()), wav());
    assert.equal(calls[1].url, "https://asr.example/v1/chat/completions");
    assert.equal(calls[1].headers.Authorization, "Bearer fake-draft-key");
    assert.deepEqual(JSON.parse(calls[1].body), {
      model: "unsaved-speech-model", stream: false,
      messages: [
        { role: "system", content: "Language hint: zh" },
        { role: "user", content: [{ type: "input_audio", input_audio: { data: `data:audio/wav;base64,${Buffer.from(wav()).toString("base64")}` } }] },
      ],
    });
    assert.deepEqual(config, before);
    assert.equal(editor.dirty, true, "testing does not acknowledge an explicit save");
  });
});

test("Data URL shape rejection alone negotiates the standard raw WAV chat shape", async () => {
  for (const [missingStatus, shapeStatus] of [[404, 400], [405, 422]]) {
    const config = configuration();
    config.asr.prompt = "Deprecated recognition instructions must not be sent.";
    config.output.raw = true;
    await fakeFetch([json({ error: {} }, missingStatus), json({ error: {} }, shapeStatus), completion("raw-shape transcript")], async (calls) => {
      const result = await testConfigInput(config, { audioBytes: wav() });
      assert.equal(result.final_text, "raw-shape transcript");
      assert.equal(calls.length, 3);
      assert.deepEqual(JSON.parse(calls[2].body), {
        model: "speech-model", stream: false,
        messages: [{ role: "user", content: [
          { type: "text", text: "Please transcribe this audio exactly. Return only the transcript text.\nLanguage hint: zh" },
          { type: "input_audio", input_audio: { data: Buffer.from(wav()).toString("base64"), format: "wav" } },
        ] }],
      });
      assert.ok(calls.slice(1).every((call) => call.url === "https://asr.example/v1/chat/completions"));
      assert.ok(calls.every((call) => call.headers.Authorization === "Bearer fake-asr-key"));
    });
  }
});

test("audio-only Data URL negotiation keeps silence valid without inventing a system instruction", async () => {
  const config = configuration();
  config.asr.language = null;
  await fakeFetch([json({ error: {} }, 405), completion("")], async (calls) => {
    assert.match((await testServiceConnection(config)).message, /连接成功/);
    const payload = JSON.parse(calls[1].body);
    assert.equal(payload.messages.length, 1);
    const user = payload.messages[0];
    assert.equal(user.role, "user");
    assert.equal(user.content.length, 1);
    const part = user.content[0];
    assert.equal(part.type, "input_audio");
    assert.deepEqual(Object.keys(part.input_audio), ["data"]);
    const data = part.input_audio.data;
    assert.ok(data.startsWith("data:audio/wav;base64,"));
    const decoded = Buffer.from(data.slice("data:audio/wav;base64,".length), "base64");
    assert.deepEqual(decoded, Buffer.from(await calls[0].body.get("file").arrayBuffer()));
    assert.equal(decoded.length, 16044);
    assert.equal(decoded.readUInt32LE(24), 16000);
  });
});

test("raw WAV compatibility uses a transcription instruction when the draft contains no context", async () => {
  const config = configuration();
  config.asr.language = null;
  config.output.raw = true;
  await fakeFetch([json({ error: {} }, 404), json({ error: {} }, 422), completion("raw transcript")], async (calls) => {
    assert.equal((await testConfigInput(config, { audioBytes: wav() })).final_text, "raw transcript");
    assert.equal(JSON.parse(calls[2].body).messages[0].content[0].text, "Please transcribe this audio exactly. Return only the transcript text.");
  });
});

test("LLM connection tests a disabled selected node using a minimal prompt and never falls back", async () => {
  const config = configuration();
  const model = config.postprocess.models[0];
  model.enabled = false;
  model.system_prompt = "This user instruction must not be used by connection tests.";
  model.user_template = "Nor should this template.";
  await fakeFetch([completion("OK")], async (calls) => {
    assert.match((await testServiceConnection(config, model.id)).message, /连接成功/);
    const payload = JSON.parse(calls[0].body);
    assert.equal(calls[0].url, "https://llm.example/v1/chat/completions");
    assert.equal(calls[0].headers.Authorization, "Bearer fake-model-key");
    assert.deepEqual(payload.messages, [
      { role: "system", content: "This is a service connection test. Return only OK." },
      { role: "user", content: "Return OK." },
    ]);
    assert.equal(payload.model, "first-model");
    assert.equal(payload.stream, false);
  });
  await fakeFetch([json({ error: { message: "invalid credential fake-model-key" } }, 401)], async () => {
    await assert.rejects(testServiceConnection(config, model.id), (error) => {
      assert.match(error.message, /HTTP 401/);
      assert.ok(!error.message.includes("fake-model-key"));
      return true;
    });
  });
});

test("audio input uses real ASR followed by enabled models and composed Prompts in their visible order", async () => {
  const config = configuration();
  const first = config.postprocess.models[0];
  first.prompts = [
    { content: "First instruction.", enabled: true },
    { content: "Ignore this disabled instruction.", enabled: false },
    { content: " \n ", enabled: true },
    { content: "Last instruction.", enabled: true },
  ];
  config.postprocess.models.push(
    { ...first, id: "disabled", enabled: false, api_key: null },
    { ...first, id: "second", model: "second-model", api_key: "fake-second-key", user_template: "Previous: {{transcript}}", prompts: [] },
  );
  const before = structuredClone(config);
  await fakeFetch([json({ text: "Raw transcript" }), completion("first output"), completion('Final: "final output"')], async (calls) => {
    const result = await testConfigInput(config, { audioBytes: [...wav()], audioName: "sample.wav" });
    assert.equal(result.raw_transcript, "Raw transcript");
    assert.equal(result.final_text, "final output");
    assert.deepEqual(result.warnings, []);
    assert.equal(calls.length, 3);
    const firstPayload = JSON.parse(calls[1].body);
    assert.equal(firstPayload.model, "first-model");
    assert.equal(firstPayload.messages[0].content, "Preserve meaning.\n\nFirst instruction.\n\nLast instruction.");
    assert.equal(firstPayload.messages[1].content, "Locale: zh-CN\nRaw transcript");
    const secondPayload = JSON.parse(calls[2].body);
    assert.equal(secondPayload.model, "second-model");
    assert.equal(secondPayload.messages[1].content, "Previous: first output");
    assert.equal(calls[2].headers.Authorization, "Bearer fake-second-key");
    assert.deepEqual(config, before);
  });
});

test("every ASR host, model and legacy protocol starts with standard multipart transcriptions", async () => {
  for (const [baseUrl, model, legacyProtocol] of [
    ["https://speech.example/v1", "audio-model", "chat-audio"],
    ["https://gateway.example/api/v1", "audio-model", "openai-transcriptions"],
    ["https://speech.example/v1", "custom-speech-large", "multipart"],
    ["https://custom-service.example/v1", "audio-model", "chat-completions"],
    ["https://speech.example/v1", "custom-audio-model", "auto"],
  ]) {
    const config = configuration();
    Object.assign(config.asr, { base_url: baseUrl, model, protocol: legacyProtocol });
    config.output.raw = true;
    await fakeFetch([json({ text: "recognized" })], async (calls) => {
      const result = await testConfigInput(config, { audioBytes: wav() });
      assert.equal(result.final_text, "recognized");
      assert.equal(calls[0].url, `${baseUrl}/audio/transcriptions`);
      assert.ok(calls[0].body instanceof FormData);
      assert.equal(calls[0].body.get("model"), model);
      assert.equal(calls[0].body.get("language"), "zh");
      assert.equal(calls[0].body.get("prompt"), null);
      assert.equal(calls[0].headers.Authorization, "Bearer fake-asr-key");
      assert.equal(calls[0].headers["Content-Type"], undefined);
    });
  }
});

test("non-negotiable ASR failures never reach chat or post-processing fallback", async () => {
  for (const status of [400, 401, 403, 415, 422, 429, 500, 503]) {
    const config = configuration();
    await fakeFetch([json({ error: { message: "unsupported fake-asr-key" } }, status), json({ error: {} }, status)], async (calls) => {
      const failure = (error) => {
        assert.match(error.message, new RegExp(`HTTP ${status}`));
        assert.ok(!error.message.includes("fake-asr-key"));
        return true;
      };
      await assert.rejects(testServiceConnection(config), failure);
      await assert.rejects(testConfigInput(config, { audioBytes: wav() }), failure);
      assert.equal(calls.length, 2);
      assert.ok(calls.every((call) => call.url === "https://asr.example/v1/audio/transcriptions"));
    });
  }
});

test("successful malformed or empty ASR responses do not negotiate another audio shape", async () => {
  const config = configuration();
  for (const [responses, pattern] of [
    [[json({ wrong: "shape" })], /格式不正确/],
    [[json(null)], /格式不正确/],
    [[new Response("not JSON")], /有效 JSON/],
    [[json({ text: " " })], /空转录/],
    [[json({ error: {} }, 404), json({ choices: [] })], /格式不正确/],
    [[json({ error: {} }, 404), json({ choices: [{ message: { content: null } }] })], /格式不正确/],
    [[json({ error: {} }, 404), new Response("not JSON")], /有效 JSON/],
    [[json({ error: {} }, 404), completion("")], /空转录/],
  ]) {
    await fakeFetch(responses, async (calls) => {
      await assert.rejects(testConfigInput(config, { audioBytes: wav() }), pattern);
      assert.equal(calls.length, responses.length);
    });
  }
  await fakeFetch([json({ error: {} }, 404), json({ choices: [] })], async () => {
    await assert.rejects(testServiceConnection(config), /格式不正确/);
  });
});

test("chat audio errors other than Data URL shape rejection never negotiate or refine", async () => {
  const config = configuration();
  for (const status of [401, 403, 404, 405, 429, 500, 503]) {
    await fakeFetch([json({ error: {} }, 404), json({ error: { message: "fake-asr-key" } }, status)], async () => {
      await assert.rejects(testConfigInput(config, { audioBytes: wav() }), (error) => {
        assert.match(error.message, new RegExp(`HTTP ${status}`));
        assert.ok(!error.message.includes("fake-asr-key"));
        return true;
      });
    });
  }
  await fakeFetch([json({ error: {} }, 404), json({ error: {} }, 400), json({ error: {} }, 503)], async () => {
    await assert.rejects(testConfigInput(config, { audioBytes: wav() }), /HTTP 503/);
  });
  for (const responses of [
    [() => { throw new TypeError("Failed to fetch fake-asr-key"); }],
    [json({ error: {} }, 404), () => { throw new TypeError("Failed to fetch fake-asr-key"); }],
  ]) {
    await fakeFetch(responses, async () => {
      await assert.rejects(testConfigInput(config, { audioBytes: wav() }), (error) => {
        assert.match(error.message, /CORS/);
        assert.ok(!error.message.includes("fake-asr-key"));
        return true;
      });
    });
  }
});

test("cancellation at any ASR negotiation stage aborts without another request or local fallback", async () => {
  const config = configuration();
  const afterMultipart = new AbortController();
  await fakeFetch([() => { afterMultipart.abort(); return json({ error: {} }, 404); }], async () => {
    await assert.rejects(testConfigInput(config, { audioBytes: wav(), signal: afterMultipart.signal }), /已取消/);
  });
  for (const preceding of [[json({ error: {} }, 404)], [json({ error: {} }, 404), json({ error: {} }, 422)]]) {
    const controller = new AbortController();
    const abortedRequest = (_url, options) => new Promise((_resolve, reject) => {
      options.signal.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), { once: true });
      controller.abort();
    });
    await fakeFetch([...preceding, abortedRequest], async () => {
      await assert.rejects(testConfigInput(config, { audioBytes: wav(), signal: controller.signal }), /已取消/);
    });
  }
});

test("chat audio timeout aborts negotiation instead of switching shape or reporting success", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  let enteredRequest;
  const reachedChat = new Promise((resolve) => { enteredRequest = resolve; });
  const waiting = (_url, options) => new Promise((_resolve, reject) => {
    options.signal.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), { once: true });
    enteredRequest();
  });
  await fakeFetch([json({ error: {} }, 404), waiting], async () => {
    const pending = testConfigInput(configuration(), { audioBytes: wav() });
    const rejected = assert.rejects(pending, /语音识别服务请求超时（30 秒）/);
    await reachedChat;
    context.mock.timers.tick(30_000);
    await rejected;
  });
});

test("text input bypasses ASR and explicit fallback produces a warning, while strict failures reject", async () => {
  const config = configuration();
  await fakeFetch([json({ error: { message: "failed fake-model-key" } }, 503)], async () => {
    const result = await testConfigInput(config, { inputText: "嗯 orally visual studio code" });
    assert.equal(result.raw_transcript, "嗯 orally visual studio code");
    assert.equal(result.final_text, "Orally Visual Studio Code。");
    assert.equal(result.warnings.length, 1);
    assert.match(result.warnings[0], /HTTP 503/);
    assert.match(result.warnings[0], /本地基础清理/);
    assert.ok(!result.warnings[0].includes("fake-model-key"));
  });
  config.postprocess.models[0].fallback_to_builtin = false;
  await fakeFetch([completion("")], async () => {
    await assert.rejects(testConfigInput(config, { inputText: "input" }), /空文本/);
  });
});

test("privacy blocking prevents every external request but allows raw, local and empty-chain text tests", async () => {
  const config = configuration();
  config.privacy.allow_external_requests = false;
  await fakeFetch([], async () => {
    await assert.rejects(testServiceConnection(config), /隐私/);
    await assert.rejects(testServiceConnection(config, config.postprocess.models[0].id), /隐私/);
    await assert.rejects(testConfigInput(config, { inputText: "input" }), /隐私/);
    await assert.rejects(testConfigInput(config, { audioBytes: wav() }), /隐私/);
    config.output.raw = true;
    assert.equal((await testConfigInput(config, { inputText: " raw text " })).final_text, " raw text ");
    config.output.raw = false;
    config.postprocess.mode = "builtin";
    assert.equal((await testConfigInput(config, { inputText: "嗯 orally" })).final_text, "Orally。");
    config.postprocess.mode = "llm";
    config.postprocess.models[0].enabled = false;
    assert.equal((await testConfigInput(config, { inputText: "unedited" })).final_text, "unedited");
  });
});

test("invalid inputs, missing browser credentials and malformed responses never report connection success", async () => {
  const config = configuration();
  await fakeFetch([], async () => {
    await assert.rejects(testConfigInput(config), /请提供/);
    await assert.rejects(testConfigInput(config, { inputText: "", audioBytes: wav() }), /同时/);
    await assert.rejects(testConfigInput(config, { inputText: " " }), /不能为空/);
    await assert.rejects(testConfigInput(config, { inputText: "a".repeat(100001) }), /100,000/);
    await assert.rejects(testConfigInput(config, { audioBytes: new Uint8Array(15 * 1024 * 1024 + 1) }), /15 MiB/);
    await assert.rejects(testConfigInput(config, { audioBytes: [0, 256] }), /格式/);
    await assert.rejects(testConfigInput(config, { audioBytes: new Uint8Array(44) }), /WAV/);
    config.asr.api_key = null;
    await assert.rejects(testServiceConnection(config), /浏览器预览无法读取环境变量/);
    config.postprocess.models[0].api_key = null;
    await assert.rejects(testConfigInput(config, { inputText: "input" }), /缺少 API Key/, "configuration errors must not fall back");
  });
  config.asr.api_key = "fake-asr-key";
  await fakeFetch([json({ wrong: "shape" })], async () => {
    await assert.rejects(testServiceConnection(config), /格式不正确/);
  });
  await fakeFetch([json({ text: "" })], async () => {
    await assert.rejects(testConfigInput(config, { audioBytes: wav() }), /空转录/);
  });
});

test("network errors explain browser CORS limits and cancellation aborts without fallback", async () => {
  const config = configuration();
  const model = config.postprocess.models[0];
  await fakeFetch([() => { throw new TypeError("Failed to fetch fake-model-key"); }], async () => {
    await assert.rejects(testServiceConnection(config, model.id), (error) => {
      assert.match(error.message, /CORS/);
      assert.match(error.message, /原生应用/);
      assert.ok(!error.message.includes("fake-model-key"));
      return true;
    });
  });
  const controller = new AbortController();
  await fakeFetch([(_url, options) => new Promise((_resolve, reject) => {
    options.signal.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), { once: true });
    controller.abort();
  })], async () => {
    await assert.rejects(testConfigInput(config, { inputText: "input", signal: controller.signal }), /已取消/);
  });
});

test("a legacy flat model can be tested and an explicit completion endpoint is kept", async () => {
  const config = configuration();
  config.postprocess.models = [];
  config.postprocess.base_url = "https://legacy.example/v1/chat/completions/";
  await fakeFetch([completion("OK"), completion("legacy result")], async (calls) => {
    await testServiceConnection(config, "");
    assert.equal((await testConfigInput(config, { inputText: "legacy input" })).final_text, "legacy result");
    assert.ok(calls.every((call) => call.url === "https://legacy.example/v1/chat/completions"));
    assert.equal(JSON.parse(calls[1].body).messages[1].content, "Locale: zh-CN\nlegacy input");
  });
});

test("service bases and complete endpoint URLs normalize to the target standard endpoint", async () => {
  for (const [baseUrl, suffix] of [
    [" https://service.example/v1/// ", ""],
    ["https://service.example/v1/audio/transcriptions/", ""],
    ["https://service.example/v1/chat/completions/", ""],
    ["https://service.example/v1/audio/transcriptions/?version=1#fragment", "?version=1"],
  ]) {
    const config = configuration();
    config.asr.base_url = baseUrl;
    config.postprocess.models[0].base_url = baseUrl;
    await fakeFetch([json({ text: "" }), completion("OK")], async (calls) => {
      await testServiceConnection(config);
      await testServiceConnection(config, config.postprocess.models[0].id);
      assert.equal(calls[0].url, `https://service.example/v1/audio/transcriptions${suffix}`);
      assert.equal(calls[1].url, `https://service.example/v1/chat/completions${suffix}`);
    });
  }
});

test("connection timeout aborts the request and reports failure rather than fallback", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const config = configuration();
  await fakeFetch([(_url, options) => new Promise((_resolve, reject) => {
    options.signal.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), { once: true });
  })], async () => {
    const pending = testServiceConnection(config, config.postprocess.models[0].id);
    const rejected = assert.rejects(pending, /超时（30 秒）/);
    context.mock.timers.tick(30_000);
    await rejected;
  });
});
