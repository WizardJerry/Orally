import test from "node:test";
import assert from "node:assert/strict";
import { OverlaySession } from "../ui/overlay-session.js";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

test("overlay advances only on actual recording levels and clears stale or stopped audio", () => {
  let timer;
  const session = new OverlaySession({ invoke: async () => {}, reducedMotion: true, schedule: (callback) => { timer = callback; return 1; }, unschedule: () => {} });
  session.receiveLevel({ rms: 0.1, peak: 0.2 });
  assert.deepEqual(session.state.heights, Array(32).fill(2));
  session.receiveStatus({ title: "Recording", detail: "Stop when done", can_stop: true });
  session.receiveLevel({ rms: 0.1, peak: 0.2 });
  assert.ok(session.state.heights[31] > 2);
  timer();
  assert.deepEqual(session.state.heights, Array(32).fill(2));
  session.receiveLevel({ rms: 0.1, peak: 0.2 });
  session.receiveStatus({ title: "Processing", detail: "Recognition", can_stop: false });
  session.receiveLevel({ rms: 0.1, peak: 0.2 });
  assert.equal(session.state.recording, false);
  assert.deepEqual(session.state.heights, Array(32).fill(2));
  session.dispose();
});

test("Stop dispatches once and stays pending until native status confirms processing", async () => {
  const queued = deferred();
  const calls = [];
  const session = new OverlaySession({ invoke: (command) => { calls.push(command); return queued.promise; } });
  assert.equal(await session.stop(), false);
  session.receiveStatus({ can_stop: true });
  const stopping = session.stop();
  assert.equal(await session.stop(), false);
  queued.resolve();
  assert.equal(await stopping, true);
  assert.equal(session.state.stopping, true);
  assert.equal(await session.stop(), false);
  session.receiveStatus({ can_stop: false });
  assert.equal(session.state.stopping, false);
  assert.deepEqual(calls, ["stop_recording"]);
  session.dispose();
});

test("late subscriptions release immediately after unmount and cannot replay or update the overlay", async () => {
  const status = deferred();
  const level = deferred();
  let released = 0;
  let updates = 0;
  const calls = [];
  const handlers = {};
  const session = new OverlaySession({ invoke: async (command) => { calls.push(command); }, onChange: () => { updates += 1; }, listen: (event, handler) => {
    handlers[event] = handler;
    return event === "dictation-status" ? status.promise : level.promise;
  } });
  const starting = session.start();
  session.dispose();
  status.resolve(() => { released += 1; });
  level.resolve(() => { released += 1; });
  await starting;
  handlers["dictation-status"]({ payload: { can_stop: true } });
  handlers["dictation-level"]({ payload: { rms: 0.1, peak: 0.2 } });
  assert.equal(released, 2);
  assert.equal(updates, 0);
  assert.deepEqual(calls, []);
});

test("replay runs after both native listeners and each listener is released exactly once", async () => {
  const calls = [];
  const session = new OverlaySession({ invoke: async (command) => { calls.push(command); }, listen: async (event) => {
    calls.push(event);
    return () => { calls.push(`release:${event}`); };
  } });
  await session.start();
  assert.deepEqual(calls, ["dictation-status", "dictation-level", "replay_dictation_status"]);
  session.dispose();
  session.dispose();
  assert.deepEqual(calls.slice(3), ["release:dictation-status", "release:dictation-level"]);
});

test("failed native stop is retryable and browser preview never dispatches a simulated action", async () => {
  let calls = 0;
  const session = new OverlaySession({ invoke: async () => { calls += 1; throw new Error("native failure"); } });
  session.receiveStatus({ can_stop: true });
  assert.equal(await session.stop(), false);
  assert.equal(session.state.stopping, false);
  assert.equal(session.state.title, "停止录音失败");
  assert.equal(session.state.detail, "native failure");
  await session.stop();
  assert.equal(calls, 2);
  const browser = new OverlaySession();
  assert.equal(browser.state.recording, true);
  assert.deepEqual(browser.state.heights, Array(32).fill(2));
  assert.equal(await browser.stop(), false);
  browser.dispose();
  session.dispose();
});

test("an old failed Stop cannot replace a newer recording status", async () => {
  const queued = deferred();
  const session = new OverlaySession({ invoke: () => queued.promise });
  session.receiveStatus({ title: "First recording", can_stop: true });
  const stopping = session.stop();
  session.receiveStatus({ title: "Processing", can_stop: false });
  session.receiveStatus({ title: "Second recording", can_stop: true });
  queued.reject(new Error("old stop failed"));
  await stopping;
  assert.equal(session.state.title, "Second recording");
  assert.equal(session.state.stopping, false);
  session.dispose();
});
