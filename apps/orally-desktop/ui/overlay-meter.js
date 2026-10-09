export const OVERLAY_BAR_COUNT = 32;
const NOISE_FLOOR = 0.002;
const FLOOR_DB = 20 * Math.log10(NOISE_FLOOR);
const FULL_DB = -16;
const bounded = (value) => Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : 0;

// RMS gates silence; peak adds actual transients without amplifying isolated noise.
export function normalizeDictationLevel(payload = {}) {
  const rms = bounded(payload?.rms);
  const peak = bounded(payload?.peak);
  if (rms <= NOISE_FLOOR) return 0;
  const amplitude = Math.max(rms, Math.min(peak, rms * 4) * 0.5);
  return bounded((20 * Math.log10(amplitude) - FLOOR_DB) / (FULL_DB - FLOOR_DB));
}

export function createMeterHeights(levels) {
  return levels.map((level) => Math.round((2 + bounded(level) * 16) * 100) / 100);
}

export class OverlayMeter {
  constructor({ reducedMotion = false } = {}) {
    this.reducedMotion = reducedMotion;
    this.reset();
  }

  reset() {
    this.level = 0;
    this.history = Array(OVERLAY_BAR_COUNT).fill(0);
    return createMeterHeights(this.history);
  }

  push(payload) {
    const incoming = normalizeDictationLevel(payload);
    if (incoming === 0) return this.reset();
    const response = this.reducedMotion ? 1 : incoming > this.level ? 0.72 : 0.5;
    this.level += (incoming - this.level) * response;
    this.history.shift();
    this.history.push(this.level);
    return createMeterHeights(this.history);
  }
}
