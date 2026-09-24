import { describe, expect, test } from "bun:test";
import { compileCharacter } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import { type CharacterDefinition, creatureSchema, type Joint, type Motion, type Vec3 } from "@wrela/model";
import { poseMatrices, quatFromEuler, quatIdentity, sampleMotion } from "./animation";
import {
  type CreatureRuntimeContext,
  createCreatureRuntimeState,
  creatureJointFrames,
  evaluateCreaturePose,
  layerCreaturePoses,
  solveChainPositions,
  solveCreatureIK,
  solveTwoBonePositions,
  validateCreatureRuntimeState,
} from "./creature-runtime";
import { RuntimeSession } from "./session";

function skeleton(): Joint[] {
  return ["hip", "knee", "foot"].map((id, i) => ({
    id,
    name: id,
    parent: i ? ["hip", "knee"][i - 1] : null,
    position: [i, 0, 0],
    rotation: [0, 0, 0],
    radius: 0.1,
    minimum: -Math.PI,
    maximum: Math.PI,
  }));
}
const chain = {
  id: "leg",
  joints: ["hip", "knee", "foot"],
  target: [1, 1, 0] as Vec3,
  pole: [0, 0, 1] as Vec3,
  weight: 1,
  iterations: 24,
  tolerance: 0.001,
};
const motion: Motion = { id: "stance", name: "stance", duration: 2, loop: true, keys: [] };
const context = (): CreatureRuntimeContext => ({
  position: [0, 0, 0],
  rotation: quatIdentity(),
  scale: 1,
  time: 0.5,
  motion,
  motionTime: 0.5,
});
const secondary = {
  id: "tail",
  joints: chain.joints,
  stiffness: 30,
  damping: 6,
  gravity: [0, -9.81, 0],
  wind: [0, 0, 1],
  maxAngle: 1,
  collisionRadius: 0.03,
  weight: 1,
};

