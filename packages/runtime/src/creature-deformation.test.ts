import { expect, test } from "bun:test";
import { type CompiledCharacter, creatureSchema, type Vec3 } from "@wrela/model";

import { poseMatrices, quatFromEuler, quatIdentity, sampleMotion } from "./animation";
import { evaluateCreatureDeformation, mergeCreatureDeformation } from "./creature-deformation";
import {
  createCreatureGroomState,
  creatureGroomOffsets,
  stepCreatureGroom,
  validateCreatureGroomState,
} from "./creature-groom";
import type { CreatureRuntimeContext } from "./creature-runtime";

function fixture(): CompiledCharacter {
  const mesh = {
    positions: new Float32Array([0, 0, 0, 1, 0, 0, 1, 0, 0.1]),
    normals: new Float32Array([0, -1, 0, 0, -1, 0, 0, -1, 0]),
    indices: new Uint32Array([0, 1, 2]),
    bounds: { min: [0, 0, 0] as Vec3, max: [1, 0, 0.1] as Vec3 },
  };
  return {
    kind: "character",
    id: "coat",
    key: "coat-1",
    mesh,
    material: "fur",
    joints: [
      {
        id: "root",
        name: "root",
        parent: null,
        position: [0, 0, 0],
        rotation: [0, 0, 0],
        radius: 0.1,
        minimum: -Math.PI,
        maximum: Math.PI,
      },
    ],
    motions: [],
    jointIndices: new Uint16Array(12),
    weights: new Float32Array([1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]),
    diagnostics: [],
    creature: creatureSchema.parse({ schemaVersion: 1 }),
    creatureBodyVertexCount: 0,
    creatureGroom: {
      key: "groom",
      representation: "opaque-tufts",
      diagnostics: [],
      guides: [
        {
          root: {
            id: "guide",
            layer: "mane",
            region: "head",
            chart: "head",
            chartRevision: 0,
            coordinates: [0, 0, 1],
          },
          points: [
            [0, 0, 0],
            [0.5, 0, 0],
            [1, 0, 0],
          ],
          normal: [0, 1, 0],
          width: 0.1,
          taper: 0.1,
          material: "fur",
          rootColor: [1, 1, 1],
          tipColor: [1, 1, 1],
          stiffness: 20,
          damping: 8,
        },
      ],
      details: [
        {
          label: "hero",
          mesh,
          vertexGuideIndices: new Uint32Array([0, 0, 0]),
          guideIds: ["guide"],
          cost: { vertices: 3, triangles: 1, bytes: 84, guides: 1 },
          maxError: null,
        },
      ],
    },
  };
}
const context: CreatureRuntimeContext = {
  position: [0, 2, 0],
  rotation: quatIdentity(),
  scale: 1,
  time: 0,
  motionTime: 0,
};

test("corrective deltas are cached, restore at zero weight, and never mutate source mesh", () => {
  const artifact = fixture();
  artifact.creatureCorrectives = [
    {
      id: "crease",
      region: "head",
      joint: "root",
      axis: "z",
      angle: 1,
      vertices: new Uint32Array([0]),
      displacements: new Float32Array([0, 0, 0.2]),
    },
  ];
  const pose = sampleMotion(artifact.joints, undefined, 0);
  pose.set("root", { translation: [0, 0, 0], rotation: quatFromEuler([0, 0, 0.5]) });
  const first = evaluateCreatureDeformation(artifact, pose);
  expect(first.deformation?.positionDeltas[2]).toBeCloseTo(0.1, 6);
  expect(first.deformation?.maxDisplacement).toBeCloseTo(0.1, 6);
  expect(evaluateCreatureDeformation(artifact, pose, first)).toBe(first);
  expect(artifact.mesh.positions[2]).toBe(0);
  expect(
    evaluateCreatureDeformation(artifact, sampleMotion(artifact.joints, undefined, 0), first).deformation,
  ).toBeUndefined();
});

test("groom guides move with gravity, preserve roots and replay without capture side effects", () => {
  const artifact = fixture(),
    matrices = poseMatrices(artifact.joints, sampleMotion(artifact.joints, undefined, 0)),
    state = createCreatureGroomState();
  for (let i = 0; i < 30; i++) stepCreatureGroom(artifact, matrices, state, context, 1 / 60);
  expect(state.guides[0].positions[0]).toEqual([0, 2, 0]);
  const checkpoint = structuredClone(state),
    offsets = creatureGroomOffsets(artifact, matrices, state, context);
  expect(offsets?.[4]).toBeLessThan(0);
  expect(Array.from(offsets?.slice(0, 3) ?? [])).toEqual([0, 0, 0]);
  for (let i = 0; i < 5; i++) creatureGroomOffsets(artifact, matrices, state, context);
  expect(state).toEqual(checkpoint);
  const restored = validateCreatureGroomState(checkpoint, artifact);
  for (let i = 0; i < 30; i++) {
    stepCreatureGroom(artifact, matrices, state, context, 1 / 60);
    stepCreatureGroom(artifact, matrices, restored, context, 1 / 60);
  }
  expect(state).toEqual(restored);
});

test("groom inverse skinning yields finite offsets with rotated/scaled moving roots", () => {
  const artifact = fixture(),
    pose = sampleMotion(artifact.joints, undefined, 0);
  pose.set("root", { translation: [0, 0, 0], rotation: quatFromEuler([0, 0, 0.4]) });
  const matrices = poseMatrices(artifact.joints, pose),
    state = createCreatureGroomState();
  const transformed = {
    ...context,
    position: [3, 2, 0] as Vec3,
    scale: 2,
    rotation: quatFromEuler([0, 0.7, 0]),
  };
  stepCreatureGroom(artifact, matrices, state, transformed, 1 / 60);
  const offsets = creatureGroomOffsets(artifact, matrices, state, transformed);
  expect(offsets?.every(Number.isFinite)).toBe(true);
  expect(Array.from(offsets?.slice(0, 3) ?? [])).toEqual([0, 0, 0]);
  const invalid = structuredClone(state);
  invalid.guides[0].positions[0][0] = Number.NaN;
  expect(() => validateCreatureGroomState(invalid, artifact)).toThrow();
});

test("merged deformation keeps correct normal directions and conservative displacement", () => {
  const artifact = fixture(),
    offsets = new Float32Array(artifact.mesh.positions.length);
  offsets[4] = 0.3;
  const merged = mergeCreatureDeformation(artifact, undefined, [offsets], "moved");
  expect(merged?.maxDisplacement).toBeCloseTo(0.3, 6);
  expect(merged?.normalDeltas?.every(Number.isFinite)).toBe(true);
  expect(merged?.normalDeltas?.[0]).not.toBe(0);
  expect(artifact.mesh.positions[4]).toBe(0);
});
