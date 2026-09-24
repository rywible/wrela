import { expect, test } from "bun:test";
import { type CharacterDefinition, creatureSchema, dot, type FieldNode, sub, type Vec3 } from "@wrela/model";

import { creatureRotate } from "./creature";
import { emptyCreatureMesh, mergeCreatureFeatureMeshes, prepareCreatureFeatures } from "./creature-features";

const node = (
  id: string,
  kind: FieldNode["kind"],
  position: Vec3 = [0, 0, 0],
  size: Vec3 = [0.01, 0.02, 0.04],
  children: string[] = [],
): FieldNode => ({
  id,
  name: id,
  kind,
  position,
  size,
  children,
  radius: 0.01,
  rotation: [0, 0, 0],
  blend: 0.05,
  material: kind === "ellipsoid" ? "ivory" : undefined,
});
function character(): CharacterDefinition {
  return {
    id: "features",
    name: "Features",
    schemaVersion: 1,
    dependencies: [],
    kind: "character",
    material: "skin",
    physics: { mode: "kinematic", mass: 1, restitution: 0, friction: 0.5 },
    joints: [
      {
        id: "root",
        name: "Root",
        parent: null,
        position: [0, 0, 0],
        rotation: [0, 0, 0],
        radius: 3,
        minimum: -3,
        maximum: 3,
      },
    ],
    motions: [],
    creature: creatureSchema.parse({ schemaVersion: 1 }),
    field: {
      root: "root",
      resolution: 12,
      bounds: { min: [-2, -2, -2], max: [2, 2, 2] },
      nodes: [
        node("root", "union", [0, 0, 0], [1, 1, 1], ["body-blend", "claw"]),
        node("body-blend", "smoothUnion", [0, 0, 0], [1, 1, 1], ["body-a", "body-b"]),
        node("body-a", "sphere", [0, 0, 0]),
        node("body-b", "sphere", [0, 0.1, 0]),
        node("claw", "ellipsoid", [0.02, 0, 0.05]),
      ],
    },
  };
}
function inverse(p: Vec3, rotation: Vec3): Vec3 {
  return [
    dot(p, creatureRotate([1, 0, 0], rotation)),
    dot(p, creatureRotate([0, 1, 0], rotation)),
    dot(p, creatureRotate([0, 0, 1], rotation)),
  ];
}

test("millimetre features receive actual surface vertices without increasing global extraction resolution", () => {
  const source = character(),
    claw = source.field.nodes[4];
  claw.size = [0.001, 0.002, 0.015];
  const compiled = prepareCreatureFeatures(source, "interactive");
  expect(compiled.meshes).toHaveLength(1);
  expect(compiled.document.field.resolution).toBe(12);
  expect(compiled.document.field.nodes.find((n) => n.id === "root")?.children).toEqual(["body-blend"]);
  expect(source.field.nodes[0].children).toEqual(["body-blend", "claw"]);
  const mesh = compiled.meshes[0];
  expect(mesh.positions.length).toBeGreaterThan(100);
  for (let i = 0; i < mesh.positions.length; i += 3) {
    const p: [number, number, number] = [mesh.positions[i], mesh.positions[i + 1], mesh.positions[i + 2]];
    const local = inverse(sub(p, claw.position), claw.rotation);
    expect(Math.hypot(...local.map((v, axis) => v / claw.size[axis]))).toBeCloseTo(1, 4);
  }
  expect(mesh.bounds.max[0] - mesh.bounds.min[0]).toBeCloseTo(0.002, 7);
  expect(mesh.sourceIds?.every((id) => id === "claw")).toBe(true);
  expect(mesh.materialGroups?.[0].material).toBe("ivory");
  expect(compiled.domain).toBe("opaque-exterior-hard-union");
});

test("ancestor transforms preserve exact feature radii, normals and bounds", () => {
  const source = character();
  source.field.nodes[0].position = [0.4, 0.3, 0.2];
  source.field.nodes[0].rotation = [0.1, 0.2, 0.7];
  source.field.nodes[4].rotation = [0.3, -0.4, 0.1];
  const compiled = prepareCreatureFeatures(source),
    mesh = compiled.meshes[0],
    root = source.field.nodes[0],
    feature = source.field.nodes[4];
  for (let i = 0; i < mesh.positions.length; i += 3) {
    const world: Vec3 = [mesh.positions[i], mesh.positions[i + 1], mesh.positions[i + 2]],
      parent = inverse(sub(world, root.position), root.rotation),
      local = inverse(sub(parent, feature.position), feature.rotation);
    expect(Math.hypot(...local.map((v, axis) => v / feature.size[axis]))).toBeCloseTo(1, 4);
    expect(Math.hypot(mesh.normals[i], mesh.normals[i + 1], mesh.normals[i + 2])).toBeCloseTo(1, 5);
  }
});

test("smooth blends, subtraction, intersections and shared DAG ancestry cannot be split", () => {
  for (const operation of ["smoothUnion", "subtract", "intersect"] as const) {
    const source = character();
    source.field.nodes[0].kind = operation;
    expect(prepareCreatureFeatures(source).meshes).toEqual([]);
  }
  const shared = character();
  shared.field.nodes[1].children.push("claw");
  expect(prepareCreatureFeatures(shared).meshes).toEqual([]);
});

test("intentional clipping and bounded resource exhaustion retain the original realization", () => {
  const source = character();
  source.field.nodes[4].position = [2, 0, 0];
  const clipped = prepareCreatureFeatures(source);
  expect(clipped.meshes).toEqual([]);
  expect(clipped.document).toBe(source);
  expect(clipped.diagnostics[0].code).toBe("creature.feature.clipped-fallback");
  source.field.nodes[4].position = [0, 0, 0];
  expect(prepareCreatureFeatures(source, "review", 0).meshes).toEqual([]);
  expect(() => prepareCreatureFeatures(source, "review", Infinity)).toThrow("vertex budget");
});

test("whole hard unions need no coarse remnant and material groups consolidate without identity loss", () => {
  const source = character();
  source.field.nodes[0].children = ["claw", "another"];
  source.field.nodes.push(node("another", "ellipsoid", [0.3, 0, 0]));
  const compiled = prepareCreatureFeatures(source);
  expect(compiled.empty).toBe(true);
  expect(compiled.meshes).toHaveLength(2);
  const merged = mergeCreatureFeatureMeshes(emptyCreatureMesh(), compiled.meshes, source.material);
  expect(merged.materialGroups).toHaveLength(1);
  expect(new Set(merged.sourceIds)).toEqual(new Set(["claw", "another"]));
  expect(merged.indices.length).toBe(compiled.meshes.reduce((sum, mesh) => sum + mesh.indices.length, 0));
});
