import test from "node:test";
import assert from "node:assert/strict";
import { TestSession } from "../ui/test-session.js";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

const tick = () => new Promise((resolve) => setImmediate(resolve));
const result = (text) => ({ raw_transcript: text, final_text: text, elapsed_ms: 1, warnings: [] });

test("recording captures the entire draft at start and stops only once", async () => {
  const phases = [];
  const session = new TestSession((state) => phases.push(state.phase));
  const draft = { asr: { api_key: "fake-asr-key", model: "original" }, output: { raw: false }, models: ["first", "second"] };
  let stops = 0;
  await session.startRecording(draft, async (captured) => ({
    stop: async () => { stops += 1; return result(`${captured.asr.model}/${captured.models.join(",")}/${captured.output.raw}`); },
    cancel: async () => {},
  }));
  draft.asr.model = "later edit";
  draft.models.reverse();
  draft.output.raw = true;
  await Promise.all([session.stopRecording(), session.stopRecording()]);
  assert.equal(stops, 1);
  assert.equal(session.phase, "complete");
  assert.equal(session.result.final_text, "original/first,second/false");
  assert.deepEqual(phases, ["idle", "starting", "recording", "processing", "complete"]);
  assert.equal(session.busy, false);
});

test("permission resolving after cancellation releases its recording and cannot reopen the session", async () => {
  const permission = deferred();
  const session = new TestSession();
  let signal;
  let cancelled = 0;
  const starting = session.startRecording({}, async (_, options) => {
    signal = options.signal;
    return permission.promise;
  });
  await tick();
  session.reset();
  assert.equal(signal.aborted, true);
  permission.resolve({ stop: async () => result("late"), cancel: async () => { cancelled += 1; } });
  await starting;
  await session.cleanup;
  assert.equal(cancelled, 1);
  assert.equal(session.phase, "idle");
  assert.equal(session.result, null);
  assert.equal(session.recording, null);
});

test("a new recording waits for a cancelled pending start and its microphone cleanup", async () => {
  const permission = deferred();
  const release = deferred();
  const session = new TestSession();
  const first = session.startRecording({}, async () => permission.promise);
  await tick();
  let secondStarted = false;
  const second = session.startRecording({}, async () => {
    secondStarted = true;
    return { stop: async () => result("second"), cancel: async () => {} };
  });
  await tick();
  assert.equal(secondStarted, false);
  permission.resolve({ stop: async () => result("old"), cancel: () => release.promise });
  await tick();
  assert.equal(secondStarted, false, "a late acquired microphone must be released first");
  release.resolve();
  await first;
  await second;
  assert.equal(secondStarted, true);
  assert.equal(session.phase, "recording");
  session.reset();
  await session.cleanup;
});

test("a dismissed processing response cannot replace a newer recording or its result", async () => {
  const oldResult = deferred();
  const session = new TestSession();
  let cancellations = 0;
  await session.startRecording({ profile: "first" }, async () => ({
    stop: () => oldResult.promise,
    cancel: async () => { cancellations += 1; },
  }));
  const oldStopping = session.stopRecording();
  session.reset();
  await session.startRecording({ profile: "second" }, async () => ({
    stop: async () => result("new result"), cancel: async () => {},
  }));
  oldResult.resolve(result("stale result"));
  await oldStopping;
  assert.equal(cancellations, 1);
  assert.equal(session.phase, "recording");
  assert.equal(session.snapshot.profile, "second");
  assert.equal(session.result, null);
  await session.stopRecording();
  assert.equal(session.result.final_text, "new result");
});

test("capture or processing errors release resources and allow another test", async () => {
  const session = new TestSession();
  await session.startRecording({}, async () => { throw new Error("permission denied"); });
  assert.equal(session.phase, "error");
  assert.equal(session.error.message, "permission denied");
  let cancelled = 0;
  await session.startRecording({}, async () => ({
    stop: async () => { throw new Error("provider failed"); },
    cancel: async () => { cancelled += 1; },
  }));
  await session.stopRecording();
  assert.equal(cancelled, 1);
  assert.equal(session.phase, "error");
  assert.equal(session.recording, null);
  await session.startRecording({}, async () => ({ stop: async () => result("retried"), cancel: async () => {} }));
  await session.stopRecording();
  assert.equal(session.result.final_text, "retried");
});

test("text tests use a captured draft and discard responses after cancellation", async () => {
  const response = deferred();
  const session = new TestSession();
  const draft = { prompt: "original" };
  let signal;
  let received;
  const testing = session.runText(draft, "sample", async (captured, text, options) => {
    received = { captured, text };
    signal = options.signal;
    return response.promise;
  });
  draft.prompt = "later edit";
  await tick();
  assert.equal(received.captured.prompt, "original");
  assert.equal(received.text, "sample");
  session.reset();
  assert.equal(signal.aborted, true);
  response.resolve(result("old text result"));
  await testing;
  assert.equal(session.phase, "idle");
  assert.equal(session.result, null);
});
