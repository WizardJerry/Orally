import test from "node:test";
import assert from "node:assert/strict";
import { ConfigTestController, MAX_TEST_RECORDING_MS } from "../ui/lib/config-test-controller.js";

const tick = () => new Promise((resolve) => setImmediate(resolve));
const result = (text) => ({ raw_transcript: text, final_text: text, elapsed_ms: 1, warnings: [] });
const draft = () => ({
  asr: { model: "draft-asr", api_key: "fixture-key" },
  postprocess: { models: [{ model: "draft-llm", system_prompt: "draft instructions" }] },
  output: { auto_copy: false },
  privacy: { allow_external_requests: true },
});

function deferred() {
  let resolve;
  const promise = new Promise((yes) => { resolve = yes; });
  return { promise, resolve };
}

test("browser recording keeps the starting draft and only ASR mode bypasses refinement", async () => {
  for (const mode of ["voice", "asr"]) {
    let supplied;
    const controller = new ConfigTestController({
      record: async ({ maxDurationMs }) => {
        assert.equal(maxDurationMs, MAX_TEST_RECORDING_MS);
        return { cancel: async () => {}, stop: async () => ({ audioBytes: new Uint8Array([1, 2]), audioName: "fixture.wav" }) };
      },
      testInput: async (config, input) => { supplied = { config, input }; return result("recognized"); },
    });
    const config = draft();
    await controller.run(config, mode);
    config.asr.model = "edited during recording";
    config.postprocess.models[0].system_prompt = "later instructions";
    await controller.stop();
    assert.equal(supplied.config.asr.model, "draft-asr");
    assert.equal(supplied.config.postprocess.models[0].system_prompt, "draft instructions");
    assert.equal(supplied.input.asrOnly, mode === "asr");
    assert.equal(config.output.auto_copy, false);
    assert.deepEqual(supplied.input.audioBytes, new Uint8Array([1, 2]));
    assert.equal(controller.getState().phase, "complete");
  }
});

test("closing while native start is pending cancels its eventual ID and releases the listener", async () => {
  const start = deferred();
  const commands = [];
  let unlistens = 0;
  const controller = new ConfigTestController({
    listen: async () => () => { unlistens += 1; },
    invoke: async (command, args) => {
      commands.push({ command, args });
      return command === "start_test_recording" ? start.promise : undefined;
    },
  });
  const running = controller.run(draft(), "asr");
  await tick();
  assert.equal(commands[0].args.asrOnly, true);
  assert.equal(commands[0].args.config.asr.model, "draft-asr");
  const cleanup = controller.reset();
  start.resolve("late-native-id");
  await running;
  await cleanup;
  assert.equal(unlistens, 1);
  assert.deepEqual(commands.map(({ command }) => command), ["start_test_recording", "cancel_test_recording"]);
  assert.equal(commands[1].args.sessionId, "late-native-id");
  assert.equal(controller.getState().phase, "idle");
  assert.equal(controller.getState().result, null);
});

test("native limits arriving before the start reply apply only to the matching recording", async () => {
  const start = deferred();
  const commands = [];
  let emit;
  let unlistens = 0;
  const controller = new ConfigTestController({
    listen: async (name, callback) => {
      assert.equal(name, "test-recording-captured");
      emit = callback;
      return () => { unlistens += 1; };
    },
    invoke: async (command, args) => {
      commands.push({ command, args });
      return command === "start_test_recording" ? start.promise : result("at limit");
    },
  });
  const running = controller.run(draft(), "voice");
  await tick();
  emit({ payload: { session_id: "other", reason: "cancelled" } });
  emit({ payload: { session_id: "captured", reason: "size-limit" } });
  start.resolve("captured");
  await running;
  await tick();
  assert.equal(controller.getState().phase, "complete");
  assert.equal(controller.getState().closeRequested, false);
  assert.match(controller.getState().notice, /大小上限/);
  assert.deepEqual(commands.map(({ command }) => command), ["start_test_recording", "stop_test_recording"]);
  assert.equal(commands[1].args.sessionId, "captured");
  assert.equal(unlistens, 1);
  emit({ payload: { session_id: "captured", reason: "cancelled" } });
  assert.equal(controller.getState().phase, "complete", "released listeners cannot change a completed test");
});

