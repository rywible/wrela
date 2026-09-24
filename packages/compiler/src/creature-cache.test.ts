import { expect, test } from "bun:test";
import { createCreatureFixture } from "@wrela/examples/creature-fixtures";
import { type CharacterDefinition, type CompiledCharacter, contentKey } from "@wrela/model";
import {
  clearCreatureCompilerCache,
  creatureCompilerCacheMetrics,
  creaturePreparationCacheMetrics,
} from "./creature";
import { cachedCreatureProduct } from "./creature-cache";
import { artifactTransfers, compileDocument } from "./index";
import { compilerCacheMetrics } from "./surface";

function source(): CharacterDefinition {
  const document = createCreatureFixture("ash-warden").project.documents.find(
    (item) => item.id === "ash-warden",
  );
  if (!document || document.kind !== "character") throw new Error("Missing fixture");
  return document;
}
function compile(document: CharacterDefinition): CompiledCharacter {
  const product = compileDocument(document, "interactive");
  if (!product || product.kind !== "character") throw new Error("Missing character");
  return product;
}
const builds = () =>
  Object.fromEntries(
    Object.entries(creaturePreparationCacheMetrics()).map(([kind, metrics]) => [kind, metrics.builds]),
  );

test("motion-only edits reuse every preparation stage and publish current motion/source revision", () => {
  clearCreatureCompilerCache();
  const document = source(),
    first = compile(document),
    before = builds(),
    chart = creatureCompilerCacheMetrics(),
    field = compilerCacheMetrics().geometry;
  document.motions[0].keys[0].rotation[0] += 0.03;
  const second = compile(document);
  expect(builds()).toEqual(before);
  expect(creatureCompilerCacheMetrics().misses).toBe(chart.misses);
  expect(compilerCacheMetrics().geometry.misses).toBe(field.misses);
  expect(second.key).not.toBe(first.key);
  expect(second.creatureSourceKey).not.toBe(first.creatureSourceKey);
  expect(second.motions[0].keys[0].rotation[0]).toBe(document.motions[0].keys[0].rotation[0]);
  expect(second.mesh.positions).toEqual(first.mesh.positions);
  expect(second.weights).toEqual(first.weights);
});

test("material-only edits update appearance without binding, corrective, feature, or groom work", () => {
  clearCreatureCompilerCache();
  const document = source(),
    first = compile(document),
    before = builds(),
    chart = creatureCompilerCacheMetrics();
  if (!document.creature) throw new Error("Missing anatomy");
  document.creature.appearance[0].color = [0.42, 0.31, 0.27];
  document.creature.appearance[0].roughness = 0.54;
  const second = compile(document),
    after = builds();
  for (const kind of ["attachments", "features", "body", "groom", "binding", "correctives", "anchors"])
    expect(after[kind]).toBe(before[kind]);
  expect(after.appearance).toBe(before.appearance + 1);
  expect(creatureCompilerCacheMetrics().misses).toBe(chart.misses);
  expect(second.mesh.positions).toEqual(first.mesh.positions);
  expect(second.mesh.colors).not.toEqual(first.mesh.colors);
  expect(second.weights).toEqual(first.weights);
  expect(second.creatureGroom?.key).toBe(first.creatureGroom?.key);
});

test("changing chart or primitive material reassigns geometry without tessellation or rebinding", () => {
  clearCreatureCompilerCache();
  const document = source();
  compile(document);
  const before = builds(),
    chart = creatureCompilerCacheMetrics(),
    field = compilerCacheMetrics().geometry;
  if (!document.creature) throw new Error("Missing anatomy");
  document.creature.charts[0].material = "old-ivory";
  const eye = document.field.nodes.find((node) => node.id === "eye-left");
  if (!eye) throw new Error("Missing eye");
  eye.material = "old-ivory";
  const next = compile(document),
    after = builds();
  expect(after.features).toBe(before.features);
  expect(after.binding).toBe(before.binding);
  expect(after.correctives).toBe(before.correctives);
  expect(creatureCompilerCacheMetrics().misses).toBe(chart.misses);
  expect(compilerCacheMetrics().geometry.misses).toBe(field.misses);
  const index = next.mesh.indices.findIndex((vertex) => next.mesh.sourceIds?.[vertex] === "eye-left"),
    group = next.mesh.materialGroups?.find((item) => index >= item.start && index < item.start + item.count);
  expect(group?.material).toBe("old-ivory");
});

