import { expect, test } from "bun:test";
import { indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import { cookRadianceSteps, radianceSourceKey, restoreRadianceSteps } from "./radiance-cooked";
import { compileRadianceLightingSteps, radianceReceiverMixture } from "./radiance-lighting";

function finish<T>(steps: Generator<void, T>): T {
  let step = steps.next();
  while (!step.done) step = steps.next();
  return step.value;
}
test("persistent radiance restores identical transport, receivers and visibility with fresh source objects", async () => {
  const surfaces = indirectBoxFixture({ ceiling: true, front: true }).surfaces;
  surfaces.find((s) => s.id === "ceiling")!.material.emission = { color: [1, 0.4, 0.1], intensity: 2 };
  const key = await radianceSourceKey(surfaces, [0, 0, 0], [0, 0, 0], []);
  const product = finish(compileRadianceLightingSteps(surfaces, { center: [0, 0, 0], key: "original" }));
  const cooked = structuredClone(finish(cookRadianceSteps(product, key)));
  expect(cooked.bytes).toBeGreaterThan(cooked.triangles.byteLength);
  const fresh = structuredClone(surfaces);
  const restored = finish(restoreRadianceSteps(cooked, key, fresh, "fresh-gpu-key"));
  expect(restored.field.key).toBe("fresh-gpu-key");
  expect(restored.field.transfer).toEqual(product.field.transfer);
  expect(restored.field.skyVisibility).toEqual(product.field.skyVisibility);
  expect(restored.field.receivers).toEqual(product.field.receivers);
  expect(restored.field.directEmission).toEqual(product.field.directEmission);
  expect(restored.field.receiverEmission).toEqual(product.field.receiverEmission);
  expect(restored.field.surfaceDiffuse).toEqual(product.field.surfaceDiffuse);
  expect(restored.geometry.report.bytes).toBe(product.geometry.report.bytes);
  expect(product.field.surfaceDiffuse?.positions.length).toBeGreaterThan(0);
  expect(product.field.receiverEmission?.some((v) => v > 0)).toBe(true);
  expect(product.field.receiverEmission?.length).toBe((product.field.receivers!.length / 8) * 3);
  for (const [id, mesh] of product.meshes) {
    expect(restored.meshes.get(id)?.radianceProbes).toEqual(mesh.radianceProbes);
    expect(restored.meshes.get(id)?.positions).toEqual(mesh.positions);
    expect(restored.meshes.get(id)?.radianceMixtures).toBe(mesh.radianceMixtures);
  }
  for (let i = 0; i < 30; i++) {
    const position: [number, number, number] = [Math.sin(i) * 1.4, 1, Math.cos(i) * 1.4];
    expect(radianceReceiverMixture(restored.geometry, restored.field.positions, position)).toEqual(
      radianceReceiverMixture(product.geometry, product.field.positions, position),
    );
  }
  const neighbor = finish(
    compileRadianceLightingSteps(fresh, { center: [2, 0, 0], key: "neighbor", reuse: restored }),
  );
  expect(neighbor.field.report.reusedSurfaceSamples).toBeGreaterThan(0);
});

test("persistent source identity is content based, rebase invariant, and rejects geometry, material and receiver edits", async () => {
  const surfaces = indirectBoxFixture().surfaces;
  const key = await radianceSourceKey(surfaces, [0, 0, 0], [0, 0, 0], []);
  const copy = structuredClone(surfaces);
  expect(await radianceSourceKey(copy, [0, 0, 0], [0, 0, 0], [])).toBe(key);
  for (const s of copy) {
    s.matrix[12] -= 32;
    s.matrix[14] += 16;
  }
  expect(await radianceSourceKey(copy, [32, 0, -16], [0, 0, 0], [])).toBe(key);
  copy[0].mesh.materialCoordinates = new Float32Array(copy[0].mesh.positions.length);
  expect(await radianceSourceKey(copy, [32, 0, -16], [0, 0, 0], [])).not.toBe(key);
  const material = structuredClone(surfaces);
  material[0].material.emission = { color: [1, 0, 0], intensity: 2 };
  expect(await radianceSourceKey(material, [0, 0, 0], [0, 0, 0], [])).not.toBe(key);
  const geometry = structuredClone(surfaces);
  geometry[0].mesh.positions = geometry[0].mesh.positions.map((v) => v + 0.1);
  expect(await radianceSourceKey(geometry, [0, 0, 0], [0, 0, 0], [])).not.toBe(key);
});

test("obsolete, mismatched and malformed persisted products are rejected before use", async () => {
  const surfaces = indirectBoxFixture().surfaces;
  const key = await radianceSourceKey(surfaces, [0, 0, 0], [0, 0, 0], []);
  const product = finish(compileRadianceLightingSteps(surfaces, { center: [0, 0, 0], key: "original" }));
  const cooked = finish(cookRadianceSteps(product, key));
  expect(() => finish(restoreRadianceSteps(cooked, "changed", surfaces, "new"))).toThrow();
  const bad = structuredClone(cooked);
  bad.field.transfer[0] = NaN;
  expect(() => finish(restoreRadianceSteps(bad, key, surfaces, "new"))).toThrow();
  const direct = structuredClone(cooked);
  direct.field.directEmission![0] = NaN;
  expect(() => finish(restoreRadianceSteps(direct, key, surfaces, "new"))).toThrow();
  const local = structuredClone(cooked);
  local.field.receiverEmission = new Float32Array(1);
  expect(() => finish(restoreRadianceSteps(local, key, surfaces, "new"))).toThrow();
  const cycle = structuredClone(cooked);
  cycle.nodes[8] = 0;
  expect(() => finish(restoreRadianceSteps(cycle, key, surfaces, "new"))).toThrow();
  const version = { ...cooked, version: 0 } as unknown as typeof cooked;
  expect(() => finish(restoreRadianceSteps(version, key, surfaces, "new"))).toThrow();
  const surface = structuredClone(cooked);
  if (!surface.field.surfaceDiffuse) throw Error("Missing surface transport");
  surface.field.surfaceDiffuse.receivers[0] = surface.field.surfaceDiffuse.positions.length;
  expect(() => finish(restoreRadianceSteps(surface, key, surfaces, "new"))).toThrow();
  const patch = structuredClone(cooked);
  if (!patch.field.surfaceDiffuse?.patches?.length) throw Error("Missing reuse certificates");
  patch.field.surfaceDiffuse.patches[0].offset = 1;
  expect(() => finish(restoreRadianceSteps(patch, key, surfaces, "new"))).toThrow();
});
