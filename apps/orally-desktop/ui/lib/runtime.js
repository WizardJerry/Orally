export const nativeInvoke = globalThis.window?.__TAURI__?.core?.invoke;
export const nativeListen = globalThis.window?.__TAURI__?.event?.listen;

export function redactError(error, snapshot) {
  let message = error instanceof Error ? error.message : String(error);
  const sections = [snapshot?.asr, snapshot?.postprocess, ...(snapshot?.postprocess?.models ?? [])];
  for (const profile of snapshot?.profiles ?? []) {
    sections.push(profile.asr, profile.postprocess, ...(profile.postprocess?.models ?? []));
  }
  const keys = [...new Set(sections.map((section) => section?.api_key)
    .filter((key) => typeof key === "string" && key.length > 0))];
  for (const key of keys.sort((a, b) => b.length - a.length)) message = message.split(key).join("[API Key 已隐藏]");
  return message;
}
