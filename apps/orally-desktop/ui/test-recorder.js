const MAX_DURATION_MS = 120_000;
const MAX_WAV_BYTES = 15 * 1024 * 1024;
const WAV_HEADER_BYTES = 44;

function recordingError(message) {
  const error = new Error(message);
  error.code = "orally-recording";
  return error;
}

function startError(error) {
  if (error?.code === "orally-recording") return error;
  const messages = {
    NotAllowedError: "麦克风权限被拒绝，请在浏览器权限设置中允许使用麦克风。",
    SecurityError: "浏览器禁止使用麦克风，请检查权限并使用 HTTPS 或 localhost。",
    NotFoundError: "未找到可用麦克风，请连接设备后重试。",
    DevicesNotFoundError: "未找到可用麦克风，请连接设备后重试。",
    NotReadableError: "麦克风无法读取，可能被其他应用占用，请关闭占用后重试。",
    TrackStartError: "麦克风无法读取，可能被其他应用占用，请关闭占用后重试。",
    OverconstrainedError: "当前麦克风不支持所需录音设置，请更换设备后重试。",
    AbortError: "录音已取消",
  };
  return recordingError(messages[error?.name] ?? "浏览器录音启动失败，请检查麦克风和浏览器权限。");
}

function stopTracks(stream) {
  for (const track of stream?.getTracks?.() ?? []) {
    try { track.stop(); } catch { /* Release every other track even if one device fails. */ }
  }
}

