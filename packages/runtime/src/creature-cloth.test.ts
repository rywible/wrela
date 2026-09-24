import { describe, expect, test } from "bun:test";
import { type CompiledCharacter, creatureSchema, type Vec3 } from "@wrela/model";

import {
  type CreatureClothContext,
  createCreatureClothState,
  creatureClothDiagnostics,
  creatureClothOffsets,
  resetCreatureCloth,
  stepCreatureCloth,
  validateCreatureClothState,
} from "./creature-cloth";

function fixture() {
  const positions: number[] = [],
    indices: number[] = [],
    coordinates: NonNullable<CompiledCharacter["creatureCoordinates"]> = [];
  for (let v = 0; v < 3; v++)
    for (let u = 0; u < 3; u++) {
      positions.push(u / 2, 2, v / 2);
      coordinates.push({ region: "cloth", chart: "panel", chartRevision: 1, coordinates: [u / 2, v / 2, 0] });
    }
  for (let v = 0; v < 2; v++)
    for (let u = 0; u < 2; u++) {
      const a = v * 3 + u,
        b = a + 1,
        c = a + 3,
        d = c + 1;
      indices.push(a, c, b, b, c, d);
    }
  const weights = new Float32Array(9 * 4);
  for (let i = 0; i < 9; i++) weights[i * 4] = 1;
  const artifact: CompiledCharacter = {
    kind: "character",
    id: "cloth-creature",
    key: "cloth-test",
    material: "cloth-material",
    mesh: {
      positions: new Float32Array(positions),
      normals: new Float32Array(positions.map((_, i) => (i % 3 === 1 ? 1 : 0))),
      indices: new Uint32Array(indices),
      bounds: { min: [0, 2, 0], max: [1, 2, 1] },
    },
    joints: [
      {
        id: "root",
        name: "Root",
        parent: null,
        position: [0, 0, 0],
        rotation: [0, 0, 0],
        radius: 0.1,
        minimum: -Math.PI,
        maximum: Math.PI,
      },
    ],
    motions: [],
    jointIndices: new Uint16Array(9 * 4),
    weights,
    diagnostics: [],
    creatureCoordinates: coordinates,
    creature: creatureSchema.parse({
      schemaVersion: 1,
      regions: [
        {
          id: "cloth",
          name: "Cloth",
          nodeIds: [],
          jointIds: ["root"],
          frame: { position: [0, 0, 0], rotation: [0, 0, 0] },
          extent: [1, 1, 1],
        },
      ],
      charts: [
        {
          id: "panel",
          region: "cloth",
          kind: "patch",
          revision: 1,
          points: [
            [0, 2, 0],
            [1, 2, 0],
            [0, 2, 1],
            [1, 2, 1],
          ],
          thickness: 0.01,
        },
      ],
      cloth: [
        {
          id: "drape",
          region: "cloth",
          chart: "panel",
          chartRevision: 1,
          pinEdges: ["v0"],
          iterations: 16,
          maxStretch: 1.05,
          bendStiffness: 0.02,
          damping: 0.1,
        },
      ],
    }),
  };
  const matrices = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
  const context: CreatureClothContext = {
    position: [0, 0, 0],
    rotation: [0, 0, 0, 1],
    scale: 1,
    time: 0,
    motionTime: 0,
  };
  return { artifact, matrices, context, state: createCreatureClothState() };
}
const length = (a: Vec3, b: Vec3) => Math.hypot(...a.map((v, i) => v - b[i]));

