import { expect, test } from "bun:test";
import {
  type CompiledCharacter,
  creatureSchema,
  type EvaluatedScene,
  identityMatrix,
  type RenderSurface,
  VIEW_MODES,
} from "@wrela/model";

import {
  applyCreatureInspection,
  creatureInspectionSource,
  inspectCreatureMaterialAtHit,
} from "./creature-inspection";
import { pickScene } from "./picking";

const surface: RenderSurface = {
  id: "actor/material/coat",
  instanceId: "actor",
  source: "creature",
  matrix: identityMatrix(),
  mesh: {
    positions: new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0.1, 1, 0, 0.1, 0, 1, 0.1]),
    normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1]),
    colors: new Float32Array(18).fill(0.2),
    indices: new Uint32Array([0, 1, 2, 3, 4, 5]),
    bounds: { min: [0, 0, 0], max: [1, 1, 0.1] },
  },
  material: {
    color: [0.1, 0.2, 0.3],
    secondary: [0.2, 0.3, 0.4],
    roughness: 0.8,
    metallic: 0,
    pattern: 1,
    scale: 2,
    normalStrength: 0.2,
    creature: { family: "fiber", fiberDirection: [0, 1, 0] },
  },
  creatureInspection: {
    regions: ["head", "shoulder", "shoulder", "shoulder", "shoulder", "shoulder"],
    coordinates: Array(6).fill(null),
    groomVertexStart: 3,
    materialId: "coat",
  },
};
const scene: EvaluatedScene = {
  surfaces: [surface],
  camera: { position: [0, 0, 5], target: [0, 0, 0], fov: 50 },
  time: 2,
  grid: false,
  mode: "clay",
  environment: {
    sunDirection: [0, 1, 0],
    sunColor: [1, 1, 1],
    sunIntensity: 1,
    ambient: 1,
    skyColor: [1, 1, 1],
    horizonColor: [1, 1, 1],
    groundColor: [1, 1, 1],
    fogDensity: 0,
    wind: [0, 0, 0],
    exposure: 1,
  },
};

test("review modes append a stable shader ABI without changing existing captures", () => {
  expect(VIEW_MODES.indexOf("clay")).toBe(10);
  expect(VIEW_MODES.slice(10, 16)).toEqual([
    "clay",
    "thickness",
    "fiber-direction",
    "regions",
    "shading-normals",
    "indirect-lighting",
  ]);
  expect(VIEW_MODES.indexOf("identity")).toBe(6);
});
test("hide coat removes only compiler-attributed groom triangles with unchanged geometry and pose", () => {
  const deformation = { revision: "pose", positionDeltas: new Float32Array(18), maxDisplacement: 0 };
  const input = { ...scene, surfaces: [{ ...surface, deformation }] };
  const viewed = applyCreatureInspection(input, { hideGroom: true });
  expect(viewed.surfaces.length).toBe(1);
  expect(viewed.surfaces[0].drawRange).toEqual({ start: 0, count: 3 });
  expect(viewed.surfaces[0].mesh).toBe(surface.mesh);
  expect(viewed.surfaces[0].deformation).toBe(deformation);
  expect(viewed.camera).toBe(scene.camera);
  expect(viewed.environment).toBe(scene.environment);
  expect(input.surfaces[0].drawRange).toBeUndefined();
  expect(
    applyCreatureInspection(
      { ...input, surfaces: [{ ...surface, creatureInspection: undefined }] },
      { hideGroom: true },
    ).surfaces.length,
  ).toBe(1);
});
test("region review colors are cached derived data and picked material retains original albedo", () => {
  const input = {
    ...scene,
    mode: "regions" as const,
    surfaces: [
      {
        ...surface,
        creatureInspection: {
          ...(surface.creatureInspection as NonNullable<RenderSurface["creatureInspection"]>),
          albedoColors: surface.mesh.colors,
        },
      },
    ],
  };
  const first = applyCreatureInspection(input),
    second = applyCreatureInspection(input);
  expect(first.surfaces[0].mesh).toBe(second.surfaces[0].mesh);
  expect(first.surfaces[0].mesh.positions).toBe(surface.mesh.positions);
  expect(first.surfaces[0].mesh.colors).not.toBe(surface.mesh.colors);
  expect(surface.mesh.colors?.[0]).toBeCloseTo(0.2);
  const hit = {
    distance: 1,
    position: [0, 0, 0] as [number, number, number],
    triangle: 0,
    barycentric: [0.2, 0.3, 0.5] as [number, number, number],
    surfaceId: surface.id,
    documentId: surface.source,
  };
  const inspected = inspectCreatureMaterialAtHit(first, hit);
  expect(inspected?.regions[0]).toEqual({ id: "shoulder", weight: 0.8 });
  expect(inspected?.albedo.vertexColor[0]).toBeCloseTo(0.2);
  expect(inspected?.albedo.minimumBeforeLayers[0]).toBeCloseTo(0.02);
  expect(inspected?.albedo.maximumBeforeLayers[0]).toBeCloseTo(0.04);
});
test("picking follows corrective geometry while inspection returns immutable source position", () => {
  const deformation = {
    revision: "moved",
    positionDeltas: new Float32Array([3, 0, 0, 3, 0, 0, 3, 0, 0, 3, 0, 0, 3, 0, 0, 3, 0, 0]),
    maxDisplacement: 3,
  };
  const input = { ...scene, surfaces: [{ ...surface, deformation, drawRange: { start: 0, count: 3 } }] };
  const hit = pickScene(input, { origin: [3.25, 0.25, 2], direction: [0, 0, -1] });
  expect(hit).not.toBeNull();
  if (!hit) throw new Error("Expected moved triangle");
  expect(hit.position[0]).toBeCloseTo(3.25);
  expect(inspectCreatureMaterialAtHit(input, hit)?.restPosition).toEqual([0.25, 0.25, 0]);
});
test("selected groom LOD uses its guide roots rather than stale high-detail vertex attribution", () => {
  const artifact = {
    kind: "character",
    id: "creature",
    key: "detail",
    mesh: surface.mesh,
    material: "coat",
    joints: [],
    motions: [],
    jointIndices: new Uint16Array(24),
    weights: new Float32Array(24),
    diagnostics: [],
    creature: creatureSchema.parse({ schemaVersion: 1 }),
    creatureBodyVertexCount: 3,
    creatureRegions: ["body", "body", "body", "wrong", "wrong", "wrong"],
    creatureCoordinates: Array(6).fill(null),
    creatureGroomDetail: "distant",
    creatureGroom: {
      key: "groom",
      representation: "opaque-tufts",
      diagnostics: [],
      guides: [
        {
          root: {
            id: "root-a",
            layer: "mane",
            region: "shoulder",
            chart: "chart",
            chartRevision: 1,
            coordinates: [0, 0, 0],
          },
        },
      ],
      details: [{ label: "distant", vertexGuideIndices: new Uint32Array([0, 0, 0]) }],
    },
  } as unknown as CompiledCharacter;
  const metadata = creatureInspectionSource(artifact);
  expect(metadata?.regions).toEqual(["body", "body", "body", "shoulder", "shoulder", "shoulder"]);
  expect(metadata?.coordinates[3]?.layer).toBe("mane");
  expect(metadata?.groomVertexStart).toBe(3);
});
