import { TestSession } from "../test-session.js";
import { startBrowserRecording } from "../test-recorder.js";
import { testConfigInput } from "../service-tests.js";

export const MAX_TEST_RECORDING_MS = 120_000;

const LIMIT_MESSAGES = {
  size: "录音已达到大小上限，已自动停止。",
  duration: "录音已达到时长上限，已自动停止。",
  "recording-error": "录音设备出现异常，正在结束测试。",
  expired: "录音测试已过期，正在结束测试。",
};

function limitReason(reason) {
  return reason === "size-limit" ? "size" : reason === "duration-limit" ? "duration" : reason;
}

// The controller owns the dialog's asynchronous resources; React only subscribes
// to immutable views. TestSession keeps completed work tied to its captured draft.
export class ConfigTestController {
  constructor({ invoke, listen, record = startBrowserRecording, testInput = testConfigInput } = {}) {
    this.invoke = invoke;
    this.listen = listen;
    this.record = record;
    this.testInput = testInput;
    this.listeners = new Set();
    this.notice = "";
    this.limit = null;
    this.validationError = null;
    this.validationSnapshot = null;
    this.closeRequested = false;
    this.session = new TestSession(() => this.changed());
    this.state = this.view();
  }

  subscribe = (listener) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getState = () => this.state;

  view() {
    const session = this.session;
    return {
      phase: this.validationError ? "error" : session.phase,
      busy: session.busy,
      result: session.result,
      error: this.validationError ?? session.error,
      snapshot: this.validationSnapshot ?? session.snapshot,
      startedAt: session.startedAt,
      recordedMs: session.recordedMs,
      notice: this.notice,
      closeRequested: this.closeRequested,
    };
  }

  changed() {
    this.state = this.view();
    for (const listener of this.listeners) listener();
    if (this.limit && this.session.phase === "recording") {
      // A native limit can arrive before start_test_recording returns its ID.
      // Wait until TestSession has installed the recording before stopping it.
      const token = this.session.token;
      queueMicrotask(() => {
        if (token === this.session.token && this.session.phase === "recording") void this.stop();
      });
    }
  }

  reset() {
    this.notice = "";
    this.limit = null;
    this.validationError = null;
    this.validationSnapshot = null;
    this.closeRequested = false;
    this.session.reset();
    return this.session.cleanup;
  }

  fail(error, snapshot = null) {
    this.validationError = error;
    this.validationSnapshot = snapshot;
    this.changed();
  }

  handleLimit(reason) {
    if (reason === "cancelled") {
      void this.reset();
      this.closeRequested = true;
      this.changed();
      return;
    }
    this.limit = reason;
    this.notice = LIMIT_MESSAGES[reason] ?? "录音已停止，正在处理测试。";
    this.changed();
  }

  elapsedMs(now = Date.now()) {
    return this.session.phase === "recording"
      ? Math.max(0, now - this.session.startedAt)
      : this.session.recordedMs;
  }

  tick(now = Date.now()) {
    const elapsed = this.elapsedMs(now);
    if (this.session.phase === "recording" && elapsed >= MAX_TEST_RECORDING_MS) this.handleLimit("duration");
    return elapsed;
  }

  stop() {
    return this.session.stopRecording();
  }

  async run(snapshot, mode, inputText = "") {
    if (this.session.busy) return;
    void this.reset();
    if (mode === "text") {
      if (!inputText.trim()) {
        this.fail(new Error("请输入要测试的文本。"), snapshot);
        return;
      }
      if (inputText.length > 100_000) {
        this.fail(new Error("测试文本不能超过 100,000 个字符。"), snapshot);
        return;
      }
      await this.session.runText(snapshot, inputText, (config, text, { signal }) => this.invoke
        ? this.invoke("test_config_input", { config, inputText: text, audioBytes: null, audioName: null })
        : this.testInput(config, { inputText: text, signal }));
      return;
    }
    if (!snapshot.privacy?.allow_external_requests) {
      this.fail(new Error("当前隐私设置已禁止外部请求，请先在应用设置中允许外部 AI 请求。"), snapshot);
      return;
    }
    const asrOnly = mode === "asr";
    await this.session.startRecording(snapshot, async (config, { signal }) => {
      if (this.invoke) return this.startNativeRecording(config, asrOnly, signal);
      const recording = await this.record({
        signal,
        maxDurationMs: MAX_TEST_RECORDING_MS,
        onLimit: ({ reason }) => { if (!signal.aborted) this.handleLimit(reason); },
      });
      return {
        cancel: () => recording.cancel(),
        stop: async () => {
          const audio = await recording.stop();
          return this.testInput(config, { ...audio, asrOnly, signal });
        },
      };
    });
  }

  async startNativeRecording(snapshot, asrOnly, signal) {
    let sessionId = null;
    let unlisten = null;
    let listening = true;
    let cancellation = null;
    const pendingLimits = [];
    const releaseListener = async () => {
      listening = false;
      const stop = unlisten;
      unlisten = null;
      signal.removeEventListener("abort", releaseListener);
      if (stop) {
        try { await stop(); } catch { /* Microphone release must still run. */ }
      }
    };
    const cancel = () => {
      if (!cancellation) cancellation = (async () => {
        await releaseListener();
        if (sessionId !== null) await this.invoke("cancel_test_recording", { sessionId });
      })();
      return cancellation;
    };
    try {
      signal.addEventListener("abort", releaseListener, { once: true });
      if (this.listen) {
        try {
          unlisten = await this.listen("test-recording-captured", (event) => {
            const payload = event.payload;
            if (!listening || signal.aborted || !payload?.session_id) return;
            if (sessionId === null) pendingLimits.push(payload);
            else if (payload.session_id === sessionId) this.handleLimit(limitReason(payload.reason));
          });
        } catch {
          if (!signal.aborted) this.notice = "录音状态通知不可用，可手动停止录音。";
        }
      } else {
        this.notice = "录音状态通知不可用，可手动停止录音。";
      }
      if (signal.aborted) throw new Error("录音测试已取消");
      sessionId = await this.invoke("start_test_recording", { config: snapshot, asrOnly });
      if (signal.aborted) {
        await cancel();
        throw new Error("录音测试已取消");
      }
      const limit = pendingLimits.find((event) => event.session_id === sessionId);
      if (limit) this.handleLimit(limitReason(limit.reason));
      return {
        stop: async () => {
          await releaseListener();
          return this.invoke("stop_test_recording", { sessionId });
        },
        cancel,
      };
    } catch (error) {
      await releaseListener();
      throw error;
    }
  }
}
