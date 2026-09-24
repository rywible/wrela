import { describe, expect, test } from "bun:test";
import {
  type AssemblyDefinition,
  assemblyJointValues,
  assemblyPartPoint,
  assemblySchema,
} from "@wrela/model";

import { assemblyProfilePoints, compileAssemblyMesh } from "./assembly";

const beam = (): AssemblyDefinition =>
  assemblySchema.parse({
    grid: 0.1,
    clearances: [],
    parts: [
      {
        id: "beam",
        name: "Beam",
        profile: { kind: "rectangle", width: 2, height: 1 },
        path: [
          [0, 0, 0],
          [0, 0, 3],
        ],
        bevel: 0,
        position: [0, 0, 0],
        rotation: [0, 0, 0],
        repeat: { count: 1, offset: [3, 0, 0] },
        sockets: [],
        wear: { amount: 0.2, scale: 1, seed: 1 },
      },
    ],
  });
describe("dimensioned assembly compilation", () => {
  test("sweeps preserve dimensions and outward winding", () => {
    const { mesh } = compileAssemblyMesh(beam(), "metal");
    expect(mesh.bounds).toEqual({ min: [-1, -0.5, 0], max: [1, 0.5, 3] });
    expect(mesh.indices.length).toBe(48);
    for (let i = 0; i < mesh.positions.length; i += 3) {
      const delta = [mesh.positions[i], mesh.positions[i + 1], mesh.positions[i + 2] - 1.5];
      expect(delta.reduce((sum, v, axis) => sum + v * mesh.normals[i + axis], 0)).toBeGreaterThan(0);
    }
    expect(new Set(mesh.sourceIds)).toEqual(new Set(["beam"]));
  });
  test("bevels cut corners without shrinking the outside envelope", () => {
    const profile = assemblyProfilePoints({ kind: "rectangle", width: 2, height: 1 }, 0.1);
    expect(profile).toHaveLength(8);
    expect(profile.some((p) => p[0] === 1 && p[1] === 0.5)).toBe(false);
    expect(Math.max(...profile.map((p) => p[0]))).toBe(1);
  });
  test("repeats, material assignment and geometric clearance diagnostics", () => {
    const assembly = beam();
    assembly.parts[0].repeat.count = 3;
    assembly.parts[0].material = "wood";
    assembly.clearances = [{ id: "reserved", name: "Walkway", min: [2, -1, 1], max: [4, 1, 2] }];
    const { mesh, diagnostics } = compileAssemblyMesh(assembly, "metal");
    expect(mesh.bounds.max[0]).toBe(7);
    expect(mesh.materialGroups?.[0].material).toBe("wood");
    expect(diagnostics).toHaveLength(1);
  });
  test("hierarchical hinges clamp and drives reproduce absolute time", () => {
    const assembly = beam();
    assembly.parts[0].joint = {
      kind: "hinge",
      axis: [0, 0, 1],
      pivot: [0, 0, 0],
      minimum: 0,
      maximum: Math.PI / 2,
      value: 0,
      drive: { period: 4, phase: 0 },
    };
    const values = assemblyJointValues(assembly, 2);
    expect(values.beam).toBeCloseTo(Math.PI / 2);
    const point = assemblyPartPoint(assembly, assembly.parts[0], [1, 0, 0], { beam: 100 });
    expect(point[0]).toBeCloseTo(0);
    expect(point[1]).toBeCloseTo(1);
    expect(assemblyJointValues(assembly, 6)).toEqual(values);
  });
  test("rejects invalid topology instead of emitting broken geometry", () => {
    const assembly = beam();
    assembly.parts[0].parent = "beam";
    expect(assemblySchema.safeParse(assembly).success).toBe(false);
    delete assembly.parts[0].parent;
    assembly.parts[0].path = [
      [0, 0, 0],
      [0, 0, 0],
    ];
    expect(assemblySchema.safeParse(assembly).success).toBe(false);
  });
});

