import { expect, test } from "bun:test";
import {
  authoredAtmosphereComposition,
  compileAtmosphereTable,
  compileScatteringAtmosphereTable,
  integrateAtmosphereRay,
  MAX_ATMOSPHERE_TABLE_BYTES,
} from "./atmosphere";

const air = { planetRadius: 6371, topHeight: 100, scaleHeight: 8, extinction: 0.02 };
function reference(height: number, cosine: number, length: number, scale: number) {
  const steps = 32768,
    r = air.planetRadius + height;
  const density = (s: number) =>
    Math.exp(-(Math.hypot(Math.sqrt(1 - cosine * cosine) * s, r + cosine * s) - air.planetRadius) / scale);
  let sum = density(0) + density(length);
  for (let i = 1; i < steps; i++) sum += (i % 2 ? 4 : 2) * density((length * i) / steps);
  return (sum * length) / (3 * steps);
}
test("atmosphere intervals bracket independent integration for horizon, zenith, sunset and high altitude", () => {
  for (const scaleHeight of [1.2, 8, 20])
    for (const ray of [
      { height: 0, cosine: 1, length: 100 },
      { height: 0, cosine: 0, length: 500 },
      { height: 20, cosine: -0.02, length: 400 },
      { height: 80, cosine: 0.05, length: 100 },
    ]) {
      const integral = integrateAtmosphereRay({ ...air, scaleHeight }, ray);
      const truth = reference(ray.height, ray.cosine, ray.length, scaleHeight);
      expect(integral.lower).toBeLessThanOrEqual(truth + 1e-9);
      expect(integral.upper).toBeGreaterThanOrEqual(truth - 1e-9);
      expect(integral.status).toBe("ready");
      expect(integral.transmissionUpper - integral.transmissionLower).toBeLessThanOrEqual(1e-5);
      expect(integral.numericError).toBe("unknown");
    }
  expect(integrateAtmosphereRay(air, { height: 0, cosine: 1, length: 100 }).segmentCount).toBe(1);
});
test("atmosphere exhaustion, cancellation, ground crossing and allocation caps are explicit", () => {
  const thin = integrateAtmosphereRay(
    { ...air, scaleHeight: 0.0001 },
    { height: 0, cosine: 0, length: 500 },
    { maxSegments: 1 },
  );
  expect(
    [thin.lower, thin.upper, thin.transmissionLower, thin.transmissionUpper].every(Number.isFinite),
  ).toBe(true);
  expect(thin.status).toBe("exhausted");
  expect(integrateAtmosphereRay(air, { height: 0, cosine: 0, length: 500 }, { maxSegments: 1 }).status).toBe(
    "exhausted",
  );
  expect(
    integrateAtmosphereRay(air, { height: 0, cosine: 0, length: 500 }, { cancelled: () => true }).status,
  ).toBe("cancelled");
  expect(() => integrateAtmosphereRay(air, { height: 0, cosine: -0.1, length: 100 })).toThrow("planet");
  expect(() => integrateAtmosphereRay(air, { height: 0, cosine: 2, length: 100 })).toThrow();
  expect(() => compileAtmosphereTable(air, { heightCount: 100000 })).toThrow();
  expect(() => compileAtmosphereTable(air, { maxBytes: 16 })).toThrow("byte budget");
  expect(() => compileAtmosphereTable({ ...air, scaleHeight: 0 })).toThrow();
  expect(compileAtmosphereTable(air, { cancelled: () => true })).toEqual({ status: "cancelled" });
});
test("atmosphere table owns bounded finite storage, source identities and honest interpolation evidence", () => {
  const options = { heightCount: 8, cosineCount: 16 };
  const table = compileAtmosphereTable(air, options);
  const changed = compileAtmosphereTable({ ...air, scaleHeight: 7 }, options);
  const resolution = compileAtmosphereTable(air, { ...options, heightCount: 9 });
  if (table.status === "cancelled" || changed.status === "cancelled" || resolution.status === "cancelled")
    throw new Error("Unexpected cancellation");
  expect(table.data.every(Number.isFinite)).toBe(true);
  expect(table.byteLength).toBe(table.data.byteLength);
  expect(table.byteLength).toBeLessThanOrEqual(MAX_ATMOSPHERE_TABLE_BYTES);
  expect(table.data[4]).toBe(8);
  expect(table.data[5]).toBe(16);
  expect(table.data[11]).toBe(2); // Ground-level downward ray is planet-occluded.
  expect(table.numericError).toBe("unknown");
  expect(table.interpolationError).toBe("unknown");
  expect(changed.sourceKey).not.toBe(table.sourceKey);
  expect(resolution.sourceKey).toBe(table.sourceKey);
  expect(resolution.key).not.toBe(table.key);
});

