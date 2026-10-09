const KEY_NAMES = {
  Space: "space", Enter: "enter", Tab: "tab", Escape: "escape", Backspace: "backspace",
  Delete: "delete", Insert: "insert", Home: "home", End: "end", PageUp: "pageup", PageDown: "pagedown",
  ArrowUp: "arrowup", ArrowDown: "arrowdown", ArrowLeft: "arrowleft", ArrowRight: "arrowright",
};
const KEY_LABELS = {
  ctrl: "Ctrl", alt: "Alt", shift: "Shift", win: "Win", space: "Space", enter: "Enter", tab: "Tab",
  escape: "Esc", backspace: "Backspace", delete: "Delete", insert: "Insert", home: "Home", end: "End",
  pageup: "PageUp", pagedown: "PageDown", arrowup: "↑", arrowdown: "↓", arrowleft: "←", arrowright: "→",
};

export function hotkeyFromEvent(event) {
  if (event.repeat || event.isComposing || ["Control", "Alt", "Shift", "Meta", "AltGraph"].includes(event.key)) return null;
  if (event.code?.startsWith("Numpad")) return null;
  let key = KEY_NAMES[event.code] ?? KEY_NAMES[event.key];
  if (/^Key[A-Z]$/.test(event.code ?? "")) key = event.code.slice(3).toLowerCase();
  else if (/^Digit[0-9]$/.test(event.code ?? "")) key = event.code.slice(5);
  else if (/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(event.code || event.key || "")) key = (event.code || event.key).toLowerCase();
  else if (!key && /^[a-z0-9]$/i.test(event.key ?? "")) key = event.key.toLowerCase();
  else if (!key && event.key === " ") key = "space";
  if (!key) return null;
  const modifiers = [];
  if (event.ctrlKey) modifiers.push("ctrl");
  if (event.altKey) modifiers.push("alt");
  if (event.shiftKey) modifiers.push("shift");
  if (event.metaKey) modifiers.push("win");
  if (modifiers.length === 0 && !/^f\d+$/.test(key)) return null;
  return [...modifiers, key].join("-");
}

export function formatHotkey(value) {
  return String(value ?? "").split(/[-+]/).map((part) => {
    const token = part.trim().toLowerCase();
    return KEY_LABELS[token] ?? token.toUpperCase();
  }).filter(Boolean).join(" + ");
}

// A captured combination becomes a draft only after the old native binding is restored.
export class HotkeyCaptureSession {
  constructor({ setActive = async () => {}, onCommit = () => {}, onChange = () => {} } = {}) {
    this.setActive = setActive;
    this.onCommit = onCommit;
    this.onChange = onChange;
    this.phase = "idle";
    this.error = null;
    this.restorationFailed = false;
    this.version = 0;
    this.activation = null;
    this.restoration = null;
    this.staged = null;
    this.releaseKey = null;
  }

  get busy() { return this.phase !== "idle"; }

  clearError() {
    this.error = null;
    this.restorationFailed = false;
    this.onChange(this);
  }

  async start() {
    if (this.busy) return false;
    const version = ++this.version;
    this.phase = "preparing";
    this.error = null;
    this.restorationFailed = false;
    this.onChange(this);
    this.activation = Promise.resolve().then(() => this.setActive(true));
    try { await this.activation; }
    catch (error) {
      if (version === this.version) {
        this.error = error;
        await this.cancel();
      }
      return false;
    }
    if (version !== this.version || this.phase !== "preparing") return false;
    this.activation = null;
    this.phase = "capturing";
    this.onChange(this);
    return true;
  }

  handleKey(event) {
    if (!this.busy || event.repeat) return Promise.resolve(false);
    if (event.key === "Escape" || event.code === "Escape") return this.cancel();
    if (this.phase !== "capturing") return Promise.resolve(false);
    const value = hotkeyFromEvent(event);
    if (!value) return Promise.resolve(false);
    this.staged = value;
    this.releaseKey = event.code || event.key.toLowerCase();
    this.phase = "awaiting-release";
    this.onChange(this);
    return Promise.resolve(true);
  }

  handleKeyUp(event) {
    if (this.phase !== "awaiting-release" || (event.code || event.key?.toLowerCase()) !== this.releaseKey) return Promise.resolve(false);
    return this.finish(this.staged);
  }

  cancel() { return this.finish(null); }

  finish(value) {
    if (!this.busy) return Promise.resolve(false);
    this.staged = value;
    if (this.restoration) return this.restoration;
    ++this.version;
    const activation = this.activation;
    this.phase = "restoring";
    this.onChange(this);
    this.restoration = (async () => {
      try {
        if (activation) { try { await activation; } catch {} }
        await this.setActive(false);
      } catch (error) {
        this.error = error;
        this.restorationFailed = true;
      }
      const captured = this.staged;
      this.staged = null;
      this.releaseKey = null;
      this.activation = null;
      this.phase = "idle";
      if (captured) this.onCommit(captured);
      return Boolean(captured);
    })().finally(() => {
      this.restoration = null;
      this.onChange(this);
    });
    return this.restoration;
  }
}