test("reserved volumes reject only actual geometry, including enclosed clearances and exact tangency", () => {
  const assembly = beam();
  assembly.parts[0].profile = { kind: "rectangle", width: 0.2, height: 0.2 };
  assembly.parts[0].path = [
    [-2, 0, 0],
    [2, 4, 0],
  ];
  // The diagonal beam's bounds span this volume, but the beam itself does not.
  assembly.clearances = [{ id: "walkway", name: "Walkway", min: [-1.5, 2.5, -0.05], max: [-1, 3, 0.05] }];
  expect(compileAssemblyMesh(assembly, "metal").diagnostics).toEqual([]);
  assembly.clearances[0] = { id: "walkway", name: "Walkway", min: [-0.2, 1.8, -0.2], max: [0.2, 2.2, 0.2] };
  expect(compileAssemblyMesh(assembly, "metal").diagnostics).toHaveLength(1);
  const solid = beam();
  solid.clearances = [{ id: "inside", name: "Inside solid", min: [-0.1, -0.1, 1], max: [0.1, 0.1, 2] }];
  expect(compileAssemblyMesh(solid, "metal").diagnostics).toHaveLength(1);
  solid.clearances[0] = { id: "touching", name: "Touches surface", min: [1, -0.1, 1], max: [2, 0.1, 2] };
  expect(compileAssemblyMesh(solid, "metal").diagnostics).toEqual([]);
});

test("linked transmissions preserve dimensions in the rest pose and propagate source overrides", () => {
  const assembly = beam();
  const driver = assembly.parts[0];
  driver.joint = {
    kind: "hinge",
    axis: [0, 0, 1],
    pivot: [0, 0, 0],
    minimum: 0,
    maximum: 2,
    value: 0.5,
    drive: { period: 4, phase: 0 },
  };
  const rack = structuredClone(driver);
  rack.id = "rack";
  rack.joint = {
    kind: "slider",
    axis: [1, 0, 0],
    pivot: [0, 0, 0],
    minimum: -2,
    maximum: 2,
    value: 0,
    link: { part: "beam", ratio: -0.5, offset: 0.2 },
  };
  const follower = structuredClone(rack);
  follower.id = "follower";
  follower.joint = { ...rack.joint, link: { part: "rack", ratio: 2, offset: 0 } };
  assembly.parts = [follower, rack, driver];
  expect(assemblySchema.safeParse(assembly).success).toBe(true);
  expect(assemblyPartPoint(assembly, rack, [0, 0, 0])[0]).toBeCloseTo(-0.05);
  expect(assemblyJointValues(assembly, 2).rack).toBeCloseTo(-0.8);
  expect(assemblyJointValues(assembly, 2).follower).toBeCloseTo(-1.6);
  expect(assemblyJointValues(assembly, 2, { beam: 0.6 }).follower).toBeCloseTo(-0.2);
  expect(assemblyJointValues(assembly, 2, { rack: 0.7 }).follower).toBeCloseTo(1.4);
  driver.joint.link = { part: "follower", ratio: 1, offset: 0 };
  driver.joint.drive = undefined;
  expect(assemblySchema.safeParse(assembly).success).toBe(false);
  driver.joint.link = { part: "missing", ratio: 1, offset: 0 };
  expect(assemblySchema.safeParse(assembly).success).toBe(false);
  expect(() => assemblyJointValues(beam(), Number.NaN)).toThrow("finite");
});

test("valid part identities do not inherit JavaScript object properties as joint commands", () => {
  const assembly = beam(),
    part = assembly.parts[0];
  part.id = "constructor";
  part.joint = { kind: "slider", axis: [1, 0, 0], pivot: [0, 0, 0], minimum: 0, maximum: 2, value: 0.5 };
  expect(Object.getOwnPropertyDescriptor(assemblyJointValues(assembly, 0), "constructor")?.value).toBe(0.5);
  expect(assemblyPartPoint(assembly, part, [0, 0, 0])[0]).toBe(0.5);
  part.id = "__proto__";
  expect(assemblyJointValues(assembly, 0).__proto__).toBe(0.5);
  expect(assemblyPartPoint(assembly, part, [0, 0, 0])[0]).toBe(0.5);
});