test("physical atmosphere table resolves exponential constituents, ozone and the horizon domain", () => {
  const composition = authoredAtmosphereComposition();
  const table = compileScatteringAtmosphereTable(composition, { transmissionBudget: 1e-4, maxSegments: 128 });
  if ("status" in table) throw new Error("Unexpected cancellation");
  expect(table.data[14]).toBe(3);
  expect(table.data.every(Number.isFinite)).toBe(true);
  expect(table.byteLength).toBe(49248);
  expect(table.maximumTransmissionWidth).toBeLessThan(0.0002);
  const changed = compileScatteringAtmosphereTable(authoredAtmosphereComposition(4, 0.002), {
    heightCount: 4,
    cosineCount: 4,
  });
  if ("status" in changed) throw new Error("Unexpected cancellation");
  expect(changed.key).not.toBe(table.key);
  let maximumTransmissionError = 0;
  for (const height of [0.002, 0.02, 0.1, 1, 10, 80])
    for (const cosine of [0, 0.01, 0.1, 0.7, 1]) {
      const r = composition.planetRadius + height;
      const horizon = -Math.sqrt(height * (2 * composition.planetRadius + height)) / r;
      const x = Math.sqrt((cosine - horizon) / (1 - horizon)) * 63;
      const y = (height / composition.topHeight) ** 0.25 * 47;
      const x0 = Math.min(62, Math.floor(x)),
        y0 = Math.min(46, Math.floor(y));
      const fx = x - x0,
        fy = y - y0;
      const interpolate = (k: number) => {
        const value = (xx: number, yy: number) => table.data[(4 + yy * 64 + xx) * 4 + k];
        return (
          (value(x0, y0) * (1 - fx) + value(x0 + 1, y0) * fx) * (1 - fy) +
          (value(x0, y0 + 1) * (1 - fx) + value(x0 + 1, y0 + 1) * fx) * fy
        );
      };
      const delta =
        (composition.topHeight - height) * (2 * composition.planetRadius + composition.topHeight + height);
      const length = delta / (Math.sqrt(r * r * cosine * cosine + delta) + r * cosine);
      const truth = [
        composition.rayleighScaleHeight,
        composition.aerosolScaleHeight,
        composition.mistScaleHeight,
      ].map((scale) => reference(height, cosine, length, scale));
      for (const betaRayleigh of composition.rayleighExtinction) {
        const expected = Math.exp(
          -betaRayleigh * truth[0] -
            composition.aerosolExtinction * truth[1] -
            composition.mistExtinction * truth[2],
        );
        const actual = Math.exp(
          -betaRayleigh * interpolate(0) -
            composition.aerosolExtinction * interpolate(1) -
            composition.mistExtinction * interpolate(2),
        );
        maximumTransmissionError = Math.max(maximumTransmissionError, Math.abs(actual - expected));
      }
    }
  // Observed fixture tolerance, not a global numeric/interpolation certificate.
  expect(maximumTransmissionError).toBeLessThan(0.02);
  expect(() => compileScatteringAtmosphereTable({ ...composition, anisotropy: 1 })).toThrow();
  expect(compileScatteringAtmosphereTable(composition, { cancelled: () => true })).toEqual({
    status: "cancelled",
  });
});

test("authored valley mist preserves local extinction without creating a deep fog bank", () => {
  const composition = authoredAtmosphereComposition(3, 0.003);
  const datumHeight = 0.02;
  expect(composition.mistExtinction * Math.exp(-datumHeight / composition.mistScaleHeight)).toBeCloseTo(
    3,
    12,
  );
  const column = integrateAtmosphereRay(
    {
      planetRadius: composition.planetRadius,
      topHeight: composition.topHeight,
      scaleHeight: composition.mistScaleHeight,
      extinction: composition.mistExtinction,
    },
    { height: datumHeight, cosine: 0.2, length: 10 },
  );
  const opticalDepth = (column.lower + column.upper) * 0.5 * composition.mistExtinction;
  // Independent local-plane exponential column; spherical curvature is tiny
  // over this shallow layer. Preserve the source units at world Y=0.
  expect(opticalDepth).toBeCloseTo((3 * 0.012) / 0.2, 3);
  expect(column.transmissionLower).toBeGreaterThan(0.83);
});