describe("creature pose solvers", () => {
  test("analytic two-bone solution preserves lengths, reaches target and obeys pole", () => {
    const solution = solveTwoBonePositions([0, 0, 0], [1, 0, 0], [2, 0, 0], [1, 1, 0], [0, 0, 1]);
    expect(solution.residual).toBeLessThan(1e-7);
    expect(Math.hypot(...solution.points[1])).toBeCloseTo(1, 6);
    expect(Math.hypot(...solution.points[2].map((v, i) => v - solution.points[1][i]))).toBeCloseTo(1, 6);
    expect(solution.points[1][2]).toBeGreaterThan(0);
    expect(solveTwoBonePositions([0, 0, 0], [1, 0, 0], [2, 0, 0], [8, 0, 0], [0, 1, 0]).residual).toBeCloseTo(
      6,
      5,
    );
  });
  test("FK agrees with skinning and IK supports rotated rest joints and unsorted skeletons", () => {
    const joints = skeleton();
    joints[0].rotation = [0.3, 0.2, -0.5];
    joints[1].rotation = [-0.2, 0.4, 0.1];
    joints.reverse();
    const initial = sampleMotion(joints, undefined, 0);
    const result = solveCreatureIK(joints, initial, chain);
    expect(result.diagnostic.residual).toBeLessThan(0.001);
    const frames = creatureJointFrames(joints, result.pose),
      matrices = poseMatrices(joints, result.pose);
    for (let i = 0; i < joints.length; i++) {
      const p = joints[i].position,
        m = matrices.subarray(i * 16, i * 16 + 16);
      const world = [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
      ];
      for (let axis = 0; axis < 3; axis++)
        expect(world[axis]).toBeCloseTo(frames.get(joints[i].id)!.position[axis], 5);
    }
    expect(initial.get("hip")!.rotation).toEqual(quatIdentity());
  });
  test("unreachable, locked and weighted solves report actual residuals", () => {
    const joints = skeleton(),
      pose = sampleMotion(joints, undefined, 0);
    expect(solveCreatureIK(joints, pose, { ...chain, target: [8, 0, 0] }).diagnostic.status).toBe(
      "unreachable",
    );
    for (const j of joints) {
      j.minimum = 0;
      j.maximum = 0;
    }
    expect(solveCreatureIK(joints, pose, chain).diagnostic.status).toBe("limited");
    const ignored = solveCreatureIK(joints, pose, { ...chain, weight: 0 });
    expect(ignored.pose).toEqual(pose);
  });
  test("Fabrik handles longer connected chains and degenerate reach without NaN", () => {
    const input: Vec3[] = [
      [0, 0, 0],
      [1, 0.1, 0],
      [2, 0, 0],
      [3, 0.1, 0],
      [4, 0, 0],
    ];
    const result = solveChainPositions(input, [1, 2, 0], 64, 0.0001);
    expect(result.residual).toBeLessThan(0.0001);
    expect(result.points[0]).toEqual(input[0]);
    for (let i = 1; i < input.length; i++)
      expect(Math.hypot(...result.points[i].map((v, a) => v - result.points[i - 1][a]))).toBeCloseTo(
        Math.hypot(...input[i].map((v, a) => v - input[i - 1][a])),
        6,
      );
    expect(
      solveChainPositions(
        [
          [0, 0, 0],
          [0, 0, 0],
        ],
        [1, 1, 1],
      )
        .points.flat()
        .every(Number.isFinite),
    ).toBe(true);
  });
  test("masked additive and override layers are ordered and leave input immutable", () => {
    const base = sampleMotion(skeleton(), undefined, 0);
    const layer = new Map([
      ["hip", { translation: [2, 0, 0] as Vec3, rotation: quatFromEuler([0, 0, 1]) }],
      ["knee", { translation: [3, 0, 0] as Vec3, rotation: quatIdentity() }],
    ]);
    const result = layerCreaturePoses(base, [
      { pose: layer, weight: 0.5, mode: "additive", joints: ["hip"] },
    ]);
    expect(result.get("hip")!.translation).toEqual([1, 0, 0]);
    expect(result.get("knee")!.translation).toEqual([0, 0, 0]);
    expect(base.get("hip")!.translation).toEqual([0, 0, 0]);
  });
});