test("assembly artifact identity and cooked mesh retain motion provenance", async () => {
  const { referenceProject } = await import("@wrela/examples");
  const { compileDocument, compilerKey, serializeArtifact, deserializeArtifact } = await import("./index");
  const object = referenceProject().documents.find((d) => d.id === "river-stone");
  if (object?.kind !== "object") throw new Error("Missing object fixture");
  object.assembly = beam();
  const artifact = compileDocument(object, "interactive");
  if (artifact?.kind !== "surface") throw new Error("Expected assembly surface");
  expect(artifact.key).toBe(compilerKey(object, "interactive"));
  const reopened = deserializeArtifact(JSON.parse(JSON.stringify(serializeArtifact(artifact))));
  if (reopened.kind !== "surface") throw new Error("Expected reopened surface");
  expect(reopened.mesh.sourceIds).toEqual(artifact.mesh.sourceIds);
  expect(reopened.mesh.colors).toEqual(artifact.mesh.colors);
  expect(reopened.mesh.materialGroups).toEqual(artifact.mesh.materialGroups);
});

test("source rejects excessive realization before committing an uncookable assembly", () => {
  const assembly = beam();
  assembly.parts[0].profile = { kind: "circle", radius: 1, segments: 64 };
  assembly.parts[0].path = Array.from({ length: 64 }, (_, i) => [0, 0, i]);
  assembly.parts[0].repeat.count = 10;
  const result = assemblySchema.safeParse(assembly);
  expect(result.success).toBe(false);
  if (!result.success)
    expect(result.error.issues.some((issue) => issue.message.includes("80,000 triangle budget"))).toBe(true);
});

test("end bevels create highlight-bearing cap transitions without shrinking dimensions", () => {
  const assembly = beam();
  assembly.parts[0].endBevel = 0.08;
  const { mesh } = compileAssemblyMesh(assembly, "stone");
  expect(mesh.bounds).toEqual({ min: [-1, -0.5, 0], max: [1, 0.5, 3] });
  expect(mesh.indices.length / 3).toBe(32);
  let sloped = 0;
  for (let i = 0; i < mesh.normals.length; i += 3)
    if (Math.abs(mesh.normals[i + 2]) > 0.1 && Math.abs(mesh.normals[i + 2]) < 0.99) sloped++;
  expect(sloped).toBeGreaterThan(0);
});

test("edge erosion is deterministic, bounded, and preserves broad-face dimensions and closed caps", () => {
  const source = beam();
  const part = source.parts[0];
  part.bevel = 0.004;
  part.endBevel = 0.004;
  const clean = compileAssemblyMesh(source, "stone").mesh;
  part.edgeWear = 0;
  expect(compileAssemblyMesh(source, "stone").mesh).toEqual(clean);
  part.edgeWear = 0.9;
  const worn = compileAssemblyMesh(source, "stone").mesh;
  expect(compileAssemblyMesh(source, "stone").mesh).toEqual(worn);
  expect(worn.bounds).toEqual(clean.bounds);
  expect(worn.indices.length).toBeGreaterThan(clean.indices.length);
  expect(worn.indices.length / 3).toBeLessThan(1200);
  expect(worn.positions.every(Number.isFinite)).toBe(true);
  expect(worn.normals.every(Number.isFinite)).toBe(true);
  // A closed triangulation has exactly two incidences at every welded edge.
  const edges = new Map<string, number>();
  const point = (index: number) =>
    Array.from(worn.positions.slice(index * 3, index * 3 + 3))
      .map((v) => v.toFixed(6))
      .join(",");
  for (let i = 0; i < worn.indices.length; i += 3)
    for (const [a, b] of [
      [0, 1],
      [1, 2],
      [2, 0],
    ]) {
      const key = [point(worn.indices[i + a]), point(worn.indices[i + b])].sort().join("|");
      edges.set(key, (edges.get(key) ?? 0) + 1);
    }
  expect([...edges.values()].every((n) => n === 2)).toBe(true);
});
