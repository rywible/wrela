import { expect, test } from "bun:test";
import { authoredAtmosphereComposition, compileScatteringAtmosphereTable } from "./atmosphere";
import { EARTH_OZONE, integrateOzoneRay, ozoneDensity } from "./atmosphere-ozone";

function quadrature(height: number, cosine: number, length: number): number {
  const count = 131072,
    radius = 6371 + height;
  const density = (s: number) => {
    const altitude = Math.hypot(s * Math.sqrt(1 - cosine * cosine), radius + s * cosine) - 6371;
    // Independent direct piecewise profile, rather than the analytic integrator's branches.
    return altitude < 10 || altitude > 40 ? 0 : altitude < 25 ? (altitude - 10) / 15 : (40 - altitude) / 15;
  };
  let sum = density(0) + density(length);
  for (let i = 1; i < count; i++) sum += (i % 2 ? 4 : 2) * density((length * i) / count);
  return (sum * length) / (3 * count);
}

test("ozone tent has an exact 15km vertical density column and correct half-column", () => {
  expect([0, 10, 17.5, 25, 32.5, 40, 100].map((h) => ozoneDensity(h))).toEqual([0, 0, 0.5, 1, 0.5, 0, 0]);
  expect(integrateOzoneRay(6371, { height: 0, cosine: 1, length: 100 })).toBeCloseTo(15, 8);
  expect(integrateOzoneRay(6371, { height: 25, cosine: 1, length: 75 })).toBeCloseTo(7.5, 8);
  expect(integrateOzoneRay(6371, { height: 50, cosine: -1, length: 50 })).toBeCloseTo(15, 8);
});

test("analytic ozone columns match independent quadrature across shell crossings and tangent rays", () => {
  for (const [height, cosine, length] of [
    [0, 0, 1000],
    [15, -0.02, 1000],
    [25, 0, 700],
    [50, -0.08, 700],
    [30, 0.9, 60],
    [80, 1, 20],
  ]) {
    const actual = integrateOzoneRay(6371, { height, cosine, length });
    expect(actual).toBeGreaterThanOrEqual(0);
    expect(Math.abs(actual - quadrature(height, cosine, length))).toBeLessThan(0.000002);
  }
  const vertical = integrateOzoneRay(6371, { height: 0, cosine: 1, length: 100 });
  const horizon = integrateOzoneRay(6371, { height: 0, cosine: 0, length: 1000 });
  expect(horizon).toBeGreaterThan(vertical * 8);
  const transmission = EARTH_OZONE.extinction.map((coefficient) => Math.exp(-coefficient * horizon));
  expect(transmission[1]).toBeLessThan(transmission[0]);
  expect(transmission[0]).toBeLessThan(transmission[2]);
});

test("ozone occupies its own versioned table lane and absorption-only metadata", () => {
  const composition = authoredAtmosphereComposition();
  const table = compileScatteringAtmosphereTable(composition, { heightCount: 4, cosineCount: 8 });
  const disabled = compileScatteringAtmosphereTable(
    { ...composition, ozone: undefined },
    { heightCount: 4, cosineCount: 8 },
  );
  if ("status" in table || "status" in disabled) throw Error("Unexpected cancellation");
  expect(table.version).toBe(3);
  expect(table.key).not.toBe(disabled.key);
  expect(table.data.slice(0, 16)).toEqual(disabled.data.slice(0, 16));
  for (let i = 16; i < (4 + 4 * 8) * 4; i += 4) {
    expect(table.data.slice(i, i + 3)).toEqual(disabled.data.slice(i, i + 3));
    expect(disabled.data[i + 3]).toBe(0);
  }
  expect(table.data[19]).toBeGreaterThan(100);
  expect(table.data.slice(-4)).toEqual(new Float32Array([10, 25, 40, 0]));
  expect(table.numericError).toBe("unknown");
  expect(table.interpolationError).toBe("unknown");
});
