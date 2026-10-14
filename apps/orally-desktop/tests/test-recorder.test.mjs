import test from "node:test";
import assert from "node:assert/strict";
import { startBrowserRecording } from "../ui/test-recorder.js";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

async function flush() {
  for (let turn = 0; turn < 6; turn += 1) await Promise.resolve();
}

async function fakeMicrophone(options, operation) {
  const globals = new Map(["navigator", "AudioContext", "webkitAudioContext", "OfflineAudioContext", "webkitOfflineAudioContext"].map((name) => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  const fixture = { requests: [], contexts: [], offlineContexts: [], decodeInputs: [], nodes: [], processor: null };
  fixture.tracks = [0, 1].map(() => ({
    readyState: "live", stops: 0,
    getSettings() { return { channelCount: 2 }; },
    stop() { this.stops += 1; this.readyState = "ended"; },
  }));
  fixture.stream = {
    getTracks: () => fixture.tracks,
    getAudioTracks: () => options.noTracks ? [] : fixture.tracks,
  };
  class Node {
    constructor(kind) { this.kind = kind; this.disconnects = 0; this.connections = []; fixture.nodes.push(this); }
    connect(destination) { this.connections.push(destination); }
    disconnect() { this.disconnects += 1; }
  }
  class AudioContext {
    constructor() {
      this.sampleRate = options.sampleRate ?? 48000;
      this.state = options.state ?? "running";
      this.destination = {};
      this.closeCalls = 0;
      this.resumeCalls = 0;
      fixture.contexts.push(this);
    }
    createMediaStreamSource(stream) {
      assert.equal(stream, fixture.stream);
      if (options.sourceError) throw options.sourceError;
      const node = new Node("source");
      node.channelCount = 2;
      return node;
    }
    createScriptProcessor(size, inputChannels, outputChannels) {
      fixture.processorArguments = { size, inputChannels, outputChannels };
      const node = new Node("processor");
      fixture.processor = node;
      return node;
    }
    createGain() { const node = new Node("gain"); node.gain = { value: 1 }; return node; }
    async resume() {
      this.resumeCalls += 1;
      await options.resumePromise;
      if (this.state !== "closed") this.state = "running";
    }
    async close() {
      this.closeCalls += 1;
      await options.closePromise;
      this.state = "closed";
    }
  }
  class OfflineAudioContext {
    constructor(channels, length, sampleRate) {
      if (options.conversionError) throw options.conversionError;
      Object.assign(this, { channels, length, sampleRate, destination: {} });
      fixture.offlineContexts.push(this);
    }
    async decodeAudioData(bytes) {
      fixture.renderInput = decodedWav(new Uint8Array(bytes));
      fixture.decodeInputs.push({ sampleRate: fixture.renderInput.sampleRate, frames: fixture.renderInput.samples.length });
      fixture.renderStartedAfterCaptureClosed = fixture.contexts.every((context) => context.state === "closed")
        && fixture.tracks.every((track) => track.readyState === "ended");
      await options.renderPromise;
      if (options.renderError) throw options.renderError;
      // Stub the browser's native DSP. These tests verify the API contract,
      // source rate and encoded output; they do not reimplement resampling.
      const length = Math.max(1, Math.floor(fixture.renderInput.samples.length * this.sampleRate / fixture.renderInput.sampleRate));
      const samples = new Float32Array(length).fill(options.renderValue ?? 0.5);
      options.onDecoded?.(this.sampleRate);
      return { sampleRate: this.sampleRate, numberOfChannels: 1, length, getChannelData: () => samples };
    }
  }
  fixture.emit = (...channels) => {
    const samples = channels.map((value) => value instanceof Float32Array ? value : Float32Array.from(value));
    fixture.processor?.onaudioprocess?.({ inputBuffer: {
      length: samples[0].length, numberOfChannels: samples.length,
      getChannelData: (channel) => samples[channel],
    } });
  };
  Object.defineProperty(globalThis, "navigator", { configurable: true, value: { mediaDevices: {
    getUserMedia(constraints) {
      fixture.requests.push(constraints);
      if (options.permissionError) return Promise.reject(options.permissionError);
      return options.permissionPromise ?? Promise.resolve(fixture.stream);
    },
  } } });
  Object.defineProperty(globalThis, "AudioContext", { configurable: true, value: options.unsupported ? undefined : AudioContext });
  Object.defineProperty(globalThis, "webkitAudioContext", { configurable: true, value: undefined });
  Object.defineProperty(globalThis, "OfflineAudioContext", { configurable: true, value: options.noOfflineContext || options.webkitOffline ? undefined : OfflineAudioContext });
  Object.defineProperty(globalThis, "webkitOfflineAudioContext", { configurable: true, value: options.webkitOffline ? OfflineAudioContext : undefined });
  try { await operation(fixture); } finally {
    for (const [name, descriptor] of globals) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
  }
}

function decodedWav(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const tag = (offset, length) => String.fromCharCode(...bytes.subarray(offset, offset + length));
  assert.equal(tag(0, 4), "RIFF");
  assert.equal(tag(8, 4), "WAVE");
  assert.equal(tag(12, 4), "fmt ");
  assert.equal(tag(36, 4), "data");
  assert.equal(view.getUint32(4, true), bytes.byteLength - 8);
  assert.equal(view.getUint32(16, true), 16);
  assert.equal(view.getUint16(20, true), 1);
  assert.equal(view.getUint16(22, true), 1);
  assert.equal(view.getUint16(32, true), 2);
  assert.equal(view.getUint16(34, true), 16);
  assert.equal(view.getUint32(40, true), bytes.byteLength - 44);
  const sampleRate = view.getUint32(24, true);
  assert.equal(view.getUint32(28, true), sampleRate * 2);
  return { sampleRate, view, samples: Array.from({ length: (bytes.length - 44) / 2 }, (_item, index) => view.getInt16(44 + index * 2, true)) };
}

test("16 kHz captured samples mix to mono PCM16 WAV without resampling, with clipping and reusable stop", async () => {
  await fakeMicrophone({ sampleRate: 16000, noOfflineContext: true }, async (fixture) => {
    const session = await startBrowserRecording();
    assert.deepEqual(fixture.requests, [{ audio: { channelCount: { ideal: 1 } }, video: false }]);
    assert.deepEqual(fixture.processorArguments, { size: 4096, inputChannels: 2, outputChannels: 1 });
    assert.equal(fixture.nodes.find((node) => node.kind === "gain").gain.value, 0, "the graph cannot play microphone feedback");
    const left = Float32Array.from([-1, 1, 0.5, -0.5, 2, NaN]);
    fixture.emit(left, [-1, 1, -0.5, -0.5, 2, 0]);
    left.fill(0);
    fixture.emit([0.5, -0.25]);
    const first = session.stop();
    const second = session.stop();
    assert.equal(first, second);
    const result = await first;
    assert.equal(await second, result);
    assert.equal(result.audioName, "microphone-test.wav");
    assert.ok(result.audioBytes instanceof Uint8Array);
    const decoded = decodedWav(result.audioBytes);
    assert.equal(decoded.sampleRate, 16000);
    assert.equal(fixture.offlineContexts.length, 0);
    assert.deepEqual(decoded.samples, [-32768, 32767, 0, -16384, 32767, 0, 16384, -8192]);
    assert.equal(result.audioBytes.length, 60);
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
    assert.equal(fixture.contexts[0].closeCalls, 1);
    assert.ok(fixture.nodes.every((node) => node.disconnects === 1));
    assert.equal(fixture.processor.onaudioprocess, null);
    fixture.emit([1, 1, 1]);
    assert.equal((await session.stop()).audioBytes.length, 60);
  });
});

test("sample rates below 16 kHz retain the actual rate and samples without an offline context", async () => {
  await fakeMicrophone({ sampleRate: 8000, noOfflineContext: true }, async (fixture) => {
    const session = await startBrowserRecording();
    fixture.emit([0.25, -0.5, 0]);
    const decoded = decodedWav((await session.stop()).audioBytes);
    assert.equal(decoded.sampleRate, 8000);
    assert.deepEqual(decoded.samples, [8192, -16384, 0]);
    assert.equal(fixture.offlineContexts.length, 0);
  });
});

test("higher rates decode original-rate mono WAV at 16 kHz after capture is released", async () => {
  for (const sampleRate of [48000, 44100, 22050]) {
    await fakeMicrophone({ sampleRate, renderValue: 0.25, webkitOffline: sampleRate === 22050 }, async (fixture) => {
      const session = await startBrowserRecording();
      fixture.emit(new Float32Array(500).fill(0.75), new Float32Array(500).fill(0.25));
      fixture.emit(new Float32Array(501).fill(0.75), new Float32Array(501).fill(0.25));
      const pending = session.stop();
      assert.equal(session.stop(), pending);
      const result = await pending;
      const decoded = decodedWav(result.audioBytes);
      const expectedFrames = Math.floor(1001 * 16000 / sampleRate);
      const renderer = fixture.offlineContexts[0];
      assert.equal(fixture.offlineContexts.length, 1);
      assert.equal(renderer.channels, 1);
      assert.equal(renderer.sampleRate, 16000);
      assert.equal(renderer.length, 1, "only the context rate is needed for native decoding");
      assert.equal(fixture.renderInput.sampleRate, sampleRate);
      assert.equal(fixture.renderInput.samples.length, 1001);
      assert.ok(fixture.renderInput.samples.every((sample) => sample === 16384));
      assert.equal(fixture.renderStartedAfterCaptureClosed, true);
      assert.equal(decoded.sampleRate, 16000);
      assert.equal(decoded.samples.length, expectedFrames);
      assert.ok(decoded.samples.every((sample) => sample === 8192), "encode the native decoder's output");
      assert.ok(Math.abs(decoded.samples.length / 16000 - 1001 / sampleRate) < 1 / 16000);
      assert.ok(fixture.nodes.every((node) => node.disconnects === 1));
      assert.equal(await session.stop(), result);
    });
  }
});

test("conversion failures reject rather than returning original-rate audio after releasing capture", async () => {
  for (const options of [
    { noOfflineContext: true },
    { conversionError: new Error("offline initialization failed") },
    { renderError: new Error("offline rendering failed") },
  ]) {
    await fakeMicrophone(options, async (fixture) => {
      const session = await startBrowserRecording();
      fixture.emit([0.5, 0.5, 0.5]);
      const pending = session.stop();
      await assert.rejects(pending, /音频转换/);
      assert.equal(session.stop(), pending);
      assert.ok(fixture.tracks.every((track) => track.stops === 1));
      assert.equal(fixture.contexts[0].closeCalls, 1);
      assert.ok(fixture.nodes.every((node) => node.disconnects === 1));
    });
  }
});

test("large conversion ratios use multiple native stages and correct accumulated frame rounding", async () => {
  await fakeMicrophone({ sampleRate: 96001 }, async (fixture) => {
    const session = await startBrowserRecording();
    fixture.emit(new Float32Array(1003).fill(0.5));
    const decoded = decodedWav((await session.stop()).audioBytes);
    assert.deepEqual(fixture.offlineContexts.map((context) => context.sampleRate), [48001, 24001, 16000]);
    assert.equal(decoded.sampleRate, 16000);
    assert.equal(decoded.samples.length, Math.floor(1003 * 16000 / 96001));
    assert.equal(decoded.samples.length, 167, "successive native rounding alone would lose an extra frame");
    assert.ok(decoded.samples.slice(0, -1).every((sample) => sample === 16384));
    assert.equal(decoded.samples.at(-1), 0, "only pad the fractional tail lost between stages");
    assert.ok(Math.abs(decoded.samples.length / 16000 - 1003 / 96001) < 1 / 16000);
  });
});

test("cancellation after a native stage prevents starting the next conversion", async () => {
  const controller = new AbortController();
  await fakeMicrophone({ sampleRate: 96000, onDecoded: () => controller.abort() }, async (fixture) => {
    const session = await startBrowserRecording({ signal: controller.signal });
    fixture.emit(new Float32Array(960).fill(0.5));
    await assert.rejects(session.stop(), /录音已取消/);
    assert.deepEqual(fixture.offlineContexts.map((context) => context.sampleRate), [48000]);
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
    assert.equal(fixture.contexts[0].closeCalls, 1);
  });
});

test("manual cancellation and abort settle a pending conversion and discard late completion or failure", async () => {
  for (const useSignal of [false, true]) {
    for (const lateFailure of [false, true]) {
      const rendering = deferred();
      await fakeMicrophone({ renderPromise: rendering.promise }, async (fixture) => {
        const controller = new AbortController();
        const session = await startBrowserRecording({ signal: controller.signal });
        fixture.emit([0.5, 0.5, 0.5]);
        const pending = session.stop();
        const rejected = assert.rejects(pending, /录音已取消/);
        await flush();
        assert.equal(fixture.offlineContexts.length, 1);
        assert.equal(fixture.renderStartedAfterCaptureClosed, true);
        if (useSignal) controller.abort();
        else await session.cancel();
        await rejected;
        await assert.rejects(session.stop(), /录音已取消/);
        if (lateFailure) rendering.reject(new Error("late render failure"));
        else rendering.resolve();
        await flush();
        await assert.rejects(session.stop(), /录音已取消/);
        assert.ok(fixture.tracks.every((track) => track.stops === 1));
        assert.ok(fixture.nodes.every((node) => node.disconnects === 1));
      });
    }
  }
});

test("cancel releases every device and context once, discards captured samples and is idempotent", async () => {
  await fakeMicrophone({}, async (fixture) => {
    const session = await startBrowserRecording();
    fixture.emit([0.1, 0.2]);
    await Promise.all([session.cancel(), session.cancel()]);
    await assert.rejects(session.stop(), /录音已取消/);
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
    assert.equal(fixture.contexts[0].closeCalls, 1);
    assert.equal(fixture.contexts[0].state, "closed");
    assert.equal(fixture.processor.onaudioprocess, null);
  });
});

test("abort while permission is pending rejects promptly and releases a late permission stream", async () => {
  const permission = deferred();
  await fakeMicrophone({ permissionPromise: permission.promise }, async (fixture) => {
    const controller = new AbortController();
    const started = startBrowserRecording({ signal: controller.signal });
    controller.abort();
    await assert.rejects(started, /录音已取消/);
    assert.equal(fixture.contexts.length, 0);
    permission.resolve(fixture.stream);
    await flush();
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
    assert.equal(fixture.contexts.length, 0);
  });
});

test("aborting a live session or a pending stop cannot return audio or reopen devices", async () => {
  const closing = deferred();
  await fakeMicrophone({ closePromise: closing.promise }, async (fixture) => {
    const controller = new AbortController();
    const session = await startBrowserRecording({ signal: controller.signal });
    fixture.emit([0.5, -0.5]);
    const stopped = session.stop();
    const rejected = assert.rejects(stopped, /录音已取消/);
    controller.abort();
    closing.resolve();
    await rejected;
    await session.cancel();
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
    assert.equal(fixture.contexts[0].closeCalls, 1);
  });
});

test("abort during a suspended context start releases resources even when resume resolves late", async () => {
  const resuming = deferred();
  await fakeMicrophone({ state: "suspended", resumePromise: resuming.promise }, async (fixture) => {
    const controller = new AbortController();
    const started = startBrowserRecording({ signal: controller.signal });
    await flush();
    assert.equal(fixture.contexts[0].resumeCalls, 1);
    controller.abort();
    await assert.rejects(started, /录音已取消/);
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
    assert.equal(fixture.contexts[0].closeCalls, 1);
    resuming.resolve();
    await flush();
    assert.equal(fixture.contexts[0].state, "closed");
    assert.equal(fixture.processor.onaudioprocess, null);
  });
});

test("duration sample limits truncate the last buffer, release the microphone and notify once", async () => {
  await fakeMicrophone({ sampleRate: 16000 }, async (fixture) => {
    const limits = [];
    const session = await startBrowserRecording({ maxDurationMs: 10, onLimit: (limit) => { limits.push(limit); } });
    fixture.emit(new Float32Array(300).fill(0.25));
    fixture.emit(new Float32Array(300).fill(1));
    await flush();
    assert.deepEqual(limits, [{ reason: "duration" }]);
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
    const result = await session.stop();
    const decoded = decodedWav(result.audioBytes);
    assert.equal(decoded.sampleRate, 16000);
    assert.equal(decoded.samples.length, 160);
    assert.ok(decoded.samples.every((sample) => sample === 8192));
    assert.equal(fixture.contexts[0].closeCalls, 1);
  });
});

test("the wall clock also stops recording when no more audio callbacks arrive", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  await fakeMicrophone({ sampleRate: 16000 }, async (fixture) => {
    const limits = [];
    const session = await startBrowserRecording({ maxDurationMs: 20, onLimit: (limit) => { limits.push(limit); } });
    fixture.emit([0.5]);
    context.mock.timers.tick(20);
    await flush();
    assert.deepEqual(limits, [{ reason: "duration" }]);
    assert.equal((await session.stop()).audioBytes.length, 46);
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
  });
});

test("the WAV size cap bounds captured memory and stops high sample rate recordings before the time limit", async () => {
  await fakeMicrophone({ sampleRate: 384000 }, async (fixture) => {
    const limits = [];
    const session = await startBrowserRecording({ onLimit: (limit) => { limits.push(limit); } });
    const maximumSamples = (15 * 1024 * 1024 - 44) / 2;
    fixture.emit(new Float32Array(maximumSamples + 16).fill(0.5));
    await flush();
    assert.deepEqual(limits, [{ reason: "size" }]);
    const result = await session.stop();
    assert.equal(fixture.decodeInputs[0].frames, maximumSamples, "conversion must not relax the original capture memory limit");
    const outputSamples = Math.floor(maximumSamples * 16000 / 384000);
    assert.equal(result.audioBytes.byteLength, 44 + outputSamples * 2);
    const view = new DataView(result.audioBytes.buffer);
    assert.equal(view.getUint32(24, true), 16000);
    assert.equal(view.getUint32(40, true), outputSamples * 2);
    assert.equal(view.getInt16(result.audioBytes.length - 2, true), 16384);
    assert.equal(fixture.contexts[0].closeCalls, 1);
  });
});

test("permissions and missing devices return actionable Chinese errors without fabricated audio", async () => {
  for (const [name, message] of [["NotAllowedError", /权限被拒绝/], ["NotFoundError", /未找到/], ["NotReadableError", /无法读取/]]) {
    await fakeMicrophone({ permissionError: new DOMException("device failure", name) }, async (fixture) => {
      await assert.rejects(startBrowserRecording(), message);
      assert.equal(fixture.contexts.length, 0);
    });
  }
  await fakeMicrophone({ noTracks: true }, async (fixture) => {
    await assert.rejects(startBrowserRecording(), /未找到/);
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
  });
  await fakeMicrophone({}, async (fixture) => {
    const session = await startBrowserRecording();
    await assert.rejects(session.stop(), /未采集到/);
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
  });
});

test("initialization failures close acquired devices and unsupported browsers never request permission", async () => {
  await fakeMicrophone({ sourceError: new Error("audio graph failed") }, async (fixture) => {
    await assert.rejects(startBrowserRecording(), /录音启动失败/);
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
    assert.equal(fixture.contexts[0].closeCalls, 1);
  });
  await fakeMicrophone({ unsupported: true }, async (fixture) => {
    await assert.rejects(startBrowserRecording(), /不支持麦克风/);
    assert.equal(fixture.requests.length, 0);
  });
  await fakeMicrophone({}, async (fixture) => {
    const controller = new AbortController();
    controller.abort();
    await assert.rejects(startBrowserRecording({ signal: controller.signal }), /已取消/);
    await assert.rejects(startBrowserRecording({ maxDurationMs: 0 }), /大于 0/);
    assert.equal(fixture.requests.length, 0);
  });
});

test("a context that cannot resume times out and releases its microphone", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  await fakeMicrophone({ state: "suspended", resumePromise: new Promise(() => {}) }, async (fixture) => {
    const started = startBrowserRecording();
    const rejected = assert.rejects(started, /未能启动音频采集/);
    await flush();
    context.mock.timers.tick(10000);
    await rejected;
    assert.ok(fixture.tracks.every((track) => track.stops === 1));
    assert.equal(fixture.contexts[0].closeCalls, 1);
  });
});
