import { expect, test } from "bun:test";
import { type CharacterDefinition, type CreatureGroom, creatureSchema } from "@wrela/model";

import {
  type CreatureGeometry,
  clearCreatureCompilerCache,
  creaturePreparationCacheMetrics,
} from "./creature";
import { createCreatureBodyProjector, creatureBodyProjectionKey } from "./creature-projection";
import type { GroomChartSample } from "./groom";

function fixture() {
  const creature = creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      {
        id: "coat",
        name: "Coat",
        nodeIds: ["torso"],
        frame: { position: [0, 0, 0], rotation: [0, 0, 0] },
        extent: [1, 1, 1],
      },
    ],
  });
  const document = {
    id: "body",
    name: "Body",
    schemaVersion: 1,
    dependencies: [],
    kind: "character",
    material: "skin",
    creature,
    field: {
      root: "torso",
      resolution: 12,
      bounds: { min: [-1, -1, -1], max: [1, 1, 1] },
      nodes: [
        {
          id: "torso",
          name: "Torso",
          kind: "union",
          children: ["skin-leaf"],
          position: [0, 0, 0],
          rotation: [0, 0, 0],
          size: [1, 1, 1],
          radius: 1,
          blend: 0,
        },
        {
          id: "skin-leaf",
          name: "Skin",
          kind: "box",
          children: [],
          position: [0, 0, 0],
          rotation: [0, 0, 0],
          size: [0.5, 0.5, 0.5],
          radius: 1,
          blend: 0,
        },
      ],
    },
    joints: [],
    motions: [],
    physics: { mode: "kinematic", mass: 1, friction: 0, restitution: 0 },
  } as CharacterDefinition;
  const sample: GroomChartSample = {
    position: [0, 0, 0],
    normal: [0, 1, 0],
    tangent: [1, 0, 0],
    region: "coat",
    chart: "support",
    chartRevision: 1,
  };
  const groom = {
    id: "coat-layer",
    region: "coat",
    rootProjection: { maxDistance: 0.75, direction: "outward" },
  } as CreatureGroom;
  const body: CreatureGeometry = {
    key: "body",
    regions: Array(8).fill("coat"),
    coordinates: Array(8).fill(null),
    diagnostics: [],
    mesh: {
      positions: new Float32Array([
        -0.5, 0.5, -0.5, 0.5, 0.5, -0.5, 0.5, 0.5, 0.5, -0.5, 0.5, 0.5, -0.5, -0.5, -0.5, 0.5, -0.5, -0.5,
        0.5, -0.5, 0.5, -0.5, -0.5, 0.5,
      ]),
      normals: new Float32Array([0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, -1, 0, 0, -1, 0, 0, -1, 0, 0, -1, 0]),
      indices: new Uint32Array([0, 3, 2, 0, 2, 1, 4, 5, 6, 4, 6, 7]),
      sourceIds: Array(8).fill("skin-leaf"),
      bounds: { min: [-0.5, -0.5, -0.5], max: [0.5, 0.5, 0.5] },
    },
  };
  return { document, body, groom, sample };
}

test("interior chart roots project to actual body triangles in physical metres", () => {
  const { document, body, groom, sample } = fixture(),
    projector = createCreatureBodyProjector(document, body),
    result = projector.projectRoot(groom, sample);
  expect(result.status).toBe("resolved");
  expect(result.position).toEqual([0, 0.5, 0]);
  expect(result.normal).toEqual([0, 1, 0]);
  expect(result.distance).toBeCloseTo(0.5, 6);
  expect(result.sourceNode).toBe("skin-leaf");
  expect(result.interval?.[0]).toBeLessThanOrEqual(0.5);
  expect(result.interval?.[1]).toBeGreaterThanOrEqual(0.5);
  expect(projector.metrics.triangleTests).toBeLessThanOrEqual(4);
  if (!groom.rootProjection) throw new Error("fixture");
  groom.rootProjection.maxDistance = 0.4;
  expect(projector.projectRoot(groom, sample).status).toBe("no-hit");
});

