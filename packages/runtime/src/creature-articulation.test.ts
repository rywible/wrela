import { describe, expect, test } from "bun:test";
import { creatureArticulationSchema, type Joint, type Quat } from "@wrela/model";

import { type Pose, quatIdentity, quatMultiply } from "./animation";
import {
  CreatureArticulationController,
  resolveCreatureArticulationSnapshot,
  validateCreatureArticulationSnapshot,
} from "./creature-articulation";
import { type CreatureRuntimeContext, creatureJointFrames } from "./creature-runtime";
import { PhysicsAdapter } from "./physics";

const joints: Joint[] = [
  {
    id: "hip",
    name: "Hip",
    parent: null,
    position: [0, 1, 0],
    rotation: [0, 0, 0],
    radius: 0.2,
    minimum: -Math.PI,
    maximum: Math.PI,
  },
  {
    id: "knee",
    name: "Knee",
    parent: "hip",
    position: [0, 0.4, 0],
    rotation: [0, 0, 0],
    radius: 0.2,
    minimum: -Math.PI,
    maximum: Math.PI,
  },
];
const source = creatureArticulationSchema.parse({
  bodies: [
    { joint: "hip", mass: 3, radius: 0.2 },
    { joint: "knee", mass: 1, radius: 0.2 },
  ],
  joints: [
    {
      id: "hinge",
      parent: "hip",
      child: "knee",
      kind: "revolute",
      axis: [0, 0, 1],
      minimum: -0.25,
      maximum: 0.25,
    },
  ],
});
const context: CreatureRuntimeContext = {
  position: [0, 1, 0],
  rotation: quatIdentity(),
  scale: 1,
  time: 0,
  motionTime: 0,
};
const pose: Pose = new Map();
async function fixture(gravity: [number, number, number] = [0, -9.81, 0]) {
  const physics = await PhysicsAdapter.create(gravity);
  physics.add({
    id: "actor",
    position: [0, 1.7, 0],
    radius: 0.3,
    halfHeight: 0.5,
    mass: 4,
    friction: 0.8,
    restitution: 0,
    mode: "kinematic",
  });
  const controller = new CreatureArticulationController(physics, "actor", joints, source, context, pose);
  return { physics, controller };
}

