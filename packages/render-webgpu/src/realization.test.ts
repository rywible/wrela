import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import {
  type CompiledRenderProduct,
  type EvaluatedScene,
  identityMatrix,
  type RenderCostObservation,
  type RenderSurface,
} from "@wrela/model";
import {
  projectedGeometryError,
  RENDER_KERNEL_VERSION,
  selectRenderProducts,
  selectWaterAppearance,
} from "./realization";

const mesh = {
  positions: new Float32Array([-1, 0, 0, 1, 0, 0, 0, 1, 0]),
  normals: new Float32Array(9),
  indices: new Uint32Array([0, 1, 2]),
  bounds: { min: [-1, -1, -1] as [number, number, number], max: [1, 1, 1] as [number, number, number] },
};
const direct: CompiledRenderProduct = {
  kind: "direct-mesh",
  key: "direct",
  sourceKey: "source",
  formatVersion: 1,
  algorithmVersion: "test",
  domainKey: "domain",
  assumptions: [],
  errors: [{ kind: "unknown", reason: "extraction" }],
  byteLength: 0,
  fallbackKey: null,
  dependencies: [],
};
const analytic: CompiledRenderProduct = {
  ...direct,
  kind: "analytic-quadric",
  key: "analytic",
  fallbackKey: direct.key,
  assumptions: [{ kind: "rigid" }, { kind: "matrix-condition", maximum: 10000 }],
  errors: [
    { kind: "real-bound", metric: "depth", maximum: 0, domain: "isolated zero set", numericError: "unknown" },
  ],
  primitive: { center: [0, 0, 0], radii: [1, 1, 1], rotation: [0, 0, 0], nodeId: "sphere" },
};
const parametric: CompiledRenderProduct = {
  ...direct,
  kind: "parametric-mesh",
  key: "parametric",
  fallbackKey: direct.key,
  mesh: { ...mesh, positions: mesh.positions.slice() },
  byteLength: 100,
  errors: [
    {
      kind: "real-bound",
      metric: "silhouette",
      maximum: 0.001,
      domain: "local Hausdorff metres",
      numericError: "unknown",
    },
  ],
};
const surface: RenderSurface = {
  id: "stone",
  source: "stone",
  mesh,
  matrix: identityMatrix(),
  material: {
    color: [1, 1, 1],
    secondary: [1, 1, 1],
    roughness: 0.3,
    metallic: 0,
    pattern: 0,
    scale: 1,
    normalStrength: 0,
  },
  renderProducts: [direct, analytic, parametric],
};
const scene: EvaluatedScene = {
  surfaces: [surface],
  camera: { position: [0, 0, 50], target: [0, 0, 0], fov: 60 },
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
  time: 0,
  mode: "beauty",
  grid: false,
};
const choose = (s: RenderSurface, geometry: "analytic" | "parametric" = "analytic") =>
  selectRenderProducts({ ...scene, surfaces: [s] }, 1080, { geometry });

test("static geometry controls preserve direct fallback and enforce projected error budget", () => {
  expect(selectRenderProducts(scene, 1080).decisions[0].kind).toBe("parametric-mesh");
  expect(selectRenderProducts(scene, 1080, { geometry: "direct" }).decisions[0].kind).toBe("direct-mesh");
  expect(choose(surface).decisions[0].kind).toBe("analytic-quadric");
  if (parametric.kind !== "parametric-mesh") throw new Error("Invalid fixture");
  expect(choose(surface, "parametric").scene.surfaces[0].mesh).toBe(parametric.mesh);
  expect(
    selectRenderProducts(scene, 1080, { geometry: "parametric", maxGeometryErrorPixels: 0 }).decisions[0]
      .kind,
  ).toBe("direct-mesh");
  expect(projectedGeometryError(scene, surface, 0.001, 1080)).toBeGreaterThan(0);
  expect(
    projectedGeometryError(
      { ...scene, camera: { ...scene.camera, position: [0, 0, 0.1] } },
      surface,
      0.001,
      1080,
    ),
  ).toBe(Infinity);
  expect(selectRenderProducts(scene, NaN, { geometry: "analytic" }).decisions[0].kind).toBe("direct-mesh");
  expect(
    selectRenderProducts(scene, 1080, { geometry: "analytic", maxGeometryErrorPixels: NaN }).decisions[0]
      .kind,
  ).toBe("direct-mesh");
});