test("growing a mane rebuilds groom and variants while retaining body, chart, binding and root correctives", () => {
  clearCreatureCompilerCache();
  const document = source(),
    first = compile(document),
    before = builds(),
    chart = creatureCompilerCacheMetrics(),
    field = compilerCacheMetrics().geometry;
  if (!document.creature) throw new Error("Missing anatomy");
  document.creature.grooms[0].length *= 1.4;
  const second = compile(document),
    after = builds();
  for (const kind of ["attachments", "features", "body", "appearance", "binding", "correctives", "anchors"])
    expect(after[kind]).toBe(before[kind]);
  expect(after.groom).toBe(before.groom + 1);
  expect(after.assembly).toBeGreaterThan(before.assembly);
  expect(creatureCompilerCacheMetrics().misses).toBe(chart.misses);
  expect(compilerCacheMetrics().geometry.misses).toBe(field.misses);
  const prefix = (first.creatureBodyVertexCount ?? 0) * 3;
  expect(second.mesh.positions.slice(0, prefix)).toEqual(first.mesh.positions.slice(0, prefix));
  expect(second.mesh.positions.slice(prefix)).not.toEqual(first.mesh.positions.slice(prefix));
});

test("public artifact mutation cannot corrupt any cached preparation product", () => {
  clearCreatureCompilerCache();
  const document = source(),
    first = compile(document),
    position = first.mesh.positions[0],
    weight = first.weights[0],
    groomRoot = first.creatureGroom?.guides[0].points[0][0];
  first.mesh.positions[0] = 999;
  first.weights[0] = -99;
  if (first.creatureGroom) first.creatureGroom.guides[0].points[0][0] = 999;
  const coordinate = first.creatureCoordinates?.find((item) => item !== null);
  if (coordinate) coordinate.coordinates[0] = 999;
  structuredClone(first, { transfer: artifactTransfers(first) });
  expect(first.mesh.positions.byteLength).toBe(0);
  const second = compile(document);
  expect(second.mesh.positions[0]).toBe(position);
  expect(second.weights[0]).toBe(weight);
  expect(second.creatureGroom?.guides[0].points[0][0]).toBe(groomRoot);
  expect(
    second.creatureCoordinates?.filter((item) => item !== null).every((item) => item.coordinates[0] !== 999),
  ).toBe(true);
});

test("cached and cold preparation agree after appearance, shape, and influence edits", () => {
  clearCreatureCompilerCache();
  const document = source();
  compile(document);
  if (!document.creature) throw new Error("Missing anatomy");
  document.creature.appearance[0].color = [0.3, 0.2, 0.1];
  document.creature.grooms[0].length *= 1.1;
  document.creature.regions[0].frame.position[0] += 0.02;
  document.joints[1].radius *= 1.1;
  const warm = compile(document),
    expected = contentKey(warm);
  clearCreatureCompilerCache();
  const cold = compile(document);
  expect(contentKey(cold)).toBe(expected);
});

test("preparation caches enforce both byte and entry budgets, including oversized products", () => {
  clearCreatureCompilerCache();
  for (let index = 0; index < 30; index++)
    cachedCreatureProduct("binding", String(index), () => new Uint8Array(64));
  const metrics = creaturePreparationCacheMetrics().binding;
  expect(metrics.entries).toBe(metrics.maxEntries);
  expect(metrics.bytes).toBeLessThanOrEqual(metrics.maxBytes);
  cachedCreatureProduct("binding", "0", () => new Uint8Array(64));
  expect(creaturePreparationCacheMetrics().binding.builds).toBe(31);
  const entries = creaturePreparationCacheMetrics().binding.entries;
  cachedCreatureProduct("binding", "oversized", () => new Uint8Array(metrics.maxBytes + 1));
  expect(creaturePreparationCacheMetrics().binding.entries).toBe(entries);
  expect(creaturePreparationCacheMetrics().binding.bytes).toBeLessThanOrEqual(metrics.maxBytes);
});
