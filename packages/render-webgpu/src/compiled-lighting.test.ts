import { expect, test } from "bun:test";
import { identityMatrix, type RenderSurface } from "@wrela/model";

import {
  applyAerialPerspective,
  createAtmosphereWGSL,
  defaultPhysicalAtmosphere,
  miePhase,
  rayleighPhase,
} from "./atmosphere";
import { createFiniteSunWGSL, extractFiniteSunSphere, packFiniteSunSpheres } from "./finite-sun";

test("exact sun sphere extraction rejects proxies, shear, deformation and partial surfaces", () => {
  const surface = {
    matrix: identityMatrix(),
    mesh: { indices: new Uint32Array(12) },
    selectedRenderProduct: {
      kind: "analytic-quadric",
      primitive: { center: [1, 2, 3], radii: [2, 2, 2], rotation: [0.1, 0.2, 0.3] },
    },
  } as RenderSurface;
  expect(extractFiniteSunSphere(surface)).toEqual({ center: [1, 2, 3], radius: 2 });
  const matrix = identityMatrix();
  matrix[0] = matrix[5] = matrix[10] = 2;
  matrix[12] = 10;
  expect(extractFiniteSunSphere({ ...surface, matrix })).toEqual({ center: [12, 4, 6], radius: 4 });
  matrix[4] = 0.01;
  expect(extractFiniteSunSphere({ ...surface, matrix })).toBeNull();
  expect(extractFiniteSunSphere({ ...surface, wind: 0.1 })).toBeNull();
  expect(extractFiniteSunSphere({ ...surface, castsShadow: false })).toBeNull();
  expect(extractFiniteSunSphere({ ...surface, drawRange: { start: 0, count: 6 } })).toBeNull();
  expect(extractFiniteSunSphere({ ...surface, selectedRenderProduct: undefined })).toBeNull();
  const ellipsoid = structuredClone(surface);
  if (ellipsoid.selectedRenderProduct?.kind !== "analytic-quadric") throw new Error("Missing test primitive");
  ellipsoid.selectedRenderProduct.primitive.radii[1] = 3;
  expect(extractFiniteSunSphere(ellipsoid)).toBeNull();
});

test("finite sun storage is bounded and rejects invalid actual spheres", () => {
  const data = packFiniteSunSpheres([{ center: [2, 3, 4], radius: 1 }]);
  expect(data.byteLength).toBe(272);
  expect(Array.from(data.slice(0, 8))).toEqual([1, 0, 0, 0, 2, 3, 4, 1]);
  expect(() =>
    packFiniteSunSpheres(Array.from({ length: 17 }, () => ({ center: [0, 0, 0] as const, radius: 1 }))),
  ).toThrow();
  expect(() => packFiniteSunSpheres([{ center: [0, 0, Number.NaN], radius: 1 }])).toThrow();
  expect(() => createFiniteSunWGSL(-1, 0)).toThrow();
  expect(() => createAtmosphereWGSL(0, 0.5)).toThrow();
});
test("scene-linear aerial perspective has vacuum, opaque and exposure-consistent limits", () => {
  const radiance = [2, 1, 0.5] as const,
    source = [0.2, 0.3, 0.4] as const;
  expect(applyAerialPerspective(radiance, source, 0, 0.02)).toEqual([...radiance]);
  expect(applyAerialPerspective(radiance, source, 10000, 1)).toEqual([...source]);
  const a = applyAerialPerspective(radiance, source, 4, 0.2);
  const b = applyAerialPerspective([4, 2, 1], [0.4, 0.6, 0.8], 4, 0.2);
  for (let i = 0; i < 3; i++) expect(b[i]).toBeCloseTo(2 * a[i], 14);
  expect(() => applyAerialPerspective(radiance, source, -1, 1)).toThrow();
});

test("physical Rayleigh and Mie phase functions preserve unit scattered energy", () => {
  const steps = 65536;
  for (const anisotropy of [-0.5, 0, 0.76, 0.95]) {
    let mie = 0,
      rayleigh = 0;
    for (let i = 0; i < steps; i++) {
      const cosine = -1 + (2 * (i + 0.5)) / steps;
      mie += miePhase(cosine, anisotropy);
      rayleigh += rayleighPhase(cosine);
    }
    expect((mie * 4 * Math.PI) / steps).toBeCloseTo(1, 4);
    expect((rayleigh * 4 * Math.PI) / steps).toBeCloseTo(1, 8);
  }
  expect(miePhase(0.5, 0)).toBe(1 / (4 * Math.PI));
  expect(() => miePhase(0, 1)).toThrow();
  const table = defaultPhysicalAtmosphere();
  expect(table.data[14]).toBe(3);
  expect(table.byteLength).toBe(table.data.byteLength);
});