test("direction opt-in controls inward projection; two genuine eligible surfaces remain ambiguous", () => {
  const { document, body, groom, sample } = fixture(),
    projector = createCreatureBodyProjector(document, body);
  if (!groom.rootProjection) throw new Error("fixture");
  sample.position = [0, 0.6, 0];
  groom.rootProjection.maxDistance = 0.2;
  expect(projector.projectRoot(groom, sample).status).toBe("no-hit");
  groom.rootProjection.direction = "both";
  const inward = projector.projectRoot(groom, sample);
  expect(inward.status).toBe("resolved");
  expect(inward.distance).toBeCloseTo(0.1, 6);
  sample.position = [0, 0, 0];
  groom.rootProjection.maxDistance = 0.75;
  expect(projector.projectRoot(groom, sample).status).toBe("ambiguous");
});

test("projection requires both anatomical ownership and declared node ancestry", () => {
  const { document, body, groom, sample } = fixture();
  if (!groom.rootProjection) throw new Error("fixture");
  groom.rootProjection.nodeIds = ["unrelated"];
  expect(createCreatureBodyProjector(document, body).projectRoot(groom, sample).status).toBe("wrong-region");
  groom.rootProjection.nodeIds = ["torso"];
  body.regions.fill("other-limb");
  expect(createCreatureBodyProjector(document, body).projectRoot(groom, sample).status).toBe("wrong-region");
});

test("projection cache tracks actual positions/normals/topology but ignores material triangle regrouping", () => {
  clearCreatureCompilerCache();
  const { document, body, groom, sample } = fixture();
  const first = createCreatureBodyProjector(document, body);
  const key = creatureBodyProjectionKey(body);
  body.mesh.indices = new Uint32Array([4, 6, 7, 4, 5, 6, 0, 2, 1, 0, 3, 2]);
  expect(creatureBodyProjectionKey(body)).toBe(key);
  createCreatureBodyProjector(document, body);
  expect(creaturePreparationCacheMetrics().projection.builds).toBe(1);
  for (let vertex = 0; vertex < 4; vertex++) body.mesh.positions[vertex * 3 + 1] = 0.6;
  body.mesh.bounds.max[1] = 0.6;
  const next = createCreatureBodyProjector(document, body);
  expect(creaturePreparationCacheMetrics().projection.builds).toBe(2);
  expect(next.projectRoot(groom, sample).position?.[1]).toBeCloseTo(0.6, 6);
  expect(first.projectRoot(groom, sample).position?.[1]).toBeCloseTo(0.5, 6);
});

test("degenerate normals and excessive layered intersections never return an arbitrary partial hit", () => {
  const { document, body, groom, sample } = fixture();
  sample.normal = [0, 0, 0];
  expect(createCreatureBodyProjector(document, body).projectRoot(groom, sample).status).toBe("no-hit");
  sample.normal = [0, 1, 0];
  body.mesh.indices = new Uint32Array(Array.from({ length: 600 }, () => [0, 3, 2]).flat());
  const result = createCreatureBodyProjector(document, body).projectRoot(groom, sample);
  expect(result.status).toBe("ambiguous");
  expect(result.reason).toContain("256 surfaces");
});

test("cold and warm projection metadata are identical after triangle regrouping and cyclic rotation", () => {
  clearCreatureCompilerCache();
  const { document, body, groom, sample } = fixture();
  // Shared-edge vertex provenance makes triangle/corner tie-breaking observable.
  document.field.nodes.push({ ...document.field.nodes[1], id: "second-leaf" });
  document.field.nodes[0].children.push("second-leaf");
  if (body.mesh.sourceIds) body.mesh.sourceIds[2] = "second-leaf";
  const original = createCreatureBodyProjector(document, body).projectRoot(groom, sample);
  body.mesh.indices = new Uint32Array([6, 7, 4, 5, 6, 4, 2, 1, 0, 3, 2, 0]);
  const warm = createCreatureBodyProjector(document, body).projectRoot(groom, sample);
  clearCreatureCompilerCache();
  const cold = createCreatureBodyProjector(document, body).projectRoot(groom, sample);
  expect(original.status).toBe("resolved");
  expect(warm).toEqual(original);
  expect(cold).toEqual(original);
});
