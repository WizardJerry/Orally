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
  const globals = new Map(["navigator", "AudioContext", "webkitAudioContext"].map((name) => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  const fixture = { requests: [], contexts: [], nodes: [], processor: null };
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

test("actual captured samples mix to mono PCM16 WAV with valid lengths, clipping and reusable stop", async () => {
  await fakeMicrophone({}, async (fixture) => {
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
    assert.equal(decoded.sampleRate, 48000);
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
    assert.equal(result.audioBytes.byteLength, 15 * 1024 * 1024);
    const view = new DataView(result.audioBytes.buffer);
    assert.equal(view.getUint32(40, true), maximumSamples * 2);
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
