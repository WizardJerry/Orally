const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

const stop = document.querySelector("#stop");
const title = document.querySelector("#title");
const detail = document.querySelector("#detail");

stop.addEventListener("click", async () => {
  stop.disabled = true;
  try {
    await invoke("stop_recording");
  } finally {
    stop.disabled = false;
  }
});

await listen("dictation-status", ({ payload }) => {
  title.textContent = payload.title;
  detail.textContent = payload.detail;
  stop.hidden = !payload.can_stop;
});
