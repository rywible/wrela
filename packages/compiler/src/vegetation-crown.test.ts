import { expect, test } from "bun:test";
import { type CompiledVegetation, type MeshData, thinCoverageBytes, validThinCoverage } from "@wrela/model";

import { deserializeArtifact, serializeArtifact } from "./cooked";
import { compileVegetationCrown, crownMipChain } from "./vegetation-crown";

const plane: MeshData = {
  positions: new Float32Array([-1, -1, 0, 1, -1, 0, -1, 1, 0, 1, 1, 0]),
  normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1]),
  indices: new Uint32Array([0, 1, 2, 1, 3, 2]),
  sourceIds: Array(4).fill("tree/trunk.1/0.2/c0/l0"),
  bounds: { min: [-1, -1, 0], max: [1, 1, 0] },
};
test("crown products conserve mip coverage, retain organ ownership and reject malformed cooked views", () => {
  const crown = compileVegetationCrown(plane, "tree-source", 0.3);
  if (!crown) throw Error("Expected crown");
  const field = crown.mesh.thinCoverage;
  if (!field) throw Error("Expected coverage");
  expect(validThinCoverage(field, crown.mesh.positions.length / 3)).toBe(true);
  expect(crown.product.qualification.status).toBe("candidate");
  expect(crown.product.sourceOrgans).toEqual(["tree/trunk.1/0.2"]);
  expect(crown.product.views).toHaveLength(64);
  expect(crown.product.byteLength).toBe(
    crown.mesh.positions.byteLength +
      crown.mesh.normals.byteLength +
      crown.mesh.indices.byteLength +
      (crown.mesh.colors?.byteLength ?? 0) +
      thinCoverageBytes(field),
  );
  const surface = {
    kind: "surface" as const,
    id: "tree-foliage",
    key: "tree",
    mesh: plane,
    material: "leaves",
    diagnostics: [],
    details: [
      {
        label: "crown",
        mesh: crown.mesh,
        vegetation: crown.product,
        maxProjectedDiameter: 24,
        maxError: null,
      },
    ],
  };
  const plant: CompiledVegetation = {
    kind: "vegetation",
    id: "tree",
    key: "tree",
    surfaces: [{ ...surface, id: "tree-trunk", details: undefined }, surface],
    windResponse: 0.3,
    maxDisplacement: 0.3,
    bounds: plane.bounds,
    diagnostics: [],
  };
  const cooked = serializeArtifact(plant);
  expect(deserializeArtifact(JSON.parse(JSON.stringify(cooked)))).toEqual(plant);
  const invalid = structuredClone(cooked) as {
    surfaces: { details?: { vegetation: { views: { firstIndex: number }[] } }[] }[];
  };
  const view = invalid.surfaces[1].details?.[0].vegetation.views[0];
  if (!view) throw Error("Missing view");
  view.firstIndex = 2;
  expect(() => deserializeArtifact(invalid)).toThrow("Malformed crown view range");
}, 15000);
test("empty texels cannot bias orientation and coverage remains linear across mips", () => {
  const levels = crownMipChain(
    new Uint8Array([255, 255, 128, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
    2,
    2,
  );
  expect([...levels[1]]).toEqual([64, 255, 128, 128]);
});

test("spatial clusters own source organs once and preserve every source triangle", async () => {
  const { partitionFoliage } = await import("./vegetation-clusters");
  const positions = new Float32Array(plane.positions.length * 8);
  const normals = new Float32Array(positions.length),
    indices: number[] = [],
    sourceIds: string[] = [];
  for (let organ = 0; organ < 8; organ++) {
    for (let vertex = 0; vertex < 4; vertex++) {
      positions.set(
        [plane.positions[vertex * 3] + organ * 3, plane.positions[vertex * 3 + 1], organ % 2],
        organ * 12 + vertex * 3,
      );
      normals.set([0, 0, 1], organ * 12 + vertex * 3);
      sourceIds.push(`tree/branch-${organ}/coverage`);
    }
    indices.push(...plane.indices.map((index) => index + organ * 4));
  }
  const source = { ...plane, positions, normals, indices: Uint32Array.from(indices), sourceIds };
  const clusters = partitionFoliage(source);
  expect(clusters).toHaveLength(4);
  expect(new Set(clusters.flatMap((c) => c.sourceOrgans)).size).toBe(8);
  expect(clusters.reduce((sum, c) => sum + c.mesh.indices.length, 0)).toBe(indices.length);
  const triangles = (values: ArrayLike<number>) =>
    Array.from(
      { length: values.length / 3 },
      (_, i) => `${values[i * 3]},${values[i * 3 + 1]},${values[i * 3 + 2]}`,
    ).sort();
  expect(clusters.flatMap((c) => triangles(c.mesh.indices)).sort()).toEqual(triangles(source.indices));
  for (const cluster of clusters)
    for (const vertex of cluster.mesh.indices)
      for (let axis = 0; axis < 3; axis++) {
        expect(positions[vertex * 3 + axis]).toBeGreaterThanOrEqual(cluster.mesh.bounds.min[axis]);
        expect(positions[vertex * 3 + axis]).toBeLessThanOrEqual(cluster.mesh.bounds.max[axis]);
      }
});
