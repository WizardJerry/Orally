// A test owns its captured draft and recording until it completes or is cancelled.
// Resource cleanup is serialized so a dismissed microphone request cannot leak
// into a later test, even when permission resolves after the dialog has closed.
export class TestSession {
  constructor(onChange = () => {}) {
    this.onChange = onChange;
    this.token = 0;
    this.phase = "idle";
    this.snapshot = null;
    this.result = null;
    this.error = null;
    this.startedAt = null;
    this.recordedMs = 0;
    this.recording = null;
    this.controller = null;
    this.pendingStart = null;
    this.cleanup = Promise.resolve();
  }

  get busy() {
    return ["starting", "recording", "processing"].includes(this.phase);
  }

  changed() {
    this.onChange(this);
  }

  reset() {
    this.token += 1;
    this.controller?.abort();
    const recording = this.recording;
    const pendingStart = this.pendingStart;
    this.recording = null;
    this.controller = null;
    this.cleanup = Promise.all([this.cleanup, pendingStart]).then(() => recording?.cancel()).catch(() => {});
    this.phase = "idle";
    this.snapshot = null;
    this.result = null;
    this.error = null;
    this.startedAt = null;
    this.recordedMs = 0;
    this.changed();
  }

  startRecording(snapshot, start) {
    this.reset();
    const token = this.token;
    const config = structuredClone(snapshot);
    const controller = new AbortController();
    const gate = this.cleanup;
    this.snapshot = config;
    this.controller = controller;
    this.phase = "starting";
    this.changed();
    const operation = (async () => {
      try {
        await gate;
        if (token !== this.token) return;
        const recording = await start(config, { signal: controller.signal });
        if (token !== this.token) { await recording.cancel(); return; }
        this.recording = recording;
        this.startedAt = Date.now();
        this.phase = "recording";
        this.changed();
      } catch (error) {
        if (token !== this.token) return;
        this.phase = "error";
        this.error = error;
        this.controller = null;
        this.changed();
      }
    })();
    this.pendingStart = operation;
    void operation.finally(() => { if (this.pendingStart === operation) this.pendingStart = null; });
    return operation;
  }

  async stopRecording() {
    if (this.phase !== "recording") return;
    const token = this.token;
    const recording = this.recording;
    this.recordedMs = Date.now() - this.startedAt;
    this.phase = "processing";
    this.changed();
    try {
      const result = await recording.stop();
      if (token !== this.token) return;
      this.result = result;
      this.phase = "complete";
    } catch (error) {
      await recording.cancel().catch(() => {});
      if (token !== this.token) return;
      this.error = error;
      this.phase = "error";
    } finally {
      if (token === this.token) {
        this.recording = null;
        this.controller = null;
        this.changed();
      }
    }
  }

  async runText(snapshot, inputText, run) {
    this.reset();
    const token = this.token;
    const config = structuredClone(snapshot);
    const controller = new AbortController();
    const gate = this.cleanup;
    this.snapshot = config;
    this.controller = controller;
    this.phase = "processing";
    this.changed();
    try {
      await gate;
      if (token !== this.token) return;
      const result = await run(config, inputText, { signal: controller.signal });
      if (token !== this.token) return;
      this.result = result;
      this.phase = "complete";
    } catch (error) {
      if (token !== this.token) return;
      this.error = error;
      this.phase = "error";
    } finally {
      if (token === this.token) { this.controller = null; this.changed(); }
    }
  }
}