test("native cancellation requests dialog closure and releases only its own session", async () => {
  let emit;
  const cancellations = [];
  const controller = new ConfigTestController({
    listen: async (_name, callback) => { emit = callback; return () => {}; },
    invoke: async (command, args) => {
      if (command === "start_test_recording") return "current";
      if (command === "cancel_test_recording") cancellations.push(args.sessionId);
    },
  });
  await controller.run(draft(), "voice");
  emit({ payload: { session_id: "other", reason: "cancelled" } });
  assert.equal(controller.getState().phase, "recording");
  emit({ payload: { session_id: "current", reason: "cancelled" } });
  await controller.session.cleanup;
  assert.equal(controller.getState().closeRequested, true);
  assert.equal(controller.getState().phase, "idle");
  assert.deepEqual(cancellations, ["current"]);
});

test("duration limit stops a recording even when native events are unavailable", async () => {
  const commands = [];
  const controller = new ConfigTestController({
    invoke: async (command) => { commands.push(command); return command === "start_test_recording" ? "native-id" : result("limited"); },
  });
  await controller.run(draft(), "voice");
  assert.match(controller.getState().notice, /通知不可用/);
  controller.tick(controller.getState().startedAt + MAX_TEST_RECORDING_MS);
  await tick();
  assert.equal(controller.getState().phase, "complete");
  assert.match(controller.getState().notice, /时长上限/);
  assert.deepEqual(commands, ["start_test_recording", "stop_test_recording"]);
});

test("late native processing results cannot replace a later text test", async () => {
  const stopping = deferred();
  const commands = [];
  const controller = new ConfigTestController({
    listen: async () => () => {},
    invoke: async (command, args) => {
      commands.push({ command, args });
      if (command === "start_test_recording") return "old-recording";
      if (command === "stop_test_recording") return stopping.promise;
      if (command === "test_config_input") return result(args.inputText);
    },
  });
  await controller.run(draft(), "voice");
  const pending = controller.stop();
  await tick();
  await controller.reset();
  await controller.run(draft(), "text", "new text result");
  stopping.resolve(result("stale recording result"));
  await pending;
  assert.equal(controller.getState().result.final_text, "new text result");
  assert.equal(commands.filter(({ command }) => command === "cancel_test_recording").length, 1);
  const textCommand = commands.find(({ command }) => command === "test_config_input");
  assert.equal(textCommand.args.audioBytes, null);
  assert.equal(textCommand.args.config.postprocess.models[0].system_prompt, "draft instructions");
});

test("privacy and invalid text prevent microphone or service requests", async () => {
  let requests = 0;
  const controller = new ConfigTestController({ invoke: async () => { requests += 1; } });
  const config = draft();
  config.privacy.allow_external_requests = false;
  await controller.run(config, "voice");
  assert.match(controller.getState().error.message, /禁止外部请求/);
  await controller.run(config, "text", " ");
  assert.match(controller.getState().error.message, /请输入/);
  await controller.run(config, "text", "a".repeat(100_001));
  assert.match(controller.getState().error.message, /100,000/);
  assert.equal(requests, 0);
});

test("a listener resolving after dialog closure is removed without starting native capture", async () => {
  const listening = deferred();
  let unlistens = 0;
  let starts = 0;
  const controller = new ConfigTestController({
    listen: async () => listening.promise,
    invoke: async () => { starts += 1; },
  });
  const running = controller.run(draft(), "voice");
  await tick();
  const cleanup = controller.reset();
  listening.resolve(() => { unlistens += 1; });
  await running;
  await cleanup;
  assert.equal(starts, 0);
  assert.equal(unlistens, 1);
  assert.equal(controller.getState().phase, "idle");
});
