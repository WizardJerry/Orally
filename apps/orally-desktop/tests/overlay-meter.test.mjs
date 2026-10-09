import test from "node:test";
import assert from "node:assert/strict";
import { OverlayMeter, OVERLAY_BAR_COUNT, normalizeDictationLevel, createMeterHeights } from "../ui/overlay-meter.js";

test("silence and invalid microphone levels stay flat even with a spurious peak", () => {
  for (const rms of [0, 0.001, 0.002, -1, NaN, Infinity, undefined]) {
    assert.equal(normalizeDictationLevel({ rms, peak: 1 }), 0);
  }
  assert.equal(normalizeDictationLevel(null), 0);
  assert.deepEqual(new OverlayMeter().reset(), Array(OVERLAY_BAR_COUNT).fill(2));
});

test("quiet speech is visible and louder real input increases bounded heights", () => {
  const quiet = normalizeDictationLevel({ rms: 0.004, peak: 0.008 });
  const speech = normalizeDictationLevel({ rms: 0.04, peak: 0.08 });
  assert.ok(quiet > 0.1 && quiet < speech);
  assert.equal(normalizeDictationLevel({ rms: 0.5, peak: 1 }), 1);
  assert.deepEqual(createMeterHeights([0, quiet, speech, 1, 4, NaN]), [2, Math.round((2 + quiet * 16) * 100) / 100, Math.round((2 + speech * 16) * 100) / 100, 18, 18, 2]);
});

test("meter history advances only with supplied audio and retains exactly 32 samples", () => {
  const meter = new OverlayMeter({ reducedMotion: true });
  const first = meter.push({ rms: 0.01, peak: 0.02 });
  const second = meter.push({ rms: 0.04, peak: 0.08 });
  assert.equal(first.length, 32);
  assert.deepEqual(first.slice(0, 31), Array(31).fill(2));
  assert.equal(second[30], first[31]);
  assert.ok(second[31] > second[30]);
  for (let index = 0; index < 40; index += 1) meter.push({ rms: 0.04, peak: 0.08 });
  assert.equal(meter.history.length, 32);
  assert.equal(new Set(createMeterHeights(meter.history)).size, 1, "constant audio settles into a constant waveform");
});

test("silence immediately removes previous waves and reset clears a stopped recording", () => {
  const meter = new OverlayMeter();
  meter.push({ rms: 0.1, peak: 0.2 });
  assert.deepEqual(meter.push({ rms: 0.001, peak: 0.01 }), Array(32).fill(2));
  meter.push({ rms: 0.1, peak: 0.2 });
  assert.deepEqual(meter.reset(), Array(32).fill(2));
  assert.equal(meter.level, 0);
});

test("reduced motion removes smoothing while preserving genuine level history", () => {
  const payload = { rms: 0.03, peak: 0.06 };
  const smooth = new OverlayMeter().push(payload);
  const direct = new OverlayMeter({ reducedMotion: true }).push(payload);
  assert.ok(smooth[31] < direct[31]);
  assert.equal(direct[31], createMeterHeights([normalizeDictationLevel(payload)])[0]);
});
