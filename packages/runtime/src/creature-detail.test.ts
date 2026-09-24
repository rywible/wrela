import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import {
  type Camera,
  type CharacterDefinition,
  type CompiledCharacter,
  type CreatureDetail,
  creatureSchema,
  identityMatrix,
  type MeshData,
  type Vec3,
} from "@wrela/model";
import { CreatureDetailSelector, creatureDeformedBounds, projectedCreatureDiameter } from "./creature-detail";
import { artifactBytes, BrowserSceneHost } from "./scene-host";
import { RuntimeSession } from "./session";

function fixture() {
  const project = referenceProject(),
    definition = project.documents.find((d) => d.kind === "character") as CharacterDefinition;
  definition.joints = [
    {
      id: "root",
      name: "root",
      parent: null,
      position: [0, 0, 0],
      rotation: [0, 0, 0],
      radius: 0.2,
      minimum: -Math.PI,
      maximum: Math.PI,
    },
    {
      id: "hinge",
      name: "hinge",
      parent: "root",
      position: [0, 1, 0],
      rotation: [0, 0, 0],
      radius: 0.2,
      minimum: -Math.PI,
      maximum: Math.PI,
    },
  ];
  definition.motions = [
    {
      id: "flex",
      name: "Flex",
      duration: 1,
      loop: true,
      keys: [
        { joint: "hinge", time: 0, rotation: [1, 0, 0], translation: [0, 0, 0] },
        { joint: "hinge", time: 1, rotation: [1, 0, 0], translation: [0, 0, 0] },
      ],
    },
  ];
  definition.creature = creatureSchema.parse({ schemaVersion: 1 });
  definition.physics = { mode: "kinematic", mass: 1, friction: 0.5, restitution: 0 };
  const mesh: MeshData = {
    positions: new Float32Array([-1, 0, -0.1, 1, 0, -0.1, 1, 2, 0.1, -1, 2, 0.1]),
    normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1]),
    indices: new Uint32Array([0, 1, 2, 0, 2, 3]),
    bounds: { min: [-1, 0, -0.1], max: [1, 2, 0.1] },
  };
  const distant: MeshData = {
    ...mesh,
    positions: new Float32Array([-1, 0, -0.1, 1, 0, -0.1, 0, 2, 0.1]),
    normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1]),
    indices: new Uint32Array([0, 1, 2]),
  };
  const weights = (count: number) =>
    new Float32Array(Array.from({ length: count * 4 }, (_, i) => (i % 4 === 0 ? 1 : 0)));
  const joints = (count: number) =>
    new Uint16Array(Array.from({ length: count * 4 }, (_, i) => (i % 4 === 0 ? 1 : 0)));
  const corrective = (index: number) => ({
    id: "bulge",
    region: "body",
    joint: "hinge",
    axis: "x" as const,
    angle: 1,
    vertices: new Uint32Array([index]),
    displacements: new Float32Array([0.1, 0.2, 0]),
  });
  const details: CreatureDetail[] = [
    {
      label: "medium",
      mesh: distant,
      weights: weights(3),
      jointIndices: joints(3),
      maxProjectedDiameter: 320,
      maxError: null,
      correctives: [corrective(2)],
    },
    {
      label: "distant",
      mesh: { ...distant, positions: distant.positions.slice() },
      weights: weights(3),
      jointIndices: joints(3),
      maxProjectedDiameter: 128,
      maxError: null,
      correctives: [corrective(1)],
    },
  ];
  const artifact: CompiledCharacter = {
    kind: "character",
    id: definition.id,
    key: "creature-detail-test",
    mesh,
    material: definition.material,
    joints: definition.joints,
    motions: definition.motions,
    weights: weights(4),
    jointIndices: joints(4),
    creature: definition.creature,
    creatureCorrectives: [corrective(3)],
    creatureDetails: details,
    diagnostics: [],
  };
  return { project, definition, artifact, details };
}
const camera = (distance: number): Camera => ({ position: [0, 0, distance], target: [0, 0, 0], fov: 60 });
function cameraForPixels(artifact: CompiledCharacter, pixels: number, viewport = 720): Camera {
  let near = 0.01,
    far = 10000;
  for (let i = 0; i < 64; i++) {
    const middle = (near + far) / 2;
    if (projectedCreatureDiameter(artifact, [0, 0, 0], 1, camera(middle), viewport) > pixels) near = middle;
    else far = middle;
  }
  return camera((near + far) / 2);
}
test("creature detail selection uses projected diameter with stable actor-specific hysteresis", () => {
  const { artifact } = fixture(),
    selector = new CreatureDetailSelector();
  const select = (id: string, pixels: number) =>
    selector.select(id, artifact, [0, 0, 0], 1, cameraForPixels(artifact, pixels));
  expect(select("walker", 450).detail).toBeUndefined();
  expect(select("walker", 300).detail).toBeUndefined(); // retain hero until below 272 px
  expect(select("walker", 260).detail?.label).toBe("medium");
  expect(select("walker", 340).detail?.label).toBe("medium"); // retain medium until above 368 px
  expect(select("walker", 375).detail).toBeUndefined();
  expect(select("far-actor", 100).detail?.label).toBe("distant");
  expect(select("walker", 300).detail).toBeUndefined(); // different actor's state did not leak
  expect(select("far-actor", 140).detail?.label).toBe("distant");
  expect(select("far-actor", 155).detail?.label).toBe("medium");
  expect(selector.retainedActors).toBe(2);
});
test("viewport pixels, scale and posed motion affect choice without changing source artifacts", () => {
  const { artifact } = fixture(),
    selector = new CreatureDetailSelector(),
    view = cameraForPixels(artifact, 250);
  expect(selector.select("small-frame", artifact, [0, 0, 0], 1, view, 720).detail?.label).toBe("medium");
  const big = selector.select("large-frame", artifact, [0, 0, 0], 1, view, 1440);
  expect(big.detail).toBeUndefined();
  expect(big.metadata.viewportHeight).toBe(1440);
  expect(big.metadata.maxError).toBeNull();
  expect(big.metadata.bounds).toBe("source-envelope-estimate");
  const transform = identityMatrix();
  transform[12] = 12;
  expect(selector.select("extended", artifact, [0, 0, 0], 1, view, 720, transform).detail).toBeUndefined();
  expect(artifact.mesh.positions).toHaveLength(12);
  expect(artifact.creatureDetails?.[0].mesh.positions).toHaveLength(9);
  expect(projectedCreatureDiameter(artifact, [0, 0, 0], 2, view)).toBeGreaterThan(
    projectedCreatureDiameter(artifact, [0, 0, 0], 1, view),
  );
  expect(() => selector.select("bad", artifact, [0, 0, 0], 1, view, 0)).toThrow("Viewport");
});
test("selector state is bounded and artifact replacement resets a stale hysteresis choice", () => {
  const { artifact } = fixture(),
    selector = new CreatureDetailSelector(0.15, 2);
  selector.select("a", artifact, [0, 0, 0], 1, cameraForPixels(artifact, 450));
  selector.select("b", artifact, [0, 0, 0], 1, camera(100));
  selector.select("c", artifact, [0, 0, 0], 1, camera(100));
  expect(selector.retainedActors).toBe(2);
  expect(selector.inspect("a")).toBeUndefined();
  selector.select("c", artifact, [0, 0, 0], 1, cameraForPixels(artifact, 400));
  expect(
    selector.select("c", { ...artifact, key: "new-source" }, [0, 0, 0], 1, cameraForPixels(artifact, 300))
      .detail?.label,
  ).toBe("medium");
  selector.clear();
  expect(selector.retainedActors).toBe(0);
});
test("current pose bounds conservatively contain blended skin motion plus bounded local deformation", () => {
  const matrices = new Float32Array(32);
  matrices.set(identityMatrix());
  matrices.set(identityMatrix(), 16);
  matrices[12] = -3;
  matrices[16 + 12] = 5;
  matrices[16 + 13] = 2;
  const bounds = creatureDeformedBounds({ min: [-1, -1, -1], max: [1, 1, 1] }, matrices, 0.4);
  for (const blend of [0, 0.1, 0.5, 0.9, 1])
    for (const point of [
      [-1.4, -1.4, -1.4],
      [1.4, 1.4, 1.4],
    ] as Vec3[]) {
      const skinned: Vec3 = [point[0] - 3 * blend + 5 * (1 - blend), point[1] + 2 * (1 - blend), point[2]];
      for (let axis = 0; axis < 3; axis++) {
        expect(skinned[axis]).toBeGreaterThanOrEqual(bounds.min[axis]);
        expect(skinned[axis]).toBeLessThanOrEqual(bounds.max[axis]);
      }
    }
  expect(bounds.min[0]).toBeLessThan(-4.39);
  expect(bounds.max[0]).toBeGreaterThan(6.39);
  expect(() => creatureDeformedBounds(bounds, new Float32Array([NaN]))).toThrow("finite");
});
test("moving actors select matching variant bindings and correctives before deformation", async () => {
  const { artifact, definition, details } = fixture(),
    runtime = await RuntimeSession.create(),
    selector = new CreatureDetailSelector();
  const originalPositions = artifact.mesh.positions.slice();
  runtime.addCharacter("moving", artifact, definition);
  runtime.playMotion("moving", "flex", 0);
  try {
    const evaluate = (view: Camera) =>
      runtime.evaluatedCharacters(
        undefined,
        [0, 0, 0],
        (source, position, scale, id, matrices) =>
          selector.select(id, source, position, scale, view, 720, matrices).detail,
      )[0];
    const near = evaluate(camera(3));
    expect(near.artifact).toBe(artifact);
    expect(near.deformation?.positionDeltas.length).toBe(12);
    const far = evaluate(camera(150));
    expect(far.artifact.mesh).toBe(details[1].mesh);
    expect(far.artifact.weights).toBe(details[1].weights);
    expect(far.artifact.jointIndices).toBe(details[1].jointIndices);
    expect(far.deformation?.positionDeltas.length).toBe(9);
    expect(far.deformation?.positionDeltas[3]).toBeCloseTo(0.1);
    const state = runtime.bodyState("moving");
    runtime.setTarget("moving", [1, state.position[1], 0]);
    runtime.advance(1 / 60);
    const moved = evaluate(camera(150));
    expect(moved.artifact).toBe(far.artifact);
    expect(moved.artifact.weights.length).toBe((moved.artifact.mesh.positions.length / 3) * 4);
    const restored = evaluate(camera(3));
    expect(restored.artifact).toBe(artifact);
    expect(restored.deformation?.positionDeltas.length).toBe(12);
    expect(artifact.mesh.positions).toEqual(originalPositions);
    expect(artifact.jointIndices).toHaveLength(16);
  } finally {
    runtime.dispose();
  }
});
test("scene host publishes selected detail metadata, honors viewport height and counts variant products once", async () => {
  const { project, artifact, details } = fixture(),
    host = new BrowserSceneHost(project, {
      compile: async (document) => (document.id === artifact.id ? artifact : null),
    });
  await host.prepare(artifact.id);
  try {
    host.setViewportHeight(360);
    const scene = host.extract(camera(150)),
      surface = scene.surfaces.find((s) => s.skin);
    expect(surface?.mesh).toBe(details[1].mesh);
    expect(surface?.skin?.weights).toBe(details[1].weights);
    expect(surface?.creatureDetail).toMatchObject({
      label: "distant",
      viewportHeight: 360,
      maxError: null,
      selection: "projected-diameter-hysteresis",
    });
    expect(() => host.setViewportHeight(NaN)).toThrow("Viewport");
    const bytes = artifactBytes([artifact]);
    const guideIndices = new Uint32Array([0, 1, 2]);
    const extra = {
      ...artifact,
      creatureDetails: details.map((detail) => ({ ...detail, groomGuideIndices: guideIndices })),
    };
    expect(artifactBytes([extra]) - bytes).toBe(guideIndices.byteLength);
    const stripped = {
      ...artifact,
      creatureDetails: details.map((detail) => ({ ...detail, correctives: [] })),
    };
    const extraCorrectives = details.reduce(
      (sum, detail) =>
        sum +
        (detail.correctives ?? []).reduce(
          (n, c) => n + c.vertices.byteLength + c.displacements.byteLength,
          0,
        ),
      0,
    );
    expect(bytes - artifactBytes([stripped])).toBe(extraCorrectives);
  } finally {
    host.dispose();
  }
});
