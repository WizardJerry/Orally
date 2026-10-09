import { OverlayMeter, OVERLAY_BAR_COUNT } from "./overlay-meter.js";

const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;
const overlay = document.querySelector(".dictation-pill");
const stop = document.querySelector("#stop");
const title = document.querySelector("#title");
const detail = document.querySelector("#detail");
const volumeMeter = document.querySelector("#meter");
const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
const meter = new OverlayMeter({ reducedMotion: motion.matches });
const bars = Array.from({ length: OVERLAY_BAR_COUNT }, () => {
  const bar = document.createElement("span");
  bar.className = "meter-bar";
  volumeMeter.append(bar);
  return bar;
});
const unlisteners = [];
let recording = !invoke;
let stopping = false;
let staleLevelTimer = null;
let disposed = false;

function renderMeter(heights) {
  volumeMeter.classList.toggle("is-silent", heights.every((height) => height === 2));
  bars.forEach((bar, index) => { bar.style.height = `${heights[index]}px`; });
}

function clearMeter() {
  clearTimeout(staleLevelTimer);
  staleLevelTimer = null;
  renderMeter(meter.reset());
}

function renderStop() {
  stop.hidden = !recording;
  stop.disabled = !invoke || !recording || stopping;
  stop.setAttribute("aria-busy", String(stopping));
}

function renderStatus(payload) {
  const nextRecording = Boolean(payload?.can_stop);
  if (!nextRecording || nextRecording !== recording) clearMeter();
  recording = nextRecording;
  if (!recording) stopping = false;
  const statusTitle = typeof payload?.title === "string" ? payload.title : "Orally";
  const statusDetail = typeof payload?.detail === "string" ? payload.detail : "";
  title.textContent = statusTitle;
  title.title = statusTitle;
  detail.textContent = statusDetail;
  detail.title = statusDetail;
  detail.classList.toggle("sr-only", recording);
  overlay.title = recording ? statusDetail : "";
  overlay.dataset.recording = String(recording);
  volumeMeter.hidden = !recording;
  renderStop();
}

function renderLevel(payload) {
  if (!recording) return;
  renderMeter(meter.push(payload));
  clearTimeout(staleLevelTimer);
  // No new audio data means no active waveform; this never generates samples.
  staleLevelTimer = setTimeout(clearMeter, 240);
}

const onMotionChange = ({ matches }) => { meter.reducedMotion = matches; };
motion.addEventListener("change", onMotionChange);
renderMeter(meter.reset());
if (invoke) renderStatus({ title: "语音输入", detail: "", can_stop: false });
else renderStop();

stop.addEventListener("click", async () => {
  if (!invoke || !recording || stopping) return;
  stopping = true;
  renderStop();
  try {
    await invoke("stop_recording");
  } catch (error) {
    stopping = false;
    renderStatus({ title: "停止录音失败", detail: String(error?.message ?? error), can_stop: recording });
  }
});

window.addEventListener("pagehide", () => {
  disposed = true;
  clearMeter();
  motion.removeEventListener("change", onMotionChange);
  for (const unlisten of unlisteners) { try { unlisten(); } catch {} }
});

if (listen) {
  const subscriptions = await Promise.allSettled([
    listen("dictation-status", ({ payload }) => renderStatus(payload)),
    listen("dictation-level", ({ payload }) => renderLevel(payload)),
  ]);
  for (const subscription of subscriptions) {
    if (subscription.status !== "fulfilled") continue;
    if (disposed) { try { subscription.value(); } catch {} }
    else unlisteners.push(subscription.value);
  }
  if (invoke && !disposed) { try { await invoke("replay_dictation_status"); } catch {} }
}
