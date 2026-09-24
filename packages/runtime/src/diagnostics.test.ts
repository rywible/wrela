import { expect, test } from "bun:test";
import { compileCharacter } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import type { CharacterDefinition, Vec3 } from "@wrela/model";
import { quatFromEuler, rotateVector } from "./animation";
import { colliderDiagnosticSegments, runtimeDiagnostics } from "./diagnostics";
import { type DiagnosticCollider, PhysicsAdapter } from "./physics";
import { RuntimeSession } from "./session";

const near = (actual: Vec3, expected: Vec3, digits = 5) =>
  actual.forEach((value, axis) => {
    expect(value).toBeCloseTo(expected[axis], digits);
  });
test("primitive wires lie on exact scaled, rotated and translated installed shape surfaces", () => {
  const q = quatFromEuler([0.3, -0.7, 1.1]),
    origin: Vec3 = [2048, 0, -4096],
    position: Vec3 = [2051, 4, -4091];
  const shapes: DiagnosticCollider[] = [
    { instanceId: "body", colliderId: "box", shape: "box", size: [0.4, 1.2, 0.7], position, rotation: q },
    { instanceId: "body", colliderId: "sphere", shape: "sphere", radius: 1.2, position, rotation: q },
    {
      instanceId: "body",
      colliderId: "capsule",
      shape: "capsule",
      radius: 0.4,
      halfHeight: 1.3,
      position,
      rotation: q,
    },
  ];
  for (const shape of shapes) {
    const lines = colliderDiagnosticSegments(shape, origin);
    expect(lines.length).toBe(shape.shape === "box" ? 12 : shape.shape === "sphere" ? 72 : 100);
    for (const line of lines)
      for (const world of [line.a, line.b]) {
        const local = rotateVector(
          [-q[0], -q[1], -q[2], q[3]],
          world.map((value, axis) => value + origin[axis] - position[axis]) as Vec3,
        );
        if (shape.shape === "box")
          for (let axis = 0; axis < 3; axis++) expect(Math.abs(local[axis])).toBeCloseTo(shape.size[axis], 5);
        else if (shape.shape === "sphere") expect(Math.hypot(...local)).toBeCloseTo(shape.radius, 5);
        else
          expect(
            Math.hypot(local[0], Math.max(0, Math.abs(local[1]) - shape.halfHeight), local[2]),
          ).toBeCloseTo(shape.radius, 5);
        expect(line.colliderId).toBe(shape.colliderId);
      }
  }
});
test("physics snapshots expose compound authored identity, body orientation, rebase and restore without stale disabled shapes", async () => {
  const physics = await PhysicsAdapter.create([0, 0, 0]);
  try {
    const rotation = quatFromEuler([0, 0, Math.PI / 2]);
    physics.add({
      id: "actor",
      position: [1000, 5, 2000],
      rotation,
      radius: 0.2,
      halfHeight: 1,
      mode: "dynamic",
      mass: 1,
      friction: 0.5,
      restitution: 0,
      colliders: [
        { id: "authored-foot", shape: "sphere", position: [1, 0, 0], rotation: [0, 0, 0], radius: 0.4 },
      ],
    });
    physics.addStaticObject(
      "crate",
      "box",
      { min: [-1, 0, -1], max: [1, 2, 1] },
      [1004, 0, 2000],
      rotation,
      2,
    );
    physics.step(1 / 60);
    const before = physics.diagnosticColliders();
    expect(before.length).toBe(2);
    expect(before[0].colliderId).toBe("authored-foot");
    near(before[0].position, [1000, 6, 2000]);
    expect(before[1]).toMatchObject({ shape: "box", size: [2, 2, 2] });
    const checkpoint = physics.checkpoint();
    physics.rebase([1024, 0, 2048]);
    const rebased = physics.diagnosticColliders();
    rebased.forEach((collider, index) => {
      near(collider.position, before[index].position);
    });
    physics.setEnabled("actor", false);
    expect(physics.diagnosticColliders().map((collider) => collider.instanceId)).toEqual(["crate"]);
    physics.restore(checkpoint);
    expect(physics.diagnosticColliders()[0].colliderId).toBe("authored-foot");
    near(physics.diagnosticColliders()[0].position, before[0].position);
  } finally {
    physics.dispose();
  }
});
test("rig diagnostics follow posed anchors through scale and body rotation and remain read-only and bounded", async () => {
  const definition = referenceProject().documents.find(
    (document): document is CharacterDefinition => document.kind === "character",
  );
  if (!definition) throw Error("Missing character");
  definition.motions = [];
  definition.joints = [
    {
      id: "root",
      name: "Root",
      parent: null,
      position: [0, 1, 0],
      rotation: [0, 0, 0],
      radius: 1,
      minimum: -Math.PI,
      maximum: Math.PI,
    },
    {
      id: "child",
      name: "Child",
      parent: "root",
      position: [1, 1, 0],
      rotation: [0, 0, 0],
      radius: 0.4,
      minimum: -Math.PI,
      maximum: Math.PI,
    },
  ];
  const runtime = await RuntimeSession.create();
  try {
    runtime.addCharacter(
      "actor",
      compileCharacter(definition, "interactive"),
      definition,
      [10, 0, 20],
      [0, Math.PI / 2, 0],
      2,
    );
    runtime.setPose("actor", "root", [0, 0, Math.PI / 2], [0, 0, 0]);
    const diagnostic = runtimeDiagnostics(runtime),
      bone = diagnostic.segments.find((line) => line.kind === "rig" && line.jointId === "child");
    if (!bone) throw Error("Missing bone");
    near(bone.a, [10, 2, 20]);
    near(bone.b, [10, 4, 20]);
    expect(bone.documentId).toBe(definition.id);
    expect(diagnostic.tick).toBe(0);
    const shifted = runtimeDiagnostics(runtime, { origin: [10, 0, 20] }).segments.find(
      (line) => line.kind === "rig" && line.jointId === "child",
    );
    if (!shifted) throw Error("Missing shifted bone");
    near(shifted.b, [0, 4, 0]);
    const limited = runtimeDiagnostics(runtime, { maxSegments: 3 });
    expect(limited.segments.length).toBe(3);
    expect(limited.truncated).toBe(true);
    expect(runtime.clock.tick).toBe(0);
    expect(runtime.getPose("actor", "root").rotation).toEqual([0, 0, Math.PI / 2]);
  } finally {
    runtime.dispose();
  }
});
