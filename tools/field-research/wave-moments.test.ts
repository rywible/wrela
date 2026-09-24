import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { compileWaves, sampledMoments, waveMoments } from "./wave-moments";

test("finite wave moments agree with independent spatial/shutter integration", () => {
  const water = referenceProject().documents.find((d) => d.kind === "water");
  if (!water || water.kind !== "water") throw new Error("Missing water");
  const waves = compileWaves(water.waves);
  const footprint = { dx: [3.1, 0.7] as [number, number], dy: [-0.1, 2.2] as [number, number], shutter: 0.2 };
  for (const t of [0, 0.13, 7.31]) {
    const exact = waveMoments(waves, 2.1, -4.7, t, footprint),
      numerical = sampledMoments(waves, 2.1, -4.7, t, footprint, 256, 32);
    for (let i = 0; i < 2; i++) expect(Math.abs(exact.mean[i] - numerical.mean[i])).toBeLessThan(1e-6);
    for (let i = 0; i < 3; i++)
      expect(Math.abs(exact.covariance[i] - numerical.covariance[i])).toBeLessThan(1e-6);
    expect(exact.covariance[0] * exact.covariance[2] - exact.covariance[1] ** 2).toBeGreaterThanOrEqual(
      -1e-14,
    );
  }
});
test("coherent cancellation survives filtering; an incoherent variance sum would invent roughness", () => {
  const a = { amplitude: 0.2, wavelength: 1.5, speed: 0.7, direction: 0.2, phase: 0 };
  const waves = compileWaves([a, { ...a, phase: Math.PI }]);
  const moments = waveMoments(waves, 3, 1, 0.3, { dx: [5, 0], dy: [0, 4], shutter: 0.2 });
  for (const value of [...moments.mean, ...moments.covariance]) expect(Math.abs(value)).toBeLessThan(1e-14);
});
test("point footprint gives the authored slope and zero covariance", () => {
  const waves = compileWaves([{ amplitude: 0.12, wavelength: 8, speed: 1.2, direction: 0.4, phase: 0 }]);
  const moments = waveMoments(waves, 0, 0, 0, { dx: [0, 0], dy: [0, 0], shutter: 0 });
  expect(moments.mean[0]).toBeCloseTo(((0.12 * Math.PI) / 4) * Math.cos(0.4), 12);
  for (const value of moments.covariance) expect(Math.abs(value)).toBeLessThan(1e-14);
});