describe("creature contacts and secondary lifecycle", () => {
  test("plants survive root motion and release; rendering does not mutate state", () => {
    const joints = skeleton(),
      pose = sampleMotion(joints, undefined, 0);
    const source = creatureSchema.parse({
      schemaVersion: 1,
      ikChains: [{ ...chain, weight: 0 }],
      contacts: [
        {
          id: "plant",
          motion: "stance",
          joint: "foot",
          start: 0.2,
          end: 1.2,
          target: [1, 1, 0],
          space: "character",
          weight: 1,
          tolerance: 0.01,
          ground: false,
        },
      ],
    });
    const state = createCreatureRuntimeState(),
      first = context();
    evaluateCreaturePose(joints, source, pose, state, first, 1 / 60);
    const target = [...state.contacts.plant.target] as Vec3;
    const moved = { ...first, position: [0.3, 0, 0] as Vec3 };
    const result = evaluateCreaturePose(joints, source, pose, state, moved, 1 / 60);
    expect(state.contacts.plant.target).toEqual(target);
    expect(result.diagnostics.find((d) => d.kind === "contact")!.residual).toBeLessThan(0.01);
    const copy = structuredClone(state);
    for (let i = 0; i < 5; i++) evaluateCreaturePose(joints, source, pose, state, moved);
    expect(state).toEqual(copy);
    evaluateCreaturePose(joints, source, pose, state, { ...moved, motionTime: 1.5 }, 1 / 60);
    expect(state.contacts).toEqual({});
    evaluateCreaturePose(joints, source, pose, state, { ...moved, motionTime: 2.5 }, 1 / 60);
    expect(state.contacts.plant.cycle).toBe(1);
    expect(state.contacts.plant.target[0]).toBeCloseTo(1.3);
  });
  test("ground adaptation and world targets respect scale and report unavailable terrain", () => {
    const joints = skeleton(),
      pose = sampleMotion(joints, undefined, 0);
    const source = creatureSchema.parse({
      schemaVersion: 1,
      ikChains: [{ ...chain, weight: 0 }],
      contacts: [
        {
          id: "plant",
          motion: "stance",
          joint: "foot",
          start: 0,
          end: 1,
          target: [2, 1, 0],
          space: "world",
          weight: 1,
          tolerance: 0.01,
          offset: 0.1,
        },
      ],
    });
    const state = createCreatureRuntimeState();
    const adapted = evaluateCreaturePose(
      joints,
      source,
      pose,
      state,
      { ...context(), scale: 2, ground: () => ({ height: 1, normal: [0, 1, 0] }) },
      1 / 60,
    );
    expect(state.contacts.plant.target[1]).toBe(1.1);
    expect(adapted.diagnostics[0].residual).toBeLessThan(0.01);
    const unavailable = evaluateCreaturePose(
      joints,
      source,
      pose,
      createCreatureRuntimeState(),
      { ...context(), ground: () => undefined },
      1 / 60,
    );
    expect(unavailable.diagnostics[0].status).toBe("unavailable");
  });
  test("secondary simulation replays from state and reports ownership conflicts", () => {
    const joints = skeleton(),
      pose = sampleMotion(joints, undefined, 0);
    const source = creatureSchema.parse({ schemaVersion: 1, secondaryChains: [secondary] });
    const state = createCreatureRuntimeState();
    for (let i = 0; i < 30; i++) evaluateCreaturePose(joints, source, pose, state, context(), 1 / 60);
    expect(state.secondary.tail.positions[1][1]).toBeLessThan(0);
    const checkpoint = structuredClone(state);
    for (let i = 0; i < 30; i++) evaluateCreaturePose(joints, source, pose, state, context(), 1 / 60);
    const restored = validateCreatureRuntimeState(checkpoint, source);
    for (let i = 0; i < 30; i++) evaluateCreaturePose(joints, source, pose, restored, context(), 1 / 60);
    expect(restored).toEqual(state);
    const conflict = evaluateCreaturePose(
      joints,
      { ...source, ikChains: [chain] },
      pose,
      state,
      context(),
      1 / 60,
    );
    expect(conflict.diagnostics.some((d) => d.kind === "ownership" && d.status === "conflict")).toBe(true);
    expect(() =>
      validateCreatureRuntimeState(
        { ...state, secondary: { tail: { positions: [[Number.NaN, 0, 0]], previous: [] } } },
        source,
      ),
    ).toThrow();
  });
  test("runtime replay, save/restore and teleport preserve or reset the correct state", async () => {
    const original = referenceProject().documents.find((d) => d.kind === "character") as CharacterDefinition;
    const definition: CharacterDefinition = {
      ...original,
      joints: skeleton(),
      motions: [motion],
      physics: { ...original.physics, mode: "kinematic" },
      creature: creatureSchema.parse({
        schemaVersion: 1,
        secondaryChains: [secondary],
        expressions: [
          {
            id: "snarl",
            weight: 0,
            weights: [{ joint: "foot", translation: [0, 0, 0], rotation: [0.2, 0, 0] }],
          },
        ],
      }),
    };
    const artifact = {
      ...compileCharacter({ ...definition, creature: undefined }, "interactive"),
      creature: definition.creature,
    };
    const runtime = await RuntimeSession.create();
    runtime.addCharacter("beast", artifact, definition, [0, 5, 0]);
    runtime.setCreatureExpression("beast", "snarl", 0.7);
    for (let i = 0; i < 90; i++) runtime.advance(1 / 60);
    const before = runtime.snapshotEntities(),
      skin = [...runtime.evaluatedCharacters()[0].skinMatrices];
    for (let i = 0; i < 8; i++) runtime.evaluatedCharacters();
    expect(runtime.snapshotEntities()).toEqual(before);
    runtime.seek(90);
    expect(runtime.snapshotEntities()).toEqual(before);
    expect([...runtime.evaluatedCharacters()[0].skinMatrices]).toEqual(skin);
    for (let i = 0; i < 5; i++) runtime.advance(1 / 60);
    runtime.restoreEntityStates(before);
    expect(runtime.snapshotEntities()).toEqual(before);
    await runtime.teleport("beast", [10, 5, 0]);
    const state = JSON.parse(runtime.snapshotEntities()[0].state.creatureState as string);
    expect(state.secondary).toEqual({});
    expect(state.expressions.snarl).toBe(0.7);
    runtime.dispose();
  });
});

