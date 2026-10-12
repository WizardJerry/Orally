import test from "node:test";
import assert from "node:assert/strict";
import { hotkeyFromEvent, formatHotkey, HotkeyCaptureSession } from "../ui/hotkey-capture.js";
import { ConfigEditor } from "../ui/editor-state.js";

const event = (key, code, modifiers = {}) => ({ key, code, ...modifiers });
function deferred() {
  let resolve;
  const promise = new Promise((yes) => { resolve = yes; });
  return { promise, resolve };
}
const tick = () => new Promise((resolve) => setImmediate(resolve));

test("physical letters, digits, function and navigation keys use the native canonical token contract", () => {
  assert.equal(hotkeyFromEvent(event("k", "KeyK", { ctrlKey: true, altKey: true })), "ctrl-alt-k");
  assert.equal(hotkeyFromEvent(event("!", "Digit1", { shiftKey: true, metaKey: true })), "shift-win-1");
  assert.equal(hotkeyFromEvent(event("F24", "F24")), "f24");
  assert.equal(hotkeyFromEvent(event("PageDown", "PageDown", { altKey: true })), "alt-pagedown");
  assert.equal(hotkeyFromEvent(event("ArrowUp", "ArrowUp", { ctrlKey: true })), "ctrl-arrowup");
  assert.equal(hotkeyFromEvent(event(" ", "Space", { ctrlKey: true, shiftKey: true })), "ctrl-shift-space");
  assert.equal(formatHotkey("ctrl-alt-shift-win-k"), "Ctrl + Alt + Shift + Win + K");
});

test("modifiers, repeat, IME, unsupported keys and bare typing cannot become a global combination", () => {
  for (const item of [
    event("Control", "ControlLeft", { ctrlKey: true }), event("a", "KeyA"), event(" ", "Space"),
    event("F25", "F25"), event(";", "Semicolon", { ctrlKey: true }), event("1", "Numpad1", { ctrlKey: true }),
    event("k", "KeyK", { ctrlKey: true, repeat: true }), event("Process", "KeyK", { ctrlKey: true, isComposing: true }),
  ]) assert.equal(hotkeyFromEvent(item), null);
});

test("capture waits for native suspension and restores before committing an unsaved combination", async () => {
  const suspended = deferred();
  const restored = deferred();
  const toggles = [];
  const commits = [];
  const session = new HotkeyCaptureSession({ setActive: (active) => {
    toggles.push(active);
    return active ? suspended.promise : restored.promise;
  }, onCommit: (value) => commits.push(value) });
  const starting = session.start();
  assert.equal(session.phase, "preparing");
  assert.equal(await session.handleKey(event("s", "KeyS", { ctrlKey: true })), false);
  suspended.resolve();
  await starting;
  await session.handleKey(event("k", "KeyK", { ctrlKey: true, altKey: true }));
  assert.equal(session.phase, "awaiting-release");
  assert.deepEqual(toggles, [true]);
  assert.equal(await session.handleKeyUp(event("Control", "ControlLeft")), false);
  const finishing = session.handleKeyUp(event("k", "KeyK"));
  assert.equal(session.phase, "restoring");
  assert.deepEqual(commits, []);
  assert.equal(session.busy, true);
  restored.resolve();
  await finishing;
  assert.deepEqual(toggles, [true, false]);
  assert.deepEqual(commits, ["ctrl-alt-k"]);
  assert.equal(session.busy, false);
});

test("Escape cancellation and late activation restore the previous binding without changing the draft", async () => {
  const activation = deferred();
  const toggles = [];
  const commits = [];
  const session = new HotkeyCaptureSession({ setActive: (active) => {
    toggles.push(active);
    return active ? activation.promise : Promise.resolve();
  }, onCommit: (value) => commits.push(value) });
  const starting = session.start();
  await tick();
  const cancelling = session.handleKey(event("Escape", "Escape"));
  activation.resolve();
  await Promise.all([starting, cancelling]);
  assert.deepEqual(toggles, [true, false]);
  assert.deepEqual(commits, []);
  assert.equal(session.phase, "idle");
  await session.start();
  await session.handleKey(event("Escape", "Escape"));
  assert.deepEqual(commits, []);
  assert.equal(session.busy, false);
});

test("blur or navigation cancellation invalidates a captured key while native restoration is pending", async () => {
  const restoration = deferred();
  const commits = [];
  const session = new HotkeyCaptureSession({ setActive: (active) => active ? Promise.resolve() : restoration.promise, onCommit: (value) => commits.push(value) });
  await session.start();
  await session.handleKey(event("F9", "F9"));
  const finishing = session.handleKeyUp(event("F9", "F9"));
  const cancelling = session.cancel();
  restoration.resolve();
  await Promise.all([finishing, cancelling]);
  assert.deepEqual(commits, []);
  assert.equal(session.phase, "idle");
});

test("activation failures restore defensively and cancelled restoration conflicts leave capture available", async () => {
  const toggles = [];
  const activationFailed = new HotkeyCaptureSession({ setActive: async (active) => {
    toggles.push(active);
    if (active) throw new Error("native suspend failed");
  } });
  await activationFailed.start();
  assert.deepEqual(toggles, [true, false]);
  assert.equal(activationFailed.busy, false);
  assert.match(activationFailed.error.message, /suspend/);
  let failedOnce = false;
  const commits = [];
  const session = new HotkeyCaptureSession({ setActive: async (active) => {
    if (!active && !failedOnce) { failedOnce = true; throw new Error("native restore failed"); }
  }, onCommit: (value) => commits.push(value) });
  await session.start();
  await session.cancel();
  assert.equal(session.phase, "idle");
  assert.equal(session.busy, false);
  assert.deepEqual(commits, []);
  await session.start();
  await session.cancel();
  assert.equal(session.busy, false);
  assert.deepEqual(commits, []);
  assert.equal(session.error, null);
});

test("a native restoration conflict keeps the captured draft saveable and permits another combination", async () => {
  const editor = new ConfigEditor();
  let conflict = true;
  const session = new HotkeyCaptureSession({
    setActive: async (active) => { if (!active && conflict) throw new Error("old shortcut is occupied"); },
    onCommit: (value) => { editor.config.hotkey.preset = value; },
  });
  await session.start();
  await session.handleKey(event("k", "KeyK", { ctrlKey: true, altKey: true }));
  await session.handleKeyUp(event("k", "KeyK"));
  assert.equal(session.busy, false);
  assert.equal(editor.config.hotkey.preset, "ctrl-alt-k");
  assert.equal(editor.dirty, true);
  assert.match(session.error.message, /occupied/);
  editor.markSaved(editor.snapshot());
  assert.equal(editor.dirty, false);
  conflict = false;
  await session.start();
  await session.handleKey(event("F12", "F12"));
  await session.handleKeyUp(event("F12", "F12"));
  assert.equal(session.busy, false);
  assert.equal(session.error, null);
  assert.equal(editor.config.hotkey.preset, "f12");
  assert.equal(editor.dirty, true);
});
