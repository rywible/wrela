import { expect, test } from "bun:test";
import { alpinePineLookdevDefinition } from "@wrela/examples";
import { botanicalPreset, thinCoverageBytes, validThinCoverage } from "@wrela/model";
import { botanicalMeshes } from "./botanical-mesh";
import { coniferMeshes } from "./conifer-mesh";
import { deserializeArtifact, serializeArtifact } from "./cooked";
import { coverageMipChain, needleCoverageTile, needleCoverageTriangles } from "./thin-coverage";
import { compileVegetation } from "./vegetation";

const source = { seed: 73, count: 48, width: 0.0032, length: 0.085, shootLength: 0.4, spread: 0.08925 };
const average = (values: Uint8Array) => values.reduce((sum, value) => sum + value, 0) / values.length / 255;

test("coverage mips retain area without accumulating quantization opacity", () => {
  const base = Uint8Array.from({ length: 128 * 256 }, (_, index) => (index % 23 === 0 ? 17 : 0));
  const levels = coverageMipChain(base, 128, 256);
  expect(levels.at(-1)?.length).toBe(1);
  for (const level of levels) expect(Math.abs(average(level) - average(base))).toBeLessThanOrEqual(0.5 / 255);
});

test("semantic tapered needles preserve actual perpendicular width and deterministic damage", () => {
  const triangles = needleCoverageTriangles({ ...source, count: 1, loss: 0 });
  const [a, b, c] = triangles[0].map(([u, v]) => [u * source.spread * 2, v * source.shootLength]);
  const base = [b[0] - a[0], b[1] - a[1]],
    axis = [c[0] - (a[0] + b[0]) * 0.5, c[1] - (a[1] + b[1]) * 0.5];
  expect(Math.hypot(...base)).toBeCloseTo(source.width, 10);
  expect(base[0] * axis[0] + base[1] * axis[1]).toBeCloseTo(0, 10);
  const tile = needleCoverageTile(source);
  expect(needleCoverageTile(source)).toEqual(tile);
  expect(average(tile.levels[0])).toBeGreaterThan(0.05);
  expect(
    needleCoverageTile({ ...source, loss: 1 }).levels.every((level) => level.every((value) => value === 0)),
  ).toBe(true);
  expect(average(needleCoverageTile({ ...source, count: 24 }).levels[0])).toBeLessThan(
    average(tile.levels[0]),
  );
});

test("filtered conifer retains shape controls and coverage field through distant realization and cooking", () => {
  const plant = alpinePineLookdevDefinition();
  plant.branches = 5;
  const near = coniferMeshes(plant, "review"),
    far = coniferMeshes(plant, "review", true);
  const coverage = near.foliage.thinCoverage;
  if (!coverage) throw Error("Missing needle coverage");
  expect(validThinCoverage(coverage, near.foliage.positions.length / 3)).toBe(true);
  expect(thinCoverageBytes(coverage)).toBe(
    coverage.uv.byteLength + coverage.levels.reduce((sum, level) => sum + level.byteLength, 0),
  );
  expect(far.foliage.thinCoverage?.key).toBe(coverage.key);
  expect(far.foliage.indices.length).toBeLessThan(near.foliage.indices.length);
  expect(near.leafCount).toBe(far.leafCount);
  expect(near.foliage.indices.length).toBeLessThan(
    coniferMeshes(plant, "review", false, "triangles").foliage.indices.length / 5,
  );
  const serialized = serializeArtifact(compileVegetation(plant));
  expect(serializeArtifact(deserializeArtifact(serialized))).toEqual(serialized);
  const malformed = structuredClone(serialized) as {
    surfaces: { mesh: { thinCoverage?: { levels: number[][] } } }[];
  };
  const corruptCoverage = malformed.surfaces[1].mesh.thinCoverage;
  if (!corruptCoverage) throw Error("Missing cooked coverage");
  corruptCoverage.levels.pop();
  expect(() => deserializeArtifact(malformed)).toThrow();
});

test("coverage contract rejects truncated chains, dimensions and nonfinite UVs", () => {
  const coverage = { ...needleCoverageTile(source), uv: new Float32Array([0, 0, 1, 0, 1, 1]) };
  expect(validThinCoverage(coverage, 3)).toBe(true);
  expect(validThinCoverage({ ...coverage, width: 127 }, 3)).toBe(false);
  expect(validThinCoverage({ ...coverage, uv: new Float32Array([NaN, 0]) }, 1)).toBe(false);
  expect(validThinCoverage({ ...coverage, levels: coverage.levels.slice(0, -1) }, 3)).toBe(false);
});

test("held-out grass, fern and oak retain leaf populations and scalar area through distance changes", () => {
  for (const species of ["grass", "fern", "oak"] as const) {
    const plant = {
      ...alpinePineLookdevDefinition(),
      seed: 119,
      height: 2,
      radius: 1,
      branches: 4,
      botanical: botanicalPreset(species),
    };
    const near = botanicalMeshes(plant, "review"),
      far = botanicalMeshes(plant, "review", true);
    const nearCoverage = near.foliage.thinCoverage,
      farCoverage = far.foliage.thinCoverage;
    if (!nearCoverage || !farCoverage) throw Error("Missing leaf coverage");
    expect(near.leafCount).toBe(far.leafCount);
    expect(near.foliage.thinCoverage?.key).toBe(far.foliage.thinCoverage?.key);
    expect(nearCoverage.levels).toEqual(farCoverage.levels);
    expect(validThinCoverage(nearCoverage, near.foliage.positions.length / 3)).toBe(true);
    expect(validThinCoverage(farCoverage, far.foliage.positions.length / 3)).toBe(true);
    expect(far.foliage.indices.length).toBeLessThan(near.foliage.indices.length);
    expect(near.foliage.bounds.min[0]).toBeCloseTo(far.foliage.bounds.min[0], 5);
    expect(near.foliage.bounds.max[0]).toBeCloseTo(far.foliage.bounds.max[0], 5);
  }
});