test("session ragdoll controls replay and save physical limbs before recovering or teleporting", async () => {
  const original = referenceProject().documents.find((d) => d.kind === "character") as CharacterDefinition;
  const joints = skeleton();
  const definition: CharacterDefinition = {
    ...original,
    joints,
    motions: [motion],
    physics: { ...original.physics, mode: "kinematic" },
    creature: creatureSchema.parse({
      schemaVersion: 1,
      articulation: {
        bodies: joints.map((joint) => ({ joint: joint.id, mass: 1, radius: 0.1 })),
        joints: [
          { id: "hip-knee", parent: "hip", child: "knee", kind: "spherical" },
          { id: "knee-foot", parent: "knee", child: "foot", kind: "spherical" },
        ],
      },
    }),
  };
  const artifact = {
    ...compileCharacter({ ...definition, creature: undefined }, "interactive"),
    creature: definition.creature,
  };
  const runtime = await RuntimeSession.create();
  runtime.physics.addGround();
  runtime.addCharacter("beast", artifact, definition, [0, 3, 0]);
  runtime.advance(1 / 60);
  runtime.enterCreatureRagdoll("beast", [0, 0, 0]);
  runtime.applyCreatureImpulse("beast", "foot", [0.4, 0.1, 0]);
  for (let i = 0; i < 59; i++) runtime.advance(1 / 60);
  expect(runtime.creatureAuthority("beast")).toBe("ragdoll");
  const saved = runtime.snapshotEntities(),
    posed = [...runtime.evaluatedCharacters()[0].skinMatrices];
  const physical = JSON.parse(saved[0].state.articulationState as string);
  const expectPortableState = (actual: ReturnType<RuntimeSession["snapshotEntities"]>) => {
    const restored = JSON.parse(actual[0].state.articulationState as string);
    for (let body = 0; body < physical.bodies.length; body++) {
      // Rapier normalizes rotations when recreating a body, within float32 precision.
      for (let axis = 0; axis < 4; axis++)
        expect(restored.bodies[body].state.rotation[axis]).toBeCloseTo(
          physical.bodies[body].state.rotation[axis],
          6,
        );
      restored.bodies[body].state.rotation = physical.bodies[body].state.rotation;
    }
    expect(restored).toEqual(physical);
    const expected = structuredClone(saved);
    expected[0].state.articulationState = actual[0].state.articulationState;
    expect(actual).toEqual(expected);
  };
  expect(physical.bodies[0].state.position[1]).toBeLessThan(3);
  expect(saved[0].position[0]).toBeCloseTo(physical.bodies[0].state.position[0], 6);
  expect(saved[0].position[0]).not.toBe(0);
  for (let i = 0; i < 8; i++) runtime.evaluatedCharacters();
  expect(runtime.snapshotEntities()).toEqual(saved);
  runtime.seek(60);
  expect(runtime.snapshotEntities()).toEqual(saved);
  expect([...runtime.evaluatedCharacters()[0].skinMatrices]).toEqual(posed);
  for (let i = 0; i < 10; i++) runtime.advance(1 / 60);
  const continuation = runtime.snapshotEntities();
  runtime.seek(60);
  for (let i = 0; i < 10; i++) runtime.advance(1 / 60);
  expect(runtime.snapshotEntities()).toEqual(continuation);
  runtime.restoreEntityStates(saved);
  expectPortableState(runtime.snapshotEntities());
  const dormant = structuredClone(saved);
  dormant[0].state.active = false;
  runtime.restoreEntityStates(dormant);
  expect(runtime.entityLifecycle[0].state).toBe("dormant");
  expect([...runtime.evaluatedCharacters()[0].skinMatrices]).toEqual(posed);
  runtime.restoreEntityStates(saved);
  expect(runtime.creatureAuthority("beast")).toBe("ragdoll");
  expectPortableState(runtime.snapshotEntities());
  const invalid = structuredClone(saved);
  invalid[0].state.articulationState = '{"version":99}';
  expect(() => runtime.restoreEntityStates(invalid)).toThrow();
  expectPortableState(runtime.snapshotEntities());
  runtime.recoverCreature("beast", 0.2);
  for (let i = 0; i < 20; i++) runtime.advance(1 / 60);
  expect(runtime.creatureAuthority("beast")).toBe("animation");
  runtime.enterCreatureRagdoll("beast");
  runtime.advance(1 / 60);
  await runtime.teleport("beast", [3, 3, 0]);
  expect(runtime.creatureAuthority("beast")).toBe("animation");
  runtime.dispose();
});

