import { expect, test } from "bun:test";
import {
  authoredAtmosphereComposition,
  compileDocument,
  compileScatteringAtmosphereTable,
} from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import {
  type EvaluatedScene,
  type ObjectDefinition,
  type RenderSurface,
  transformMatrix,
} from "@wrela/model";
import { finiteSunScene, validateAtmosphereTable } from "@wrela/render-webgpu/transport";

function sceneFixture(): EvaluatedScene {
  const source = referenceProject().documents.find(
    (document): document is ObjectDefinition => document.kind === "object",
  );
  if (!source) throw new Error("Object fixture missing");
  const doc: ObjectDefinition = {
    ...source,
    field: {
      root: "ball",
      resolution: 16,
      bounds: { min: [-2, -2, -2], max: [2, 2, 2] },
      nodes: [
        {
          id: "ball",
          name: "Ball",
          kind: "sphere",
          position: [0, 0, 0],
          rotation: [0, 0, 0],
          size: [1, 1, 1],
          radius: 1,
          blend: 0,
          children: [],
        },
      ],
    },
  };
  const artifact = compileDocument(doc, "interactive");
  if (artifact?.kind !== "surface") throw new Error("Sphere compilation failed");
  const sphere: RenderSurface = {
    id: "ball",
    source: doc.id,
    mesh: artifact.mesh,
    renderProducts: artifact.renderProducts,
    selectedRenderProduct: artifact.renderProducts?.find((product) => product.kind === "analytic-quadric"),
    matrix: transformMatrix([0, 2, 0]),
    material: {
      color: [0.5, 0.5, 0.5],
      secondary: [0.5, 0.5, 0.5],
      roughness: 0.5,
      metallic: 0,
      pattern: 0,
      scale: 1,
      normalStrength: 0,
    },
  };
  const ground: RenderSurface = {
    ...sphere,
    id: "ground",
    selectedRenderProduct: undefined,
    renderProducts: undefined,
    matrix: transformMatrix([0, 0, 0]),
    mesh: {
      positions: new Float32Array([-10, 0, -10, 10, 0, -10, 0, 0, 10]),
      normals: new Float32Array([0, 1, 0, 0, 1, 0, 0, 1, 0]),
      indices: new Uint32Array([0, 2, 1]),
      bounds: { min: [-10, 0, -10], max: [10, 0, 10] },
    },
  };
  return {
    surfaces: [sphere, ground],
    camera: { position: [0, 3, 10], target: [0, 0, 0], fov: 50 },
    environment: {
      sunDirection: [0, 1, 0],
      sunColor: [1, 1, 1],
      sunIntensity: 1,
      ambient: 0.5,
      skyColor: [0.2, 0.4, 0.6],
      horizonColor: [0.5, 0.6, 0.7],
      groundColor: [0.2, 0.2, 0.2],
      fogDensity: 0,
      wind: [0, 0, 0],
      exposure: 1,
    },
    time: 0,
    mode: "beauty",
    grid: false,
  };
}

test("finite sun rejects omitted geometry and horizon cases while accepting an exact sphere over a plane", () => {
  const scene = sceneFixture();
  expect(finiteSunScene(scene, 0.00465).enabled).toBe(true);
  expect(finiteSunScene(scene, 0.00465).data[0]).toBe(1);
  expect(
    finiteSunScene(
      { ...scene, surfaces: [{ ...scene.surfaces[0], selectedRenderProduct: undefined }, scene.surfaces[1]] },
      0.00465,
    ).enabled,
  ).toBe(false);
  expect(
    finiteSunScene({ ...scene, environment: { ...scene.environment, sunDirection: [1, 0.001, 0] } }, 0.00465)
      .enabled,
  ).toBe(false);
  expect(
    finiteSunScene(
      {
        ...scene,
        surfaces: [scene.surfaces[0], { ...scene.surfaces[1], matrix: transformMatrix([0, 2, 0]) }],
      },
      0.00465,
    ).enabled,
  ).toBe(false);
  expect(
    finiteSunScene({ ...scene, surfaces: [{ ...scene.surfaces[0], wind: 1 }, scene.surfaces[1]] }, 0.00465)
      .enabled,
  ).toBe(false);
});

test("atmosphere upload validates ownership, dimensions, finite cells and memory budget", () => {
  const table = compileScatteringAtmosphereTable(authoredAtmosphereComposition(), {
    heightCount: 4,
    cosineCount: 8,
  });
  if ("status" in table) throw new Error("Unexpected cancellation");
  const runtime = { ...table, planetCenter: [0, -6360000, 0] as [number, number, number] };
  expect(() => validateAtmosphereTable(runtime, 1024 * 1024)).not.toThrow();
  expect(() => validateAtmosphereTable(runtime, 8)).toThrow();
  expect(() => validateAtmosphereTable({ ...runtime, byteLength: 0 }, 1024 * 1024)).toThrow();
  const invalid = table.data.slice();
  invalid[12] = 2.5;
  expect(() => validateAtmosphereTable({ ...runtime, data: invalid }, 1024 * 1024)).toThrow();
  invalid.set(table.data);
  invalid[8] = NaN;
  expect(() => validateAtmosphereTable({ ...runtime, data: invalid }, 1024 * 1024)).toThrow();
  invalid.set(table.data);
  invalid[invalid.length - 3] = 9;
  expect(() => validateAtmosphereTable({ ...runtime, data: invalid }, 1024 * 1024)).toThrow("ozone");
  // A legacy V2 payload has no absorption tail and an unused fourth cell lane.
  const legacy = table.data.slice(0, table.data.length - 8);
  legacy[14] = 2;
  for (let i = 19; i < legacy.length; i += 4) legacy[i] = 0;
  expect(() =>
    validateAtmosphereTable({ ...runtime, data: legacy, byteLength: legacy.byteLength }, 1024 * 1024),
  ).not.toThrow();
});
