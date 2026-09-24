import { expect, test } from "bun:test";
import { CLOUD_FORMATION_FRAME_OFFSET, CLOUD_FRAME_FLOATS } from "./cloud-formations";
import { canReuseCloudHistory, cloudHistoryMaxAge } from "./cloud-history";

function frame() {
  const f = new Float32Array(CLOUD_FRAME_FLOATS);
  f[16] = 1;
  f[25] = 0.5;
  f[26] = Math.sqrt(0.75);
  f[28] = 4 / 3;
  f[29] = 0.6;
  f[36] = 0.5;
  f[37] = 1;
  f[39] = 10;
  f[48] = 0.8;
  return f;
}
test("small camera, wind, shape and light changes reuse; large changes reset", () => {
  const before = frame(),
    after = frame();
  after[0] += 1;
  after[39] += 0.5;
  after[36] += 0.001;
  after[4] += 0.001;
  expect(canReuseCloudHistory(before, after)).toBe(true);
  for (const [index, delta] of [
    [0, 1000],
    [39, 1000],
    [36, 0.3],
    [4, 0.2],
    [12, 8192],
    [28, 0.5],
    [48, 0.2],
    [50, 0.2],
  ]) {
    const changed = frame();
    changed[index] += delta;
    expect(canReuseCloudHistory(before, changed)).toBe(false);
  }
});
test("source switch, camera cuts, and invalid data reset history", () => {
  const before = frame(),
    after = frame();
  before[44] = 0.495;
  after[44] = 0.505;
  expect(canReuseCloudHistory(before, after)).toBe(false);
  after.set(before);
  after[26] *= -1;
  expect(canReuseCloudHistory(before, after)).toBe(false);
  after.set(before);
  after[39] = Number.NaN;
  expect(canReuseCloudHistory(before, after)).toBe(false);
});

test("large wind steps redraw mixed upper and lower cloud depths", () => {
  const before = frame(),
    after = frame();
  before[50] = after[50] = 0.3;
  after[39] += 3;
  expect(canReuseCloudHistory(before, after)).toBe(true);
  after[39] += 5;
  expect(canReuseCloudHistory(before, after)).toBe(false);
  before[50] = after[50] = 0;
  expect(canReuseCloudHistory(before, after)).toBe(true);
});

test("sunset light expires before motion-only cloud history", () => {
  const before = frame(),
    after = frame();
  before[5] = after[5] = 0.075;
  after[0] += 1;
  expect(cloudHistoryMaxAge(before, after)).toBe(7);
  after[5] -= 0.008;
  expect(cloudHistoryMaxAge(before, after)).toBe(1);
  before[5] = 0.55;
  after[5] = 0.542;
  expect(cloudHistoryMaxAge(before, after)).toBe(3);
  before[5] = -0.3;
  after[5] = -0.308;
  expect(cloudHistoryMaxAge(before, after)).toBe(3);
});

test("opening weather groups expires stale contours without resetting stable camera motion", () => {
  const before = frame(),
    after = frame();
  const background = CLOUD_FORMATION_FRAME_OFFSET + 1;
  before[background] = after[background] = 0.4;
  after[0] += 1;
  expect(canReuseCloudHistory(before, after)).toBe(true);
  expect(cloudHistoryMaxAge(before, after)).toBe(7);
  after[background] += 0.002;
  expect(canReuseCloudHistory(before, after)).toBe(true);
  expect(cloudHistoryMaxAge(before, after)).toBe(1);
  after[background] += 0.1;
  expect(canReuseCloudHistory(before, after)).toBe(false);
});

test("middle-layer wind and edits invalidate mixed-depth history", () => {
  const before = frame(),
    after = frame();
  const middle = CLOUD_FORMATION_FRAME_OFFSET + 4;
  before[middle] = after[middle] = 0.3;
  after[39] += 8;
  expect(canReuseCloudHistory(before, after)).toBe(false);
  after.set(before);
  after[middle + 1] += 100;
  expect(canReuseCloudHistory(before, after)).toBe(false);
});