describe("articulated creature physics", () => {
  test("same-world ground collision and constrained impacts move the visible skeleton", async () => {
    const { physics, controller } = await fixture();
    try {
      physics.addGround();
      controller.enterRagdoll();
      controller.impulse("knee", [0.8, 0, 0]);
      for (let i = 0; i < 180; i++) physics.step(1 / 60);
      const snapshot = controller.snapshot();
      for (const { state } of snapshot.bodies) {
        expect(state.position[1]).toBeGreaterThan(0.17);
        expect(state.position.every(Number.isFinite)).toBe(true);
      }
      expect(controller.diagnostics()[0].residual).toBeLessThan(0.02);
      const resolved = creatureJointFrames(joints, controller.resolvePose(pose, context));
      const hip = snapshot.bodies.find((body) => body.joint === "hip");
      if (!hip) throw new Error("Missing hip body");
      expect((resolved.get("hip")?.position[1] ?? NaN) + context.position[1]).toBeCloseTo(
        hip.state.position[1],
        5,
      );
      expect(physics.diagnosticColliders().filter((value) => value.instanceId === "actor").length).toBe(2);
      expect(physics.drainContactEvents().some((event) => event.a === "actor" || event.b === "actor")).toBe(
        true,
      );
    } finally {
      controller.dispose();
      physics.dispose();
    }
  });
  test("hinge limits resist torque rather than merely clamping rendered pose", async () => {
    const { physics, controller } = await fixture([0, 0, 0]);
    try {
      controller.enterRagdoll();
      controller.impulse("knee", [0, 0, 0], [0, 0, 0.03]);
      for (let i = 0; i < 180; i++) physics.step(1 / 60);
      const bodies = controller.snapshot().bodies;
      const hip = bodies.find((body) => body.joint === "hip")?.state.rotation;
      const knee = bodies.find((body) => body.joint === "knee")?.state.rotation;
      if (!hip || !knee) throw new Error("Missing bodies");
      const inverse: Quat = [-hip[0], -hip[1], -hip[2], hip[3]];
      const relative = quatMultiply(inverse, knee);
      const angle = 2 * Math.atan2(Math.abs(relative[2]), Math.abs(relative[3]));
      expect(angle).toBeLessThan(0.3);
      expect(controller.diagnostics()[0].residual).toBeLessThan(0.01);
    } finally {
      controller.dispose();
      physics.dispose();
    }
  });
  test("animation velocity transfers into ragdoll and read-only pose queries do not advance recovery", async () => {
    const { physics, controller } = await fixture([0, 0, 0]);
    try {
      const moved = { ...context, position: [0.1, 1, 0] as [number, number, number] };
      controller.updateTargets(pose, moved, 0.1);
      physics.step(0.1);
      controller.enterRagdoll();
      expect(controller.snapshot().bodies[0].state.velocity[0]).toBeCloseTo(1, 4);
      controller.recover(0.2);
      const before = controller.snapshot();
      for (let i = 0; i < 10; i++) controller.resolvePose(pose, moved);
      expect(controller.snapshot()).toEqual(before);
      for (let i = 0; i < 15; i++) {
        controller.updateTargets(pose, moved, 1 / 60);
        physics.step(1 / 60);
      }
      expect(controller.authority).toBe("animation");
      expect(controller.resolvePose(pose, moved)).toBe(pose);
    } finally {
      controller.dispose();
      physics.dispose();
    }
  });
  test("shared-world checkpoints and portable limb snapshots preserve authority and velocities", async () => {
    const { physics, controller } = await fixture();
    let restored: CreatureArticulationController | undefined;
    try {
      physics.addGround();
      controller.enterRagdoll();
      controller.impulse("knee", [0.2, 0.1, 0]);
      for (let i = 0; i < 10; i++) physics.step(1 / 60);
      const saved = controller.snapshot();
      const checkpoint = physics.checkpoint();
      for (let i = 0; i < 10; i++) physics.step(1 / 60);
      const continued = controller.snapshot();
      physics.restore(checkpoint);
      restored = CreatureArticulationController.restoreFromCheckpoint(
        physics,
        "actor",
        joints,
        source,
        context,
        pose,
        saved,
      );
      expect(restored.snapshot()).toEqual(saved);
      for (let i = 0; i < 10; i++) physics.step(1 / 60);
      expect(restored.snapshot()).toEqual(continued);
      physics.restore(checkpoint);
      restored = CreatureArticulationController.restoreFromCheckpoint(
        physics,
        "actor",
        joints,
        source,
        context,
        pose,
        saved,
      );
      const portable = JSON.parse(JSON.stringify(saved));
      restored.dispose();
      restored = CreatureArticulationController.restoreFromCheckpoint(
        physics,
        "actor",
        joints,
        source,
        context,
        pose,
        portable,
      );
      expect(restored.snapshot()).toEqual(saved);
      physics.rebase([1000, 0, -1000]);
      const after = restored.snapshot();
      expect(after.bodies[0].state.position[0]).toBeCloseTo(saved.bodies[0].state.position[0], 3);
      expect(restored.diagnostics()[0].residual).toBeLessThan(0.02);
    } finally {
      restored?.dispose();
      physics.dispose();
    }
  });
  test("malformed portable states reject before allocation or shared-world mutation", async () => {
    const { physics, controller } = await fixture();
    try {
      const state = controller.snapshot();
      const malformed = structuredClone(state);
      malformed.bodies[0].state.rotation = [0, 0, 0, 0];
      expect(() => validateCreatureArticulationSnapshot(malformed, source)).toThrow();
      expect(() =>
        CreatureArticulationController.restoreFromCheckpoint(
          physics,
          "invalid",
          joints,
          source,
          context,
          pose,
          malformed,
        ),
      ).toThrow();
      expect(physics.hasBody("invalid::articulation::hip")).toBe(false);
      expect(controller.snapshot()).toEqual(state);
    } finally {
      controller.dispose();
      physics.dispose();
    }
  });
  test("dormant snapshots preserve the active physical pose after all bodies are removed", async () => {
    const { physics, controller } = await fixture();
    try {
      controller.enterRagdoll();
      for (let i = 0; i < 20; i++) physics.step(1 / 60);
      const expected = controller.resolvePose(pose, context),
        saved = controller.snapshot();
      controller.dispose();
      expect(resolveCreatureArticulationSnapshot(joints, source, saved, pose, context)).toEqual(expected);
      expect(physics.hasBody("actor::articulation::hip")).toBe(false);
    } finally {
      physics.dispose();
    }
  });
});
