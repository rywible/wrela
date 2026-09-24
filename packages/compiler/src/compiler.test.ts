import { describe, expect, test } from "bun:test";
import {
  type CharacterDefinition,
  contentKey,
  type FieldDefinition,
  type FieldNode,
  type MeshData,
  type ObjectDefinition,
  type TerrainDefinition,
  type VegetationDefinition,
  type WaterDefinition,
} from "@wrela/model";

import {
  artifactTransfers,
  compileCharacter,
  compileDocument,
  compileField,
  compilerKey,
  compileSurface,
  compileVegetation,
  evaluateField,
  generateTerrainPatch,
  queryWater,
  terrainHeight,
  terrainNormal,
} from "./index";

const envelope = { id: "test", name: "Test", schemaVersion: 1 as const, dependencies: [] };
const node = (id: string, kind: FieldNode["kind"] = "sphere", extra: Partial<FieldNode> = {}): FieldNode => ({
  id,
  name: id,
  kind,
  position: [0, 0, 0],
  rotation: [0, 0, 0],
  size: [1, 1, 1],
  radius: 1,
  blend: 0.2,
  children: [],
  ...extra,
});
const field = (nodes: FieldNode[] = [node("ball")], root = nodes[0].id): FieldDefinition => ({
  root,
  nodes,
  bounds: { min: [-1.5, -1.5, -1.5], max: [1.5, 1.5, 1.5] },
  resolution: 24,
});
const object: ObjectDefinition = {
  ...envelope,
  kind: "object",
  field: field(),
  material: "clay",
  collision: "sphere",
};
const terrain: TerrainDefinition = {
  ...envelope,
  kind: "terrain",
  seed: 43,
  amplitude: 20,
  frequency: 0.02,
  octaves: 4,
  baseHeight: 2,
  material: "grass",
  interventions: [],
};
const water: WaterDefinition = {
  ...envelope,
  kind: "water",
  level: 3,
  color: [0.1, 0.3, 0.5],
  roughness: 0.2,
  waves: [
    { amplitude: 0.4, wavelength: 6, speed: 2, direction: 0.8, phase: 0.2 },
    { amplitude: 0.1, wavelength: 2, speed: -0.4, direction: -0.4, phase: 2 },
  ],
};
function assertFiniteMesh(mesh: MeshData) {
  expect(mesh.positions.length % 3).toBe(0);
  expect(mesh.normals.length).toBe(mesh.positions.length);
  for (const v of mesh.positions) expect(Number.isFinite(v)).toBe(true);
  for (const v of mesh.normals) expect(Number.isFinite(v)).toBe(true);
  for (const v of mesh.indices) expect(v).toBeLessThan(mesh.positions.length / 3);
}
describe("field programs", () => {
  test("sphere distance, transformed box, capsule and torus semantics", () => {
    expect(evaluateField(field(), [0, 0, 0])).toBe(-1);
    expect(evaluateField(field(), [2, 0, 0])).toBe(1);
    const box = field([
      node("box", "box", { position: [2, 0, 0], rotation: [0, 0, Math.PI / 2], size: [2, 0.5, 0.5] }),
    ]);
    expect(evaluateField(box, [2, 1.5, 0])).toBeCloseTo(-0.5, 5);
    expect(evaluateField(box, [3, 0, 0])).toBeCloseTo(0.5, 5);
    expect(evaluateField(field([node("capsule", "capsule")]), [0, 2, 0])).toBeCloseTo(0);
    expect(evaluateField(field([node("ring", "torus", { radius: 0.2 })]), [1, 0, 0])).toBeCloseTo(-0.2);
    expect(evaluateField(field([node("ellipsoid", "ellipsoid", { size: [1, 2, 3] })]), [0, 0, 0])).toBe(-1);
  });
  test("CSG operations retain generating provenance and group transforms", () => {
    const nodes = [
      node("group", "union", { position: [5, 0, 0], children: ["left", "right"] }),
      node("left", "sphere", { position: [-0.6, 0, 0] }),
      node("right", "sphere", { position: [0.6, 0, 0] }),
    ];
    const f = field(nodes);
    expect(compileField(f).sample([4.4, 0, 0]).source).toBe("left");
    nodes[0].kind = "subtract";
    expect(evaluateField(f, [5.6, 0, 0])).toBeGreaterThan(0);
    nodes[0].kind = "intersect";
    expect(evaluateField(f, [5, 0, 0])).toBeLessThan(0);
    nodes[0].kind = "smoothUnion";
    expect(evaluateField(f, [5, 0, 0])).toBeLessThan(-0.4);
  });
  test("rejects cycles, missing references, and exponentially expanding DAGs", () => {
    expect(() => compileField(field([node("a", "union", { children: ["a"] })]))).toThrow("Cycle");
    expect(() => compileField(field([node("a", "union", { children: ["missing"] })]))).toThrow("Missing");
    const nodes = [node("leaf")];
    for (let i = 0; i < 10; i++)
      nodes.push(node(`group${i}`, "union", { children: Array(4).fill(nodes[nodes.length - 1].id) }));
    expect(() => compileField(field(nodes, nodes[nodes.length - 1].id))).toThrow("512");
  });
});
describe("surface compilation", () => {
  test("sphere mesh is closed, outward oriented and approximates analytic volume", () => {
    const result = compileSurface(object, "review"),
      mesh = result.mesh;
    assertFiniteMesh(mesh);
    expect(result.diagnostics).toHaveLength(0);
    const edgeCounts = new Map<string, number>();
    let volume = 0;
    for (let i = 0; i < mesh.indices.length; i += 3) {
      const ids = Array.from(mesh.indices.slice(i, i + 3));
      for (let j = 0; j < 3; j++) {
        const a = ids[j],
          b = ids[(j + 1) % 3],
          key = `${Math.min(a, b)}:${Math.max(a, b)}`;
        edgeCounts.set(key, (edgeCounts.get(key) ?? 0) + 1);
      }
      const [a, b, c] = ids.map((id) => Array.from(mesh.positions.slice(id * 3, id * 3 + 3)));
      volume +=
        (a[0] * (b[1] * c[2] - b[2] * c[1]) +
          a[1] * (b[2] * c[0] - b[0] * c[2]) +
          a[2] * (b[0] * c[1] - b[1] * c[0])) /
        6;
    }
    expect([...edgeCounts.values()].every((n) => n === 2)).toBe(true);
    expect(Math.abs(volume - (4 * Math.PI) / 3)).toBeLessThan(0.06);
    for (let i = 0; i < mesh.positions.length; i += 3) {
      const p = mesh.positions.slice(i, i + 3),
        n = mesh.normals.slice(i, i + 3);
      expect(p[0] * n[0] + p[1] * n[1] + p[2] * n[2]).toBeGreaterThan(0.97);
      expect(mesh.sourceIds?.[i / 3]).toBe("ball");
    }
  });
  test("reports truncated surfaces and validates hostile resolution", () => {
    const clipped = structuredClone(object);
    clipped.field.bounds = { min: [-0.5, -0.5, -0.5], max: [0.5, 0.5, 0.5] };
    expect(compileSurface(clipped).diagnostics.some((d) => d.code === "field.clipped")).toBe(true);
    const bad = structuredClone(object);
    bad.field.resolution = 1000;
    expect(() => compileDocument(bad)).toThrow();
  });
  test("keys ignore labels and unrelated material parameters; quality and shape invalidate", () => {
    const rename = { ...object, name: "New name" };
    expect(compilerKey(rename)).toBe(compilerKey(object));
    const fieldRename = structuredClone(object);
    fieldRename.field.nodes[0].name = "Renamed node";
    expect(compilerKey(fieldRename)).toBe(compilerKey(object));
    const shape = structuredClone(object);
    shape.field.nodes[0].radius = 0.8;
    expect(compilerKey(shape)).not.toBe(compilerKey(object));
    expect(compilerKey(object, "interactive")).not.toBe(compilerKey(object, "export"));
    expect(compilerKey({ ...object, id: "duplicate" })).not.toBe(compilerKey(object));
    const artifact = compileDocument(object);
    if (!artifact) throw new Error("Expected compiled object");
    expect(artifact.key).toBe(compilerKey(object));
    const transfers = artifactTransfers(artifact);
    expect(new Set(transfers).size).toBe(transfers.length);
    expect(transfers).toContain(
      artifact.kind === "vegetation"
        ? (artifact.surfaces[0].mesh.positions.buffer as ArrayBuffer)
        : (artifact.mesh.positions.buffer as ArrayBuffer),
    );
    if (artifact.kind === "surface")
      for (const product of artifact.renderProducts ?? [])
        if (product.kind === "parametric-mesh") {
          expect(transfers).toContain(product.mesh.positions.buffer as ArrayBuffer);
          expect(transfers).toContain(product.mesh.indices.buffer as ArrayBuffer);
        }
  });
  test("envelope bindings are finite normalized and rigid at isolated extremes", () => {
    const character: CharacterDefinition = {
      ...envelope,
      kind: "character",
      field: field(),
      material: "clay",
      joints: [
        {
          id: "root",
          name: "Root",
          parent: null,
          position: [0, -0.8, 0],
          rotation: [0, 0, 0],
          radius: 1.1,
          minimum: -Math.PI,
          maximum: Math.PI,
        },
        {
          id: "head",
          name: "Head",
          parent: "root",
          position: [0, 0.8, 0],
          rotation: [0, 0, 0],
          radius: 0.8,
          minimum: -Math.PI,
          maximum: Math.PI,
        },
      ],
      motions: [],
      physics: { mode: "kinematic", mass: 1, restitution: 0, friction: 0.5 },
    };
    const artifact = compileCharacter(character, "interactive");
    expect(artifact.weights.length).toBe((artifact.mesh.positions.length / 3) * 4);
    for (let i = 0; i < artifact.weights.length; i += 4) {
      expect(Array.from(artifact.weights.slice(i, i + 4)).reduce((a, b) => a + b, 0)).toBeCloseTo(1, 6);
      for (let j = 0; j < 4; j++) {
        expect(artifact.weights[i + j]).toBeGreaterThanOrEqual(0);
        expect(artifact.jointIndices[i + j]).toBeLessThan(2);
      }
    }
    expect(artifactTransfers(artifact)).toHaveLength(5);
  });
});
describe("terrain spatial contract", () => {
  test("request ordering and negative coordinate borders preserve samples", () => {
    const a = generateTerrainPatch(terrain, -32, -16, 16, 8),
      b = generateTerrainPatch(terrain, -16, -16, 16, 8);
    assertFiniteMesh(a);
    assertFiniteMesh(b);
    for (let j = 0; j <= 8; j++) {
      expect(a.positions[(j * 9 + 8) * 3 + 1]).toBe(b.positions[j * 9 * 3 + 1]);
      expect(Array.from(a.normals.slice((j * 9 + 8) * 3, (j * 9 + 8) * 3 + 3))).toEqual(
        Array.from(b.normals.slice(j * 9 * 3, j * 9 * 3 + 3)),
      );
    }
    const repeat = generateTerrainPatch(terrain, -32, -16, 16, 8);
    expect(contentKey(Array.from(repeat.positions))).toBe(contentKey(Array.from(a.positions)));
  });
  test("every supported stitched edge matches the coarser piecewise linear surface", () => {
    for (const side of ["north", "south", "east", "west"] as const) {
      const patch = generateTerrainPatch(terrain, -16, -16, 16, 8, { [side]: true });
      for (let i = 1; i < 8; i += 2) {
        const row = side === "north" ? 0 : side === "south" ? 8 : i,
          col = side === "west" ? 0 : side === "east" ? 8 : i,
          id = row * 9 + col,
          delta = side === "north" || side === "south" ? 1 : 9;
        expect(patch.positions[id * 3 + 1]).toBeCloseTo(
          (patch.positions[(id - delta) * 3 + 1] + patch.positions[(id + delta) * 3 + 1]) / 2,
          5,
        );
      }
    }
    expect(() => generateTerrainPatch(terrain, 0, 0, 16, 7, { north: true })).toThrow("even");
  });
  test("interventions have bounded spatial support and continuous boundaries", () => {
    const edited = {
      ...terrain,
      interventions: [
        {
          id: "raise",
          kind: "raise" as const,
          center: [0, 0] as [number, number],
          radius: 10,
          strength: 8,
          targetHeight: 0,
        },
      ],
    };
    expect(terrainHeight(edited, 0, 0) - terrainHeight(terrain, 0, 0)).toBeCloseTo(8);
    expect(terrainHeight(edited, 11, 0)).toBe(terrainHeight(terrain, 11, 0));
    expect(Math.abs(terrainHeight(edited, 9.999, 0) - terrainHeight(terrain, 9.999, 0))).toBeLessThan(1e-8);
    expect(Math.hypot(...terrainNormal(terrain, -15, 12))).toBeCloseTo(1, 10);
    expect(() => terrainHeight(terrain, 1e10, 0)).toThrow();
  });
});
describe("water and vegetation", () => {
  test("analytic water normal and velocity agree with derivatives of actual height", () => {
    const x = -3.24,
      z = 2.34,
      t = 12.4,
      e = 1e-5,
      q = queryWater(water, x, z, t),
      dx = (queryWater(water, x + e, z, t).height - queryWater(water, x - e, z, t).height) / (2 * e),
      dz = (queryWater(water, x, z + e, t).height - queryWater(water, x, z - e, t).height) / (2 * e),
      dt = (queryWater(water, x, z, t + e).height - queryWater(water, x, z, t - e).height) / (2 * e);
    expect(-q.normal[0] / q.normal[1]).toBeCloseTo(dx, 6);
    expect(-q.normal[2] / q.normal[1]).toBeCloseTo(dz, 6);
    expect(q.velocity[1]).toBeCloseTo(dt, 6);
  });
  test("vegetation is seed reproducible with separate bark and foliage and bounded geometry", () => {
    const tree: VegetationDefinition = {
      ...envelope,
      kind: "vegetation",
      height: 8,
      radius: 3,
      branches: 14,
      seed: 52,
      material: "leaves",
      trunkMaterial: "bark",
      windResponse: 0.5,
      variation: 0.8,
    };
    expect(compilerKey({ ...tree, windResponse: 1.2 })).toBe(compilerKey(tree));
    const a = compileVegetation(tree),
      b = compileVegetation(tree);
    expect(a.surfaces.map((s) => s.material)).toEqual(["bark", "leaves"]);
    expect(a.surfaces[0].mesh.positions).toEqual(b.surfaces[0].mesh.positions);
    expect(compileVegetation({ ...tree, seed: 54 }).surfaces[1].mesh.positions).not.toEqual(
      a.surfaces[1].mesh.positions,
    );
    for (const s of a.surfaces) {
      assertFiniteMesh(s.mesh);
      expect(s.mesh.positions.length).toBeLessThan(100000);
      let disagree = 0;
      for (let i = 0; i < s.mesh.indices.length; i += 3) {
        const [a, b, c] = Array.from(s.mesh.indices.slice(i, i + 3)).map((v) => v * 3),
          p = s.mesh.positions,
          n = s.mesh.normals,
          ux = p[b] - p[a],
          uy = p[b + 1] - p[a + 1],
          uz = p[b + 2] - p[a + 2],
          vx = p[c] - p[a],
          vy = p[c + 1] - p[a + 1],
          vz = p[c + 2] - p[a + 2];
        if (
          (uy * vz - uz * vy) * n[a] + (uz * vx - ux * vz) * n[a + 1] + (ux * vy - uy * vx) * n[a + 2] <
          -1e-5
        )
          disagree++;
      }
      expect(disagree).toBe(0);
    }
  });
});

