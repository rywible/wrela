import { expect, test } from "bun:test";
import { atmosphereTransmissionWidth, integrateAtmosphereRay } from "./atmosphere";
import { compileAuthoredAtmosphere } from "./atmosphere-authoring";

test("analytic extinction interval maximum bounds independent coefficient samples", () => {
  for (const [lower, upper, maximum] of [
    [0, 0.3, 530],
    [1, 1.001, 530],
    [500, 500.1, 0.02],
    [0.00001, 0.00002, 1],
  ]) {
    const bound = atmosphereTransmissionWidth(lower, upper, maximum);
    for (let i = 0; i <= 1000; i++) {
      const k = maximum * (i / 1000) ** 4;
      expect(Math.exp(-k * lower) - Math.exp(-k * upper)).toBeLessThanOrEqual(bound + 1e-15);
    }
  }
});

test("uniform integration preserves bounds across changed weather coefficients", () => {
  const integral = integrateAtmosphereRay(
    { planetRadius: 6371, topHeight: 100, scaleHeight: 0.012, extinction: 0 },
    { height: 0, cosine: 0, length: 500 },
    { transmissionBudget: 1e-4, extinctionUpperBound: 530, maxSegments: 256 },
  );
  expect(integral.status).toBe("ready");
  expect(atmosphereTransmissionWidth(integral.lower, integral.upper, 530)).toBeLessThanOrEqual(1e-4);
  expect(integral.segmentCount).toBeGreaterThan(1);
});

test("authored weather reuses density body with isolated headers and explicit error evidence", () => {
  const clear = compileAuthoredAtmosphere(1, 0),
    rain = compileAuthoredAtmosphere(10, 0.1);
  expect(clear.exhaustedCells).toBe(0);
  expect(clear.maximumTransmissionWidth).toBeLessThanOrEqual(1e-4);
  expect(clear.numericError).toBe("unknown");
  expect(clear.interpolationError).toBe("unknown");
  expect(clear.data.subarray(16)).toEqual(rain.data.subarray(16));
  expect(clear.data[9]).toBe(0);
  expect(rain.data[9]).toBeGreaterThan(500);
  expect(clear.key).not.toBe(rain.key);
  rain.data[16] = -1;
  expect(compileAuthoredAtmosphere(10, 0.1).data[16]).toBeGreaterThanOrEqual(0);
});