function encodeWav(chunks, sampleCount, sampleRate) {
  const bytes = new Uint8Array(WAV_HEADER_BYTES + sampleCount * 2);
  const view = new DataView(bytes.buffer);
  function tag(offset, text) {
    for (let index = 0; index < text.length; index += 1) bytes[offset + index] = text.charCodeAt(index);
  }
  tag(0, "RIFF");
  view.setUint32(4, bytes.length - 8, true);
  tag(8, "WAVE");
  tag(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // Uncompressed PCM.
  view.setUint16(22, 1, true); // Mono after mixing input channels.
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  tag(36, "data");
  view.setUint32(40, sampleCount * 2, true);
  let offset = WAV_HEADER_BYTES;
  for (const chunk of chunks) {
    for (const value of chunk) {
      const sample = Math.max(-1, Math.min(1, Number.isFinite(value) ? value : 0));
      view.setInt16(offset, Math.round(sample * (sample < 0 ? 32768 : 32767)), true);
      offset += 2;
    }
  }
  return bytes;
}

/**
 * Capture an actual microphone as mono PCM16 WAV, keeping the device's sample
 * rate. At a limit the microphone is released and stop() retrieves the clip.
 */
export async function startBrowserRecording({ signal = null, maxDurationMs = MAX_DURATION_MS, onLimit = null } = {}) {
  if (!Number.isFinite(maxDurationMs) || maxDurationMs <= 0) throw recordingError("录音时长需要大于 0");
  if (signal?.aborted) throw recordingError("录音已取消");
  const mediaDevices = globalThis.navigator?.mediaDevices;
  const AudioContextClass = globalThis.AudioContext ?? globalThis.webkitAudioContext;
  if (!mediaDevices?.getUserMedia || !AudioContextClass) {
    throw recordingError("当前浏览器不支持麦克风录音，请使用支持录音的浏览器，并通过 HTTPS 或 localhost 打开页面。");
  }
  const duration = Math.min(maxDurationMs, MAX_DURATION_MS);
  let stream = null;
  let context = null;
  let source = null;
  let processor = null;
  let silentOutput = null;
  let state = "starting";
  let released = false;
  let cleanup = null;
  let timer = null;
  let resumeTimer = null;
  let chunks = [];
  let sampleCount = 0;
  let sampleRate = 0;
  let stopPromise = null;
  let abortStart;
  const aborted = new Promise((_resolve, reject) => { abortStart = reject; });
  // The same signal remains useful after the start promise has settled.
  aborted.catch(() => {});

  function release() {
    if (released) return cleanup ?? Promise.resolve();
    released = true;
    clearTimeout(timer);
    clearTimeout(resumeTimer);
    if (processor) processor.onaudioprocess = null;
    for (const node of [source, processor, silentOutput]) {
      try { node?.disconnect(); } catch { /* Disconnection must not prevent device release. */ }
    }
    source = null;
    processor = null;
    silentOutput = null;
    stopTracks(stream);
    stream = null;
    const currentContext = context;
    context = null;
    cleanup = Promise.resolve().then(async () => {
      if (currentContext && currentContext.state !== "closed") {
        try { await currentContext.close(); } catch { /* Nodes and microphone tracks are already released. */ }
      }
    });
    return cleanup;
  }

  function cancel() {
    state = "cancelled";
    chunks = [];
    sampleCount = 0;
    stopPromise = null;
    signal?.removeEventListener("abort", abort);
    return release();
  }

  function abort() {
    abortStart(recordingError("录音已取消"));
    void cancel();
  }

  function stop() {
    if (signal?.aborted) void cancel();
    if (state === "cancelled") return Promise.reject(recordingError("录音已取消"));
    if (stopPromise) return stopPromise;
    state = "stopped";
    stopPromise = (async () => {
      try {
        await release();
        if (state === "cancelled") throw recordingError("录音已取消");
        if (!sampleCount) throw recordingError("未采集到录音音频，请确认麦克风可用并录制一段语音后重试。");
        const audioBytes = encodeWav(chunks, sampleCount, sampleRate);
        chunks = [];
        state = "completed";
        return { audioBytes, audioName: "microphone-test.wav" };
      } finally {
        signal?.removeEventListener("abort", abort);
      }
    })();
    return stopPromise;
  }

  function limit(reason) {
    if (state !== "recording") return;
    state = "limited";
    void release();
    queueMicrotask(() => {
      if (state !== "limited" || !onLimit) return;
      try { Promise.resolve(onLimit({ reason })).catch(() => {}); } catch { /* The UI owns callback errors. */ }
    });
  }

  function checkCancelled() {
    if (state === "cancelled") throw recordingError("录音已取消");
  }

  signal?.addEventListener("abort", abort, { once: true });
  try {
    // getUserMedia cannot be aborted. A permission response arriving after
    // cancellation must release its new stream without creating audio nodes.
    const permission = Promise.resolve(mediaDevices.getUserMedia({ audio: { channelCount: { ideal: 1 } }, video: false }))
      .then((captured) => {
        if (state === "cancelled") {
          stopTracks(captured);
          throw recordingError("录音已取消");
        }
        stream = captured;
        return captured;
      });
    await Promise.race([permission, aborted]);
    checkCancelled();
    const tracks = stream.getAudioTracks?.() ?? [];
    if (!tracks.length || tracks.every((track) => track.readyState === "ended")) {
      throw recordingError("未找到可用麦克风，请连接设备后重试。");
    }
    context = new AudioContextClass();
    if (!context.createScriptProcessor || !context.createMediaStreamSource || !context.createGain) {
      throw recordingError("当前浏览器无法生成 PCM WAV 录音，请使用支持 Web Audio 的浏览器。");
    }
    sampleRate = context.sampleRate;
    if (!Number.isInteger(sampleRate) || sampleRate <= 0 || sampleRate > 384_000) {
      throw recordingError("麦克风的采样率不受支持，请更换设备后重试。");
    }
    source = context.createMediaStreamSource(stream);
    let channels = source.channelCount || 1;
    try { channels = tracks[0].getSettings?.().channelCount || channels; } catch { /* Some browsers do not expose track settings. */ }
    processor = context.createScriptProcessor(4096, Math.max(1, Math.min(32, channels)), 1);
    silentOutput = context.createGain();
    silentOutput.gain.value = 0;
    const durationSamples = Math.max(1, Math.floor(sampleRate * duration / 1000));
    const sizeSamples = Math.floor((MAX_WAV_BYTES - WAV_HEADER_BYTES) / 2);
    const sampleLimit = Math.min(durationSamples, sizeSamples);
    const sampleLimitReason = durationSamples <= sizeSamples ? "duration" : "size";
    processor.onaudioprocess = (event) => {
      if (state !== "recording") return;
      const input = event.inputBuffer;
      if (!input.numberOfChannels || !input.length) return;
      const count = Math.min(input.length, sampleLimit - sampleCount);
      if (count > 0) {
        const mixed = new Float32Array(count);
        for (let channel = 0; channel < input.numberOfChannels; channel += 1) {
          const samples = input.getChannelData(channel);
          for (let index = 0; index < count; index += 1) mixed[index] += samples[index] / input.numberOfChannels;
        }
        chunks.push(mixed);
        sampleCount += count;
      }
      if (sampleCount >= sampleLimit) limit(sampleLimitReason);
    };
    if (context.state === "suspended") {
      const resumeTimeout = new Promise((_resolve, reject) => {
        resumeTimer = setTimeout(() => reject(recordingError("浏览器未能启动音频采集，请点击录音按钮后重试。")), 10_000);
      });
      await Promise.race([context.resume(), aborted, resumeTimeout]);
      clearTimeout(resumeTimer);
    }
    checkCancelled();
    if (context.state !== "running") throw recordingError("浏览器未能启动音频采集，请点击录音按钮后重试。");
    state = "recording";
    source.connect(processor);
    processor.connect(silentOutput);
    silentOutput.connect(context.destination);
    timer = setTimeout(() => limit("duration"), duration);
    return { stop, cancel };
  } catch (error) {
    await cancel();
    throw startError(error);
  }
}
