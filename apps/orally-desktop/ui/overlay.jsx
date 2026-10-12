import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { Button } from "@/components/ui/button";
import { nativeInvoke, nativeListen, redactError } from "@/lib/runtime";
import { OVERLAY_BAR_COUNT } from "./overlay-meter.js";
import { OverlaySession } from "./overlay-session.js";
import "./theme.css";
import "./overlay.css";

const STATIC_HEIGHTS = Array(OVERLAY_BAR_COUNT).fill(2);
const STOP_ICON = <svg viewBox="0 0 12 12" fill="currentColor" aria-hidden="true"><rect x="2" y="2" width="8" height="8" rx="1" /></svg>;

function DictationOverlay() {
  const sessionRef = useRef(null);
  const [state, setState] = useState(() => ({ title: nativeInvoke ? "语音输入" : "正在录音…", detail: nativeInvoke ? "" : "按快捷键或点击停止", recording: !nativeInvoke, stopping: false, heights: STATIC_HEIGHTS }));

  useEffect(function connectDictationOverlay() {
    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const session = new OverlaySession({ invoke: nativeInvoke, listen: nativeListen, onChange: setState, formatError: redactError, reducedMotion: motion.matches });
    sessionRef.current = session;
    setState(session.state);
    function updateMotion(event) { session.meter.reducedMotion = event.matches; }
    function disposeSession() { session.dispose(); }
    motion.addEventListener("change", updateMotion);
    window.addEventListener("pagehide", disposeSession);
    void session.start();
    return function disconnectDictationOverlay() {
      session.dispose();
      if (sessionRef.current === session) sessionRef.current = null;
      motion.removeEventListener("change", updateMotion);
      window.removeEventListener("pagehide", disposeSession);
    };
  }, []);

  const silent = state.heights.every((height) => height === 2);
  return (
    <main className="dictation-pill" data-recording={state.recording} aria-label="Orally 语音输入状态" title={state.recording ? state.detail : undefined}>
      <div className="overlay-header">
        <div className="overlay-status">
          <span className="recording-dot" aria-hidden="true" />
          <strong id="title" role="status" aria-live="polite" title={state.title}>{state.title}</strong>
        </div>
        <span className="overlay-brand">Orally</span>
        {state.recording ? <Button id="stop" variant="ghost" size="icon" disabled={!nativeInvoke || state.stopping}
          aria-label="停止语音输入" title="停止语音输入" aria-busy={state.stopping} onClick={() => { void sessionRef.current?.stop(); }}>{STOP_ICON}</Button> : null}
      </div>
      <div className="overlay-bottom">
        {state.recording ? <div id="meter" className={`volume-meter${silent ? " is-silent" : ""}`} aria-hidden="true">
          {state.heights.map((height, index) => <span className="meter-bar" key={index} style={{ height: `${height}px` }} />)}
        </div> : null}
        <p id="detail" className={state.recording ? "sr-only" : undefined} role="status" aria-live="polite" title={state.detail}>{state.detail}</p>
      </div>
    </main>
  );
}

createRoot(document.getElementById("root")).render(<DictationOverlay />);
