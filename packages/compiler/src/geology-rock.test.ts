import { expect, test } from "bun:test";
import { createGeologyAuthoringStudy, createTerrainLookdev } from "@wrela/examples";
import { buildGeologicalRockField, documentSchema } from "@wrela/model";
import { compileField } from "./field";
import { buildGeologyObjects, reviewGeology } from "./geology";
import { extractSurface } from "./surface";

test("geological bedrock keeps a connected protected core across seeds and inclined bedding", () => {
  for (const seed of [1, 491, 1709, 8317, 65535])
    for (const dip of [-0.48, 0.12, 0.48]) {
      const field = buildGeologicalRockField(
        [12, 8, 10],
        { seed, layers: 6, fracture: 0.9, bedding: { dip, strike: 1.05 }, fractureScale: 1.4 },
        48,
      );
      const compiled = compileField(field);
      expect(field.nodes.length).toBeLessThan(30);
      // Intact core is shared by each bedding horizon. Local surface scars cannot
      // leave floating layers or interior holes; source dimensions remain exact.
      for (let y = 0.2; y < 5.5; y += 0.1)
        for (const x of [-2, 0, 2]) expect(compiled.distance([x, y, 0])).toBeLessThan(0);
      for (const point of [
        [6.1, 3, 0],
        [-6.1, 3, 0],
        [0, 8.1, 0],
        [0, -0.1, 0],
        [0, 3, 5.1],
      ] as [number, number, number][])
        expect(compiled.distance(point)).toBeGreaterThan(0);
    }
});

test("held-out geological sources preserve player-width passages and collision meshes", () => {
  for (const variant of ["alpine", "cross-bedded", "confined"] as const) {
    const terrain = createGeologyAuthoringStudy(variant);
    expect(documentSchema.safeParse(terrain).success).toBe(true);
    const review = reviewGeology(terrain);
    expect(review?.collisionSamples).toBe(0);
    expect(review?.steepSamples).toBe(0);
    expect(review?.sightlineClear).toBe(true);
    for (const object of buildGeologyObjects(terrain)) {
      expect(documentSchema.safeParse(object).success).toBe(true);
      expect(object.collision).toBe("mesh");
      const result = extractSurface(object.field, "interactive");
      expect(result.mesh.indices.length).toBeGreaterThan(0);
      expect([...result.mesh.positions].every(Number.isFinite)).toBe(true);
      expect([...result.mesh.normals].every(Number.isFinite)).toBe(true);
    }
  }
});

test("alpine terrain uses authored banks, a continuous protected route and the reusable rock construction", () => {
  const recipe = createTerrainLookdev();
  for (const document of recipe.documents) expect(documentSchema.safeParse(document).success).toBe(true);
  const terrain = recipe.documents.find((document) => document.kind === "terrain");
  if (terrain?.kind !== "terrain") throw Error("Missing alpine terrain");
  expect(terrain.geology?.landforms.find((landform) => landform.id === "creek-bed")?.profile?.kind).toBe(
    "river",
  );
  const review = reviewGeology(terrain);
  expect(review?.collisionSamples).toBe(0);
  expect(review?.steepSamples).toBe(0);
  expect(terrain.geology?.corridors?.[0].points.length).toBe(4);
});

test("weathering cannot remove the wall immediately around a cave opening", () => {
  for (const variant of ["alpine", "cross-bedded", "confined"] as const) {
    const terrain = createGeologyAuthoringStudy(variant);
    const formation = terrain.geology?.formations[0];
    if (!formation?.rock || !terrain.geology) throw Error("Expected stratified cave");
    formation.heading = 0;
    formation.rock.fracture = 1;
    const object = buildGeologyObjects(terrain)[0];
    const weathered = compileField(object.field);
    const opening = compileField({ ...object.field, root: "opening" });
    formation.rock.fracture = 0;
    const intact = compileField(buildGeologyObjects(terrain)[0].field);
    const spacing = Math.max(...formation.size) / formation.resolution;
    let checked = 0;
    for (let x = -formation.size[0] / 2; x <= formation.size[0] / 2; x += spacing)
      for (let y = 0.1; y <= formation.size[1] * 0.8; y += spacing)
        for (const z of [
          -formation.size[2] * 0.48,
          -formation.size[2] * 0.42,
          0,
          formation.size[2] * 0.42,
          formation.size[2] * 0.48,
        ]) {
          const point: [number, number, number] = [x, y, z];
          const cavityDistance = opening.distance(point);
          if (intact.distance(point) < -0.03 && cavityDistance > 0.03 && cavityDistance < spacing * 2) {
            expect(weathered.distance(point)).toBeLessThan(0);
            checked++;
          }
        }
    expect(checked).toBeGreaterThan(100);
  }
});
