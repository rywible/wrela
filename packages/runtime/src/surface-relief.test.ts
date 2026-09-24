import { expect, test } from "bun:test";
import { compileAssemblyMesh, makeDirectRenderProduct } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import {
  assemblySchema,
  createSurfaceAppearance,
  identityMatrix,
  type MeshData,
  type RenderSurface,
  surfaceReliefSchema,
} from "@wrela/model";
import { artifactBytes } from "./artifact-memory";
import { SurfaceReliefCache } from "./surface-relief";

function surface(): RenderSurface {
  const assembly = assemblySchema.parse({
    grid: 0.1,
    clearances: [],
    parts: [
      {
        id: "stone",
        name: "Stone",
        profile: { kind: "rectangle", width: 0.4, height: 0.4 },
        path: [
          [0, 0, 0],
          [0, 0, 0.4],
        ],
        bevel: 0,
        position: [0, 0, 0],
        rotation: [0, 0, 0],
        repeat: { count: 1, offset: [0, 0, 0] },
        sockets: [],
        wear: { amount: 0, scale: 1, seed: 1 },
      },
    ],
  });
  const appearance = createSurfaceAppearance();
  appearance.relief = surfaceReliefSchema.parse({
    kind: "stone",
    amplitude: 0.015,
    scale: 0.1,
    seed: 7,
    targetEdgeLength: 0.05,
  });
  return {
    id: "stone",
    source: "stone",
    mesh: compileAssemblyMesh(assembly, "rock").mesh,
    matrix: identityMatrix(),
    material: {
      color: [0.3, 0.3, 0.3],
      secondary: [0.3, 0.3, 0.3],
      roughness: 0.7,
      metallic: 0,
      pattern: 0,
      scale: 1,
      normalStrength: 0,
      appearance,
    },
    renderProducts: [makeDirectRenderProduct("old-geometry")],
  };
}

test("relief cache reuses immutable source geometry, invalidates recipes and counts coarse memory once", () => {
  const source = surface(),
    cache = new SurfaceReliefCache();
  cache.beginFrame();
  const first = cache.apply(source, "rock");
  expect(first.mesh).not.toBe(source.mesh);
  expect(first.reliefAppearance?.recipe).toEqual(source.material.appearance?.relief);
  expect(first.reliefAppearance?.geometryWeights).toEqual(cache.reviews[0].geometryBandWeights);
  expect(first.mesh.reliefCoordinates?.length).toBe(first.mesh.positions.length);
  expect(first.mesh.reliefNormals?.length).toBe(first.mesh.normals.length);
  expect(first.renderProducts?.map((product) => product.kind)).toEqual(["direct-mesh", "parametric-mesh"]);
  expect(first.renderProducts?.[0].sourceKey).not.toBe("old-geometry");
  expect(cache.reviews[0].applied).toBe(true);
  expect(cache.byteLength).toBe(artifactBytes(cache.artifacts));
  const second = cache.apply({ ...source, id: "another-instance" }, "rock");
  expect(second.mesh).toBe(first.mesh);
  expect(cache.compilations).toBe(1);
  const alternateBudget = new SurfaceReliefCache(32 * 1024 * 1024, 200);
  const alternate = alternateBudget.apply(source, "rock");
  expect(alternate.renderProducts?.[0].sourceKey).not.toBe(first.renderProducts?.[0].sourceKey);
  expect(cache.reviews.length).toBe(1);
  expect(cache.reviews[0].cacheHit).toBe(true);
  const appearance = structuredClone(source.material.appearance);
  if (!appearance?.relief) throw new Error("Missing relief");
  appearance.relief.seed++;
  cache.beginFrame();
  const changed = cache.apply({ ...source, material: { ...source.material, appearance } }, "rock");
  expect(changed.mesh).not.toBe(first.mesh);
  expect(changed.renderProducts?.[0].sourceKey).not.toBe(first.renderProducts?.[0].sourceKey);
  expect(cache.compilations).toBe(2);
  cache.apply({ ...source, mesh: structuredClone(source.mesh) }, "rock");
  expect(cache.compilations).toBe(3);
  cache.clear();
  expect(cache.byteLength).toBe(0);
  expect(cache.reviews).toEqual([]);
});

