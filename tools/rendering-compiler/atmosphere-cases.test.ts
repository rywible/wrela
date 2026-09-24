import { expect, test } from "bun:test";
import {
  ATMOSPHERE_CASES,
  type AtmosphereSample,
  atmosphereCamera,
  atmosphereSun,
  summarizeAtmosphere,
  validateAtmosphereSamples,
} from "./atmosphere-cases";

test("atmosphere edge readback rejects invalid HDR even in a single pixel", () => {
  for (const bad of [NaN, Infinity, -Infinity, -0.001]) {
    expect(() => summarizeAtmosphere("bad", new Float32Array([1, bad, 1, 1]), new Uint8Array(4))).toThrow();
  }
  const sample = summarizeAtmosphere(
    "valid",
    new Float32Array([2, 1, 0, 1]),
    new Uint8Array([255, 0, 0, 255]),
  );
  expect(sample.maximum).toBe(2);
  expect(sample.meanRgb).toEqual([2, 1, 0]);
  expect(sample.displayMean).toBeCloseTo(1 / 3);
});
function passingSamples(): AtmosphereSample[] {
  return ATMOSPHERE_CASES.map(({ id }) => ({
    id,
    meanRgb: [1, 1, 1],
    maximum: id === "planet-shadow" ? 0 : 1,
    meanLuminance: id === "sunset" ? 0.2 : id === "high-altitude" ? 0.01 : 1,
    displayMean: id === "exposure-high" ? 0.8 : 0.2,
    linearPixels: 1,
  }));
}
test("behavioral gates detect frozen lighting, planet light leaks and exposure in the wrong stage", () => {
  expect(validateAtmosphereSamples(passingSamples(), 0).planetShadow).toBe(true);
  for (const [id, property, value] of [
    ["high-altitude", "meanLuminance", 1],
    ["sunset", "meanLuminance", 1],
    ["planet-shadow", "maximum", 0.1],
    ["horizon-below", "meanLuminance", 0.5],
    ["exposure-high", "displayMean", 0.2],
  ] as const) {
    const cases = passingSamples();
    const item = cases.find((sample) => sample.id === id);
    if (!item) throw Error("Missing test case");
    item[property] = value;
    expect(() => validateAtmosphereSamples(cases, 0)).toThrow();
  }
  expect(() => validateAtmosphereSamples(passingSamples(), 0.01)).toThrow();
  expect(() => validateAtmosphereSamples(passingSamples(), NaN)).toThrow();
});
test("bounded cases preserve sun orientation and high-altitude camera look direction", () => {
  expect(ATMOSPHERE_CASES.length).toBe(8);
  for (const item of ATMOSPHERE_CASES) {
    expect(Math.hypot(...atmosphereSun(item.sunElevation))).toBeCloseTo(1, 12);
    const camera = atmosphereCamera(item.height);
    expect(camera.target[1] - camera.position[1]).toBeCloseTo(0.2, 9);
    expect(item.exposure).toBeGreaterThan(0);
  }
});