describe("cloth patch simulation", () => {
  test("gravity bends the patch while authored top edge remains pinned and stretch bounded", () => {
    const { artifact, matrices, context, state } = fixture();
    for (let i = 0; i < 120; i++) stepCreatureCloth(artifact, matrices, state, context, 1 / 120);
    const particles = state.panels[0].positions;
    for (let i = 0; i < 3; i++) expect(particles[i]).toEqual([i / 2, 2, 0]);
    expect(particles[7][1]).toBeLessThan(1.95);
    const edges = new Set<string>();
    for (let t = 0; t < artifact.mesh.indices.length; t += 3) {
      const [a, b, c] = Array.from(artifact.mesh.indices.slice(t, t + 3));
      for (const pair of [
        [a, b],
        [b, c],
        [c, a],
      ])
        edges.add(pair.sort((x, y) => x - y).join(":"));
    }
    for (const edge of edges) {
      const [a, b] = edge.split(":").map(Number),
        restA: Vec3 = [(a % 3) / 2, 2, Math.floor(a / 3) / 2],
        restB: Vec3 = [(b % 3) / 2, 2, Math.floor(b / 3) / 2];
      expect(length(particles[a], particles[b]) / length(restA, restB)).toBeLessThan(1.06);
    }
  });
  test("pinned edge follows animated skeleton transport and returned deltas are pre-skin", () => {
    const { artifact, matrices, context, state } = fixture();
    matrices[12] = 0.7;
    stepCreatureCloth(artifact, matrices, state, context, 1 / 60);
    expect(state.panels[0].positions[0][0]).toBeCloseTo(0.7, 6);
    const before = JSON.stringify(state),
      offsets = creatureClothOffsets(artifact, matrices, state, context);
    expect(offsets).toBeDefined();
    expect(offsets?.[0]).toBeCloseTo(0, 7);
    expect(offsets?.[1]).toBeCloseTo(0, 7);
    expect(JSON.stringify(state)).toBe(before);
  });
  test("sphere, capsule, and ground constraints keep free particles outside obstacles", () => {
    const { artifact, matrices, context, state } = fixture();
    context.colliders = [{ center: [0.5, 1.8, 1], radius: 0.25 }];
    context.capsules = [{ a: [0.1, 1.75, 0.5], b: [0.9, 1.75, 0.5], radius: 0.2 }];
    context.ground = () => ({ height: 1.2, normal: [0, 1, 0] });
    for (let i = 0; i < 60; i++) stepCreatureCloth(artifact, matrices, state, context, 1 / 120);
    for (const p of state.panels[0].positions.slice(3)) {
      expect(length(p, [0.5, 1.8, 1])).toBeGreaterThanOrEqual(0.259999);
      const nearest: Vec3 = [Math.min(0.9, Math.max(0.1, p[0])), 1.75, 0.5];
      expect(length(p, nearest)).toBeGreaterThanOrEqual(0.209999);
      expect(p[1]).toBeGreaterThanOrEqual(1.209999);
    }
  });
  test("checkpoint restoration reproduces dynamics and reset removes stale velocity", () => {
    const { artifact, matrices, context, state } = fixture();
    for (let i = 0; i < 20; i++) stepCreatureCloth(artifact, matrices, state, context, 1 / 120);
    const restored = validateCreatureClothState(JSON.parse(JSON.stringify(state)), artifact);
    for (let i = 0; i < 20; i++) {
      stepCreatureCloth(artifact, matrices, state, context, 1 / 120);
      stepCreatureCloth(artifact, matrices, restored, context, 1 / 120);
    }
    expect(restored).toEqual(state);
    resetCreatureCloth(state);
    context.position = [10, 0, 0];
    stepCreatureCloth(artifact, matrices, state, context, 1 / 120);
    expect(state.panels[0].positions[0]).toEqual([10, 2, 0]);
    expect(state.panels[0].positions.every((p) => p[0] >= 9.9)).toBe(true);
  });
  test("fixed substeps agree when elapsed time is partitioned differently", () => {
    const a = fixture(),
      b = fixture();
    for (let i = 0; i < 10; i++) stepCreatureCloth(a.artifact, a.matrices, a.state, a.context, 1 / 60);
    for (let i = 0; i < 20; i++) stepCreatureCloth(b.artifact, b.matrices, b.state, b.context, 1 / 120);
    expect(a.state.panels).toEqual(b.state.panels);
  });
  test("explicit surface anchors transport pinned chart particles", () => {
    const { artifact, matrices, context, state } = fixture();
    const cloth = artifact.creature?.cloth[0];
    if (!cloth) throw new Error("fixture");
    cloth.pinEdges = [];
    cloth.pins = [{ coordinates: [0, 0], anchor: "mount" }];
    artifact.creatureAnchors = [
      {
        id: "mount",
        status: "resolved",
        position: [0.1, 2.1, 0],
        normal: [0, 1, 0],
        residual: 0,
        confidence: 1,
        diagnostics: [],
      },
    ];
    stepCreatureCloth(artifact, matrices, state, context, 1 / 120);
    expect(state.panels[0].positions[0]).toEqual([0.1, 2.1, 0]);
  });
  test("state validation rejects nonfinite values, stale topology, and oversized restore", () => {
    const { artifact, matrices, context, state } = fixture();
    stepCreatureCloth(artifact, matrices, state, context, 1 / 120);
    let bad = structuredClone(state);
    bad.panels[0].positions[0][0] = Infinity;
    expect(() => validateCreatureClothState(bad, artifact)).toThrow();
    bad = structuredClone(state);
    bad.panels[0].key = "stale";
    expect(() => validateCreatureClothState(bad, artifact)).toThrow();
    bad = structuredClone(state);
    bad.panels.push(structuredClone(bad.panels[0]));
    expect(() => validateCreatureClothState(bad, artifact)).toThrow();
    expect(() => stepCreatureCloth(artifact, matrices, state, context, NaN)).toThrow();
  });
  test("soft authored pins retain controllable compliance instead of silently becoming fixed", () => {
    const { artifact, matrices, context, state } = fixture();
    const cloth = artifact.creature?.cloth[0];
    if (!cloth) throw new Error("fixture");
    cloth.pinEdges = [];
    cloth.pins = [{ coordinates: [0, 0], anchor: "mount", weight: 0.15 }];
    artifact.creatureAnchors = [
      {
        id: "mount",
        status: "resolved",
        position: [0, 2, 0],
        normal: [0, 1, 0],
        residual: 0,
        confidence: 1,
        diagnostics: [],
      },
    ];
    for (let i = 0; i < 30; i++) stepCreatureCloth(artifact, matrices, state, context, 1 / 120);
    expect(state.panels[0].positions[0][1]).toBeLessThan(2);
    expect(state.panels[0].positions[0][1]).toBeGreaterThan(1.8);
  });
  test("inspection reports unsatisfied constraint residuals without changing simulation", () => {
    const { artifact, matrices, context, state } = fixture();
    stepCreatureCloth(artifact, matrices, state, context, 1 / 120);
    state.panels[0].positions[8][0] += 10;
    const before = JSON.stringify(state),
      report = creatureClothDiagnostics(artifact, matrices, state, context);
    expect(report[0].stretchResidual).toBeGreaterThan(1);
    expect(report[0].maximumPinError).toBe(0);
    expect(JSON.stringify(state)).toBe(before);
  });
  test("dense display geometry follows a bounded independent physics lattice without losing rest detail", () => {
    const { artifact, matrices, context, state } = fixture(),
      n = 12;
    const positions: number[] = [],
      indices: number[] = [],
      coordinates: NonNullable<CompiledCharacter["creatureCoordinates"]> = [];
    for (let v = 0; v <= n; v++)
      for (let u = 0; u <= n; u++) {
        positions.push(u / n, 2, v / n);
        coordinates.push({
          region: "cloth",
          chart: "panel",
          chartRevision: 1,
          coordinates: [u / n, v / n, 0],
        });
      }
    for (let v = 0; v < n; v++)
      for (let u = 0; u < n; u++) {
        const a = v * (n + 1) + u,
          b = a + 1,
          c = a + n + 1,
          d = c + 1;
        indices.push(a, c, b, b, c, d);
      }
    artifact.mesh = {
      ...artifact.mesh,
      positions: new Float32Array(positions),
      normals: new Float32Array(positions.map((_, i) => (i % 3 === 1 ? 1 : 0))),
      indices: new Uint32Array(indices),
    };
    artifact.creatureCoordinates = coordinates;
    artifact.weights = new Float32Array(coordinates.length * 4);
    artifact.jointIndices = new Uint16Array(coordinates.length * 4);
    for (let i = 0; i < coordinates.length; i++) artifact.weights[i * 4] = 1;
    const cloth = artifact.creature?.cloth[0];
    if (!cloth) throw new Error("fixture");
    cloth.gravity = [0, 0, 0];
    stepCreatureCloth(artifact, matrices, state, context, 1 / 120);
    expect(state.panels[0].positions).toHaveLength(25);
    const rest = creatureClothOffsets(artifact, matrices, state, context);
    expect(rest?.every((value) => Math.abs(value) < 1e-7)).toBe(true);
    for (const particle of state.panels[0].positions) particle[1] -= 0.3 * particle[2];
    const offsets = creatureClothOffsets(artifact, matrices, state, context);
    if (!offsets) throw new Error("offsets");
    for (let i = 0; i < coordinates.length; i++)
      expect(offsets[i * 3 + 1]).toBeCloseTo(-0.3 * positions[i * 3 + 2], 6);
  });
});