test("relief skips mixed materials and partial, skinned, deformed or water surfaces explicitly", () => {
  const source = surface(),
    cache = new SurfaceReliefCache();
  const half = Math.floor(source.mesh.indices.length / 6) * 3;
  const mixed = {
    ...source,
    mesh: {
      ...source.mesh,
      materialGroups: [
        { material: "rock", start: 0, count: half },
        { material: "paint", start: half, count: source.mesh.indices.length - half },
      ],
    },
  };
  const skin = {
    ...source,
    skin: { jointIndices: new Uint16Array(), weights: new Float32Array(), matrices: identityMatrix() },
  };
  const deformation = {
    ...source,
    deformation: {
      revision: "pose",
      positionDeltas: new Float32Array(),
      normalDeltas: new Float32Array(),
      maxDisplacement: 0.1,
    },
  };
  const partial = { ...source, drawRange: { start: 3, count: 3 } };
  const waterDefinition = referenceProject().documents.find((document) => document.kind === "water");
  if (waterDefinition?.kind !== "water") throw new Error("Missing reference water");
  const water = { ...source, water: waterDefinition };
  for (const item of [mixed, skin, deformation, partial, water]) expect(cache.apply(item, "rock")).toBe(item);
  expect(cache.compilations).toBe(0);
  expect(cache.diagnostics.map((item) => item.code)).toEqual([
    "surface-relief.mixed-material-unsupported",
    "surface-relief.skin-unsupported",
    "surface-relief.deformation-unsupported",
    "surface-relief.partial-draw-range-unsupported",
    "surface-relief.water-unsupported",
  ]);
});

test("wind relief preserves coarse generator details and marks projected-size selection as unbounded", () => {
  const source = surface(),
    originalCoarser: MeshData = structuredClone(source.mesh);
  source.wind = 0.1;
  source.details = [
    { label: "original-small", mesh: originalCoarser, maxProjectedDiameter: 24, maxError: null },
  ];
  source.drawRange = { start: 0, count: source.mesh.indices.length };
  const cache = new SurfaceReliefCache();
  const near = cache.apply(source, "rock");
  expect(near.drawRange?.count).toBe(near.mesh.indices.length);
  expect(near.renderProducts).toBeUndefined();
  expect(near.details?.[0].mesh).toBe(source.mesh);
  expect(near.details?.[0].maxError).toBeNull();
  expect(near.details?.[1].mesh).toBe(originalCoarser);
  expect(cache.reviews[0].realization).toBe("wind-heuristic");
});

test("bounded cache pins visible entries and memoizes excess working set instead of recompiling every frame", () => {
  const first = surface(),
    second = { ...surface(), id: "second", source: "second" };
  const probe = new SurfaceReliefCache();
  probe.apply(first, "rock");
  const cache = new SurfaceReliefCache(probe.byteLength + 1024);
  cache.beginFrame();
  const resident = cache.apply(first, "rock");
  expect(cache.apply(second, "rock")).toBe(second);
  expect(cache.size).toBe(1);
  const compiled = cache.compilations;
  for (let frame = 0; frame < 3; frame++) {
    cache.beginFrame();
    expect(cache.apply(first, "rock").mesh).toBe(resident.mesh);
    expect(cache.apply(second, "rock")).toBe(second);
  }
  expect(cache.compilations).toBe(compiled);
  expect(cache.byteLength).toBeLessThanOrEqual(cache.maxBytes);
  // A different view can replace an entry that has not been used this frame.
  cache.beginFrame();
  expect(cache.apply(second, "rock").mesh).not.toBe(second.mesh);
  expect(cache.size).toBe(1);
  expect(cache.byteLength).toBeLessThanOrEqual(cache.maxBytes);
  const tiny = new SurfaceReliefCache(1);
  tiny.apply(first, "rock");
  tiny.beginFrame();
  tiny.apply(first, "rock");
  expect(tiny.compilations).toBe(1);
  expect(tiny.size).toBe(0);
});

test("unchanged pinned working sets reuse exact admission accounting across repeated budget refusals", () => {
  const first = surface(),
    second = { ...surface(), id: "refused", source: "refused" };
  const probe = new SurfaceReliefCache();
  probe.apply(first, "rock");
  const cache = new SurfaceReliefCache(probe.byteLength + 1024);
  cache.beginFrame();
  cache.apply(first, "rock");
  expect(cache.apply(second, "rock")).toBe(second);
  for (let frame = 0; frame < 3; frame++) {
    cache.beginFrame();
    const scans = cache.admissionMeasurements;
    cache.apply(first, "rock");
    for (let instance = 0; instance < 200; instance++) expect(cache.apply(second, "rock")).toBe(second);
    expect(cache.admissionMeasurements - scans).toBe(0);
    expect(cache.size).toBe(1);
    expect(cache.byteLength).toBeLessThanOrEqual(cache.maxBytes);
  }
  // A new view releases the pinned source and admits the previously refused mesh.
  cache.beginFrame();
  expect(cache.apply(second, "rock").mesh).not.toBe(second.mesh);
  expect(cache.size).toBe(1);
});