test("bounded pelvis compensation adapts a planted foot to slopes and reports unresolved reach", () => {
  const joints = skeleton();
  joints[0].position = [0, 2.3, 0];
  joints[1].position = [0, 1.3, 0.1];
  joints[2].position = [0, 0.3, 0];
  const pose = sampleMotion(joints, undefined, 0);
  const source = creatureSchema.parse({
    schemaVersion: 1,
    pelvis: { joint: "hip", maxOffset: [0, 0.5, 0], weight: 1, iterations: 3 },
    ikChains: [{ ...chain, pole: [0, 1, 1], weight: 0 }],
    contacts: [
      {
        id: "slope-plant",
        motion: "stance",
        joint: "foot",
        start: 0,
        end: 1,
        target: [0, 0, 0],
        space: "character",
        weight: 1,
        tolerance: 0.01,
      },
    ],
  });
  const normal: Vec3 = [0, 2 / Math.sqrt(5), 1 / Math.sqrt(5)];
  const groundContext = {
    ...context(),
    position: [10, 1, 5] as Vec3,
    scale: 2,
    rotation: quatFromEuler([0, 0.6, 0]),
    ground: () => ({ height: 1, normal }),
  };
  const state = createCreatureRuntimeState();
  const solved = evaluateCreaturePose(joints, source, pose, state, groundContext, 1 / 60);
  expect(solved.diagnostics.find((d) => d.kind === "pelvis")?.status).toBe("satisfied");
  expect(solved.diagnostics.find((d) => d.kind === "contact")?.residual).toBeLessThan(0.01);
  expect(solved.pose.get("hip")?.translation[1]).toBeLessThan(-0.28);
  expect(solved.pose.get("hip")?.translation[1]).toBeGreaterThan(-0.5);
  const constrained = evaluateCreaturePose(
    joints,
    { ...source, pelvis: { ...source.pelvis!, maxOffset: [0, 0.1, 0] } },
    pose,
    createCreatureRuntimeState(),
    groundContext,
    1 / 60,
  );
  expect(constrained.diagnostics.find((d) => d.kind === "pelvis")?.status).toBe("limited");
  expect(constrained.diagnostics.find((d) => d.kind === "contact")?.residual).toBeGreaterThan(0.3);
  const unavailable = evaluateCreaturePose(
    joints,
    source,
    pose,
    state,
    { ...groundContext, ground: () => undefined },
    1 / 60,
  );
  expect(unavailable.diagnostics.some((d) => d.kind === "contact" && d.status === "unavailable")).toBe(true);
});
