import { OverlayMeter } from "./overlay-meter.js";

// Own native subscriptions and stop acknowledgement independently of the React view.
export class OverlaySession {
  constructor({ invoke, listen, onChange = () => {}, formatError = (error) => String(error?.message ?? error), reducedMotion = false, schedule = setTimeout, unschedule = clearTimeout } = {}) {
    this.invoke = invoke;
    this.listen = listen;
    this.onChange = onChange;
    this.formatError = formatError;
    this.schedule = schedule;
    this.unschedule = unschedule;
    this.meter = new OverlayMeter({ reducedMotion });
    this.disposed = false;
    this.unlisteners = [];
    this.levelTimer = null;
    this.recordingVersion = 0;
    this.state = {
      title: invoke ? "语音输入" : "正在录音…",
      detail: invoke ? "" : "按快捷键或点击停止",
      recording: !invoke,
      stopping: false,
      heights: this.meter.reset(),
    };
  }

  publish(next) {
    if (this.disposed) return;
    this.state = { ...this.state, ...next };
    this.onChange(this.state);
  }

  clearLevel() {
    if (this.levelTimer !== null) this.unschedule(this.levelTimer);
    this.levelTimer = null;
    this.publish({ heights: this.meter.reset() });
  }

  receiveStatus(payload) {
    if (this.disposed) return;
    const recording = Boolean(payload?.can_stop);
    if (!recording || recording !== this.state.recording) this.recordingVersion += 1;
    if (!recording || recording !== this.state.recording) this.clearLevel();
    this.publish({
      recording,
      stopping: recording ? this.state.stopping : false,
      title: typeof payload?.title === "string" ? payload.title : "Orally",
      detail: typeof payload?.detail === "string" ? payload.detail : "",
    });
  }

  receiveLevel(payload) {
    if (this.disposed || !this.state.recording) return;
    if (this.levelTimer !== null) this.unschedule(this.levelTimer);
    this.publish({ heights: this.meter.push(payload) });
    this.levelTimer = this.schedule(() => this.clearLevel(), 240);
  }

  async start() {
    if (!this.listen || this.disposed) return;
    const subscribe = async (event, handler) => {
      try {
        const unlisten = await this.listen(event, ({ payload }) => handler(payload));
        if (this.disposed) { try { unlisten(); } catch {} }
        else this.unlisteners.push(unlisten);
      } catch {}
    };
    await Promise.all([
      subscribe("dictation-status", (payload) => this.receiveStatus(payload)),
      subscribe("dictation-level", (payload) => this.receiveLevel(payload)),
    ]);
    if (this.invoke && !this.disposed) {
      try { await this.invoke("replay_dictation_status"); } catch {}
    }
  }

  async stop() {
    if (!this.invoke || this.disposed || !this.state.recording || this.state.stopping) return false;
    const recordingVersion = this.recordingVersion;
    this.publish({ stopping: true });
    try {
      await this.invoke("stop_recording");
      // The command only queues StopIfRecording; the next native status confirms it.
      return true;
    } catch (error) {
      if (!this.disposed && this.state.recording && recordingVersion === this.recordingVersion) this.publish({ stopping: false, title: "停止录音失败", detail: this.formatError(error) });
      return false;
    }
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    if (this.levelTimer !== null) this.unschedule(this.levelTimer);
    this.levelTimer = null;
    this.meter.reset();
    for (const unlisten of this.unlisteners.splice(0)) { try { unlisten(); } catch {} }
  }
}
