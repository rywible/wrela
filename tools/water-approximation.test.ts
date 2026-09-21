import { expect, test } from "bun:test";
import { queryWater } from "@wrela/compiler";
import { waterGeometryAttenuation, waterGeometryErrorBound } from "@wrela/model";

test("band-limited water triangles stay within their declared analytic height error", () => {
  const water = {
    id: "water",
    kind: "water" as const,
    name: "Water",
    schemaVersion: 1 as const,
    dependencies: [],
    level: 0,
    color: [0, 0, 0] as [number, number, number],
    roughness: 0.2,
    waves: [
      { amplitude: 0.12, wavelength: 8, speed: 1.2, direction: 0.4, phase: 0 },
      { amplitude: 0.06, wavelength: 3.2, speed: 0.8, direction: 1.7, phase: 1 },
    ],
  };
  const spacing = 2.5,
    bound = waterGeometryErrorBound(water.waves, spacing);
  const filtered = {
    ...water,
    waves: water.waves.map((w) => ({
      ...w,
      amplitude: w.amplitude * waterGeometryAttenuation(w.wavelength, spacing),
    })),
  };
  expect(filtered.waves[1].amplitude).toBe(0);
  let maxError = 0;
  for (let sample = 0; sample < 2000; sample++) {
    const x0 = ((sample % 17) - 8) * spacing,
      z0 = ((Math.floor(sample / 17) % 17) - 8) * spacing,
      time = sample * 0.17,
      u = (sample % 11) / 10,
      v = (sample % 13) / 12;
    const a = queryWater(filtered, x0, z0, time).height,
      b = queryWater(filtered, x0 + spacing, z0, time).height,
      c = queryWater(filtered, x0, z0 + spacing, time).height,
      d = queryWater(filtered, x0 + spacing, z0 + spacing, time).height;
    const triangle =
      u + v <= 1 ? a * (1 - u - v) + b * u + c * v : d * (u + v - 1) + b * (1 - v) + c * (1 - u);
    maxError = Math.max(
      maxError,
      Math.abs(triangle - queryWater(water, x0 + u * spacing, z0 + v * spacing, time).height),
    );
  }
  expect(maxError).toBeLessThanOrEqual(bound);
  expect(bound).toBeLessThan(0.18);
});
