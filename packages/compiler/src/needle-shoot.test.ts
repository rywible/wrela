import { expect, test } from "bun:test";
import { shapedPineLookdevDefinition } from "@wrela/examples";
import { validThinCoverage } from "@wrela/model";
import { coniferMeshes, coniferShootRecipe } from "./conifer-mesh";
import { compileNeedleShoot } from "./needle-shoot";
import { compileShootCoverage } from "./shoot-coverage";

test("paired source needles share attachments; pruning and exact replay preserve source products", () => {
  const doc = shapedPineLookdevDefinition(),
    recipe = { ...coniferShootRecipe(doc), loss: 0 };
  const shoot = compileNeedleShoot(recipe, 2);
  expect(shoot.needles.length).toBe(recipe.count);
  for (let i = 0; i < shoot.needles.length; i += 2) {
    expect(shoot.needles[i].centers[0]).toEqual(shoot.needles[i + 1].centers[0]);
    expect(shoot.needles[i].centers[3]).not.toEqual(shoot.needles[i + 1].centers[3]);
  }
  expect(compileNeedleShoot(structuredClone(recipe), 2)).toBe(shoot);
  expect(shoot.mesh.indices.length / 3).toBe(recipe.count * 15);
  expect(shoot.mesh.positions.every(Number.isFinite)).toBe(true);
  expect(compileNeedleShoot({ ...recipe, loss: 1 }, 2).needles).toEqual([]);
  expect(compileNeedleShoot(recipe, 2, 1).needles.length).toBeLessThan(shoot.needles.length);
  expect(() => compileNeedleShoot({ ...recipe, count: Infinity }, 0)).toThrow();
});

test("derived coverage uses every retained reference needle exactly once and preserves filtered area", () => {
  const source = compileShootCoverage(coniferShootRecipe(shapedPineLookdevDefinition()));
  expect(source.coverage.layers).toBe(source.shoots.length * 3);
  expect(
    validThinCoverage({ ...source.coverage, uv: new Float32Array([0, 0]), layer: new Uint16Array([0]) }, 1),
  ).toBe(true);
  for (const shoot of source.shoots) {
    expect(shoot.needleRanges.reduce((n, r) => n + r.count, 0)).toBe(shoot.mesh.indices.length);
    expect(shoot.needleRanges.every((r) => r.plane >= 0 && r.plane < 3)).toBe(true);
  }
  const average = (level: Uint8Array, layer: number) => {
    const texels = level.length / 4 / (source.coverage.layers ?? 1);
    let sum = 0;
    for (let i = 0; i < texels; i++) sum += level[(layer * texels + i) * 4] / 255;
    return sum / texels;
  };
  for (let layer = 0; layer < (source.coverage.layers ?? 1); layer++) {
    expect(average(source.coverage.levels[0], layer)).toBeGreaterThan(0.015);
    for (const mip of source.coverage.levels)
      expect(Math.abs(average(mip, layer) - average(source.coverage.levels[0], layer))).toBeLessThanOrEqual(
        0.5 / 255 + 1e-6,
      );
  }
});

test("bounded branch reference and proxy retain the same needles without whole-tree truncation", () => {
  const doc = shapedPineLookdevDefinition();
  const reference = coniferMeshes(doc, "review", false, "triangles", "b16");
  const proxy = coniferMeshes(doc, "review", false, "filtered", "b16");
  expect(reference.truncated).toBe(false);
  expect(proxy.truncated).toBe(false);
  expect(proxy.leafCount).toBe(reference.leafCount);
  expect(reference.leafCount).toBeGreaterThan(100);
  expect(proxy.foliage.indices.length).toBeLessThan(reference.foliage.indices.length / 10);
  expect(proxy.foliage.sourceIds?.every((id) => id.startsWith(`${doc.id}/b16/`))).toBe(true);
  const field = proxy.foliage.thinCoverage;
  expect(field?.format).toBe("coverage-normal");
  expect(field && validThinCoverage(field, proxy.foliage.positions.length / 3)).toBe(true);
  doc.botanical!.pruning.removedBranches = ["b12"];
  expect(coniferMeshes(doc, "review", false, "filtered", "b16").foliage).toEqual(proxy.foliage);
});

test("authored cohort contrast changes shoot color without changing geometry or retained organs", () => {
  const recipe = { length: 0.25, needleLength: 0.065, needleWidth: 0.003, count: 144, loss: 0.1 };
  const young = compileNeedleShoot(recipe, 0, 0),
    old = compileNeedleShoot(recipe, 0, 1);
  const contrasted = compileNeedleShoot({ ...recipe, cohortContrast: 0.8 }, 0, 0);
  const contrastedOld = compileNeedleShoot({ ...recipe, cohortContrast: 0.8 }, 0, 1);
  expect(contrasted.mesh.positions).toEqual(young.mesh.positions);
  expect(contrasted.mesh.indices).toEqual(young.mesh.indices);
  expect(contrasted.needles.map((n) => n.id)).toEqual(young.needles.map((n) => n.id));
  expect(contrasted.mesh.colors![0]).toBeGreaterThan(young.mesh.colors![0]);
  expect(contrastedOld.mesh.colors![0]).toBeLessThan(old.mesh.colors![0]);
  expect(() => compileNeedleShoot({ ...recipe, cohortContrast: 2 }, 0, 0)).toThrow();
});