test("geometry selector rejects deformation, partial ranges, stale products and invalid transforms", () => {
  const invalid: RenderSurface[] = [
    { ...surface, wind: 1 },
    {
      ...surface,
      skin: { weights: new Float32Array(), jointIndices: new Uint16Array(), matrices: identityMatrix() },
    },
    { ...surface, drawRange: { start: 0, count: 0 } },
    { ...surface, matrix: new Float32Array(16) },
    { ...surface, matrix: Float32Array.from({ length: 16 }, () => NaN) },
    { ...surface, renderProducts: [direct, { ...analytic, sourceKey: "stale" }] },
    { ...surface, renderProducts: [direct, { ...analytic, fallbackKey: "missing" }] },
    {
      ...surface,
      renderProducts: [
        direct,
        { ...analytic, assumptions: [{ kind: "pose-revision", revision: "unknown" }] },
      ],
    },
  ];
  for (const value of invalid) expect(choose(value).decisions[0].kind).toBe("direct-mesh");
  const illConditioned = identityMatrix();
  illConditioned[0] = 1e-6;
  expect(choose({ ...surface, matrix: illConditioned }).decisions[0].kind).toBe("direct-mesh");
  expect(choose({ ...surface, matrix: new Float32Array(16) }, "parametric").decisions[0].kind).toBe(
    "direct-mesh",
  );
});

test("automatic geometry accepts honest real bounds, selecting current measured costs or projected work", () => {
  const primary: CompiledRenderProduct = {
    ...direct,
    errors: [
      {
        kind: "real-bound",
        metric: "silhouette",
        maximum: 0.001,
        domain: "local Hausdorff metres",
        numericError: "unknown",
      },
    ],
  };
  const bounded = { ...scene, surfaces: [{ ...surface, renderProducts: [primary, analytic, parametric] }] };
  const cost = (productKey: string, gpuP95Ms: number, preparationMs = 0): RenderCostObservation => ({
    productKey,
    adapter: "gpu",
    browser: "browser",
    kernelVersion: RENDER_KERNEL_VERSION,
    fixture: "same-workload",
    gpuP50Ms: gpuP95Ms,
    gpuP95Ms,
    preparationMs,
  });
  const costs = [cost("direct", 10), cost("analytic", 1), cost("parametric", 11)];
  expect(selectRenderProducts(bounded, 1080).decisions[0].kind).toBe("direct-mesh");
  expect(selectRenderProducts(bounded, 1080, { costs }, "gpu", "browser").decisions[0].kind).toBe(
    "analytic-quadric",
  );
  expect(selectRenderProducts(bounded, 1080, { costs }, "other", "browser").decisions[0].kind).toBe(
    "direct-mesh",
  );
  expect(
    selectRenderProducts(
      bounded,
      1080,
      { costs: [cost("direct", 10), cost("analytic", 1, 20), cost("parametric", 11)] },
      "gpu",
      "browser",
    ).decisions[0].kind,
  ).toBe("direct-mesh");
  expect(
    selectRenderProducts(
      bounded,
      1080,
      { costs: costs.map((cost) => ({ ...cost, fixture: cost.productKey })) },
      "gpu",
      "browser",
    ).decisions[0].kind,
  ).toBe("direct-mesh");
  expect(selectRenderProducts(bounded, 1080, { maxGeometryErrorPixels: 0 }).decisions[0].kind).toBe(
    "analytic-quadric",
  );
});

test("water automatic policy preserves quality and shutter with explicit reference controls", () => {
  const water = referenceProject().documents.find((doc) => doc.kind === "water");
  if (!water || water.kind !== "water") throw new Error("Missing water fixture");
  const waterScene = { ...scene, surfaces: [{ ...surface, water }] };
  const selected = selectWaterAppearance(waterScene, {}, "low");
  expect(selected.scene.surfaces[0].waterAppearance).toEqual({
    mode: "auto",
    quality: "low",
    shutterSeconds: 1 / 60,
  });
  expect(selectWaterAppearance(waterScene, { water: "reference" }, "high").decisions[0].kind).toBe(
    "reference",
  );
  expect(
    selectWaterAppearance(waterScene, { shutterSeconds: NaN }).scene.surfaces[0].waterAppearance
      ?.shutterSeconds,
  ).toBe(1 / 60);
  const layered = {
    ...waterScene,
    surfaces: [{ ...waterScene.surfaces[0], material: { ...surface.material, normalStrength: 0.2 } }],
  };
  expect(selectWaterAppearance(layered).decisions[0].kind).toBe("direct");
});
