import { expect, test } from "bun:test";
import { createWaterLookdev } from "@wrela/examples/water-lookdev";
import { waterSchema } from "@wrela/model";
import { compileWaterEffects } from "./water-effects";

test("local sheets have bounded topology, explicit spray and reject ambiguous authoring", () => {
  const source = waterSchema.parse(
    createWaterLookdev("surf").documents.find((d) => d.id === "water-study-surf"),
  );
  const meshes = compileWaterEffects(source);
  expect(meshes).toHaveLength(2);
  expect(compileWaterEffects(source)).toBe(meshes);
  for (const mesh of meshes) {
    expect(mesh.positions.length / 3).toBeLessThan(2000);
    expect(mesh.positions.every(Number.isFinite)).toBe(true);
    expect(Math.max(...mesh.indices)).toBeLessThan(mesh.positions.length / 3);
    expect(mesh.colors!.some((v, i) => i % 3 === 2 && v > 0)).toBe(true);
  }
  expect(
    waterSchema.safeParse({ ...source, effects: [source.effects![0], source.effects![0]] }).success,
  ).toBe(false);
  expect(
    waterSchema.safeParse({ ...source, effects: [{ ...source.effects![0], end: source.effects![0].start }] })
      .success,
  ).toBe(false);
});