test("authored node materials produce complete contiguous triangle groups", () => {
  const doc = structuredClone(object);
  doc.field = field([
    node("union", "union", { children: ["left", "right"] }),
    node("left", "sphere", { position: [-0.55, 0, 0], radius: 0.65, material: "dark" }),
    node("right", "sphere", { position: [0.55, 0, 0], radius: 0.65 }),
  ]);
  const artifact = compileSurface(doc),
    groups = artifact.mesh.materialGroups;
  expect(groups?.map((group) => group.material).sort()).toEqual(["clay", "dark"]);
  let end = 0;
  for (const group of groups ?? []) {
    expect(group.start).toBe(end);
    expect(group.count % 3).toBe(0);
    expect(group.count).toBeGreaterThan(0);
    end += group.count;
  }
  expect(end).toBe(artifact.mesh.indices.length);
  expect(artifact.mesh.sourceIds?.length).toBe(artifact.mesh.positions.length / 3);
});

test("all sixteen stitching masks preserve coarse borders and corner normals at each patch scale", () => {
  const edges = ["north", "east", "south", "west"] as const,
    resolution = 16,
    row = 17;
  const edgeVertex = (edge: (typeof edges)[number], index: number) =>
    edge === "north"
      ? index
      : edge === "south"
        ? resolution * row + index
        : edge === "west"
          ? index * row
          : index * row + resolution;
  for (const size of [32, 64, 128, 256])
    for (let mask = 0; mask < 16; mask++) {
      const x = -1024 - size * 3,
        z = -768 - size * 5,
        stitch = Object.fromEntries(edges.map((edge, index) => [edge, !!(mask & (1 << index))]));
      const fine = generateTerrainPatch(terrain, x, z, size, resolution, stitch);
      for (const [edgeIndex, edge] of edges.entries()) {
        const coarser = !!(mask & (1 << edgeIndex)),
          neighborSize = size * (coarser ? 2 : 1),
          nx = x + (edge === "west" ? -neighborSize : edge === "east" ? size : 0),
          nz = z + (edge === "north" ? -neighborSize : edge === "south" ? size : 0),
          opposite = edges[(edgeIndex + 2) % 4],
          neighbor = generateTerrainPatch(terrain, nx, nz, neighborSize, resolution);
        for (let index = 0; index <= resolution; index++) {
          const at = edgeVertex(edge, index),
            coordinate = index / (coarser ? 2 : 1),
            low = edgeVertex(opposite, Math.floor(coordinate)),
            high = edgeVertex(opposite, Math.ceil(coordinate)),
            alpha = coordinate - Math.floor(coordinate);
          expect(fine.positions[at * 3 + 1]).toBeCloseTo(
            neighbor.positions[low * 3 + 1] * (1 - alpha) + neighbor.positions[high * 3 + 1] * alpha,
            5,
          );
          const normal = [0, 1, 2].map(
              (axis) =>
                neighbor.normals[low * 3 + axis] * (1 - alpha) + neighbor.normals[high * 3 + axis] * alpha,
            ),
            length = Math.hypot(...normal);
          for (let axis = 0; axis < 3; axis++)
            expect(fine.normals[at * 3 + axis]).toBeCloseTo(normal[axis] / length, 6);
        }
      }
    }
});
test("supported coordinate-domain borders admit one-sided normals and bounded patches", () => {
  const limit = 1_000_000_000;
  for (const sign of [-1, 1]) {
    expect(Math.hypot(...terrainNormal(terrain, sign * limit, sign * limit))).toBeCloseTo(1, 6);
    const minimum = sign > 0 ? limit - 32 : -limit;
    expect(generateTerrainPatch(terrain, minimum, minimum, 32, 16).positions.length).toBe(17 * 17 * 3);
  }
});
test("vegetation wind envelopes contain extreme responses, phases and scaled rotations", () => {
  const tree: VegetationDefinition = {
      ...envelope,
      kind: "vegetation",
      height: 8,
      radius: 3,
      branches: 9,
      seed: 73,
      material: "leaves",
      trunkMaterial: "bark",
      windResponse: 0.1,
      variation: 0.8,
    },
    artifact = compileVegetation(tree, "interactive");
  expect(artifact.maxDisplacement).toBeLessThanOrEqual(1.2);
  expect(artifact.maxDisplacement).toBeGreaterThan(0);
  for (const factor of [0.01, 0.5, 1, 4])
    for (const angle of [0, 0.7, 1.5]) {
      const c = Math.cos(angle),
        s = Math.sin(angle),
        transform = (x: number, y: number, z: number) => [
          (c * x - s * y) * factor,
          (s * x + c * y) * factor,
          z * factor,
        ],
        minimum = [Infinity, Infinity, Infinity],
        maximum = [-Infinity, -Infinity, -Infinity];
      for (let corner = 0; corner < 8; corner++) {
        const p = transform(
          artifact.bounds[corner & 1 ? "max" : "min"][0],
          artifact.bounds[corner & 2 ? "max" : "min"][1],
          artifact.bounds[corner & 4 ? "max" : "min"][2],
        );
        for (let axis = 0; axis < 3; axis++) {
          minimum[axis] = Math.min(minimum[axis], p[axis]);
          maximum[axis] = Math.max(maximum[axis], p[axis]);
        }
      }
      for (const surface of artifact.surfaces)
        for (
          let vertex = 0;
          vertex < surface.mesh.positions.length;
          vertex += Math.max(3, Math.floor(surface.mesh.positions.length / 300) * 3)
        )
          for (const wind of [
            [0, 0],
            [1, 2],
            [1e6, -1e6],
          ])
            for (const phase of [0, 0.7, Math.PI / 2, 3]) {
              const p = surface.mesh.positions,
                world = transform(p[vertex], p[vertex + 1], p[vertex + 2]),
                speed = Math.hypot(...wind),
                amplitude =
                  ((Math.min(Math.max(p[vertex + 1], 0) ** 2 * 0.012, 0.6) * 2 * Math.min(speed, 10)) / 10) *
                  factor *
                  Math.sin(phase);
              if (speed) {
                world[0] += (wind[0] / speed) * amplitude;
                world[2] += (wind[1] / speed) * amplitude;
              }
              for (let axis = 0; axis < 3; axis++) {
                expect(world[axis]).toBeGreaterThanOrEqual(minimum[axis] - 1e-5);
                expect(world[axis]).toBeLessThanOrEqual(maximum[axis] + 1e-5);
              }
            }
    }
});
