import { expect, test } from "bun:test";
import {
  identityMatrix,
  intersectQuadric,
  multiplyMatrices,
  quadricUnitToLocal,
  type RenderSurface,
  type Vec3,
} from "@wrela/model";

import { batchSurfaces } from "./batching";
import { lookAt, multiply, orthographic, perspective } from "./math";
import { PRIMITIVE_FLOATS, packPrimitive, packPrimitiveBatch, primitiveRectangle } from "./primitive";

const shape = {
  center: [0, 0, 0] as Vec3,
  radii: [1, 2, 1] as Vec3,
  rotation: [0, 0, 0] as Vec3,
  nodeId: "stone",
};
test("primitive GPU pack supports perspective, orthographic light and render-relative transforms", () => {
  const cameraVP = multiply(perspective(60, 1), lookAt([0, 0, 5], [0, 0, 0]));
  const lightVP = multiply(orthographic(10, 0.1, 100), lookAt([5, 8, 5], [0, 0, 0]));
  const model = identityMatrix();
  model[12] = 8;
  const data = packPrimitive(shape, model, cameraVP, lightVP);
  if (!data) throw new Error("Primitive packing failed");
  expect(data.length).toBe(PRIMITIVE_FLOATS);
  expect(data.every(Number.isFinite)).toBe(true);
  expect(intersectQuadric(data.slice(0, 16), [8, 0, 5], [0, 0, -1])?.distance).toBeCloseTo(4);
  expect(packPrimitive(shape, new Float32Array(16), cameraVP, lightVP)).toBeNull();
  expect(packPrimitive({ ...shape, radii: [1e-8, 1, 1] }, model, cameraVP, lightVP)).toBeNull();
  const poison = identityMatrix();
  poison[0] = NaN;
  expect(packPrimitive(shape, poison, cameraVP, lightVP)).toBeNull();
});

test("primitive rectangles conservatively cover projected bounds and independent ellipsoid samples", () => {
  const rotatedShape = { ...shape, radii: [0.9, 0.4, 0.6] as Vec3, rotation: [0.3, 0.7, -0.4] as Vec3 };
  const model = identityMatrix();
  model[0] = 1.4;
  model[4] = 0.25;
  model[5] = 0.8;
  model[12] = 0.8;
  const unitToWorld = multiplyMatrices(model, quadricUnitToLocal(rotatedShape));
  const cameraVP = multiply(perspective(60, 1.4), lookAt([0, 1, 8], [0, 0, 0]));
  const lightVP = multiply(orthographic(10, 0.1, 100), lookAt([5, 8, 5], [0, 0, 0]));
  const project = (vp: Float32Array, world: number[]) => {
    const clip = [0, 1, 2, 3].map(
      (r) => vp[r] * world[0] + vp[r + 4] * world[1] + vp[r + 8] * world[2] + vp[r + 12],
    );
    return [clip[0] / clip[3], clip[1] / clip[3]];
  };
  for (const vp of [cameraVP, lightVP]) {
    const rectangle = primitiveRectangle(unitToWorld, vp);
    expect((rectangle[2] - rectangle[0]) * (rectangle[3] - rectangle[1])).toBeLessThan(1);
    const check = (world: number[]) => {
      const p = project(vp, world);
      expect(p[0]).toBeGreaterThanOrEqual(rectangle[0]);
      expect(p[0]).toBeLessThanOrEqual(rectangle[2]);
      expect(p[1]).toBeGreaterThanOrEqual(rectangle[1]);
      expect(p[1]).toBeLessThanOrEqual(rectangle[3]);
    };
    const extent = [0, 1, 2].map((r) => Math.hypot(unitToWorld[r], unitToWorld[r + 4], unitToWorld[r + 8]));
    for (let corner = 0; corner < 8; corner++)
      check([0, 1, 2].map((r) => unitToWorld[r + 12] + (corner & (1 << r) ? extent[r] : -extent[r])));
    for (let latitude = 0; latitude <= 20; latitude++)
      for (let longitude = 0; longitude < 40; longitude++) {
        const theta = (latitude * Math.PI) / 20,
          phi = (longitude * Math.PI) / 20;
        const unit = [Math.sin(theta) * Math.cos(phi), Math.cos(theta), Math.sin(theta) * Math.sin(phi)];
        check(
          [0, 1, 2].map(
            (r) =>
              unitToWorld[r] * unit[0] +
              unitToWorld[r + 4] * unit[1] +
              unitToWorld[r + 8] * unit[2] +
              unitToWorld[r + 12],
          ),
        );
      }
  }
  const packed = packPrimitive(rotatedShape, model, cameraVP, lightVP);
  if (!packed) throw new Error("Missing primitive data");
  expect(Array.from(packed.slice(64, 68))).toEqual(
    Array.from(new Float32Array(primitiveRectangle(unitToWorld, cameraVP))),
  );
  expect(Array.from(packed.slice(68, 72))).toEqual(
    Array.from(new Float32Array(primitiveRectangle(unitToWorld, lightVP))),
  );
});

test("primitive proxy expands to full viewport for camera inside, near clipping and projective uncertainty", () => {
  const vp = perspective(60, 1);
  expect(primitiveRectangle(identityMatrix(), vp)).toEqual([-1, -1, 1, 1]);
  const near = identityMatrix();
  near[0] = near[5] = near[10] = 0.01;
  near[14] = -0.055;
  expect(primitiveRectangle(near, vp)).toEqual([-1, -1, 1, 1]);
  const invalid = identityMatrix();
  invalid[14] = NaN;
  expect(primitiveRectangle(invalid, vp)).toEqual([-1, -1, 1, 1]);
  const behind = identityMatrix();
  behind[14] = 10;
  expect(primitiveRectangle(behind, vp)).toEqual([-1, -1, 1, 1]);
});

test("batched analytic records preserve per-instance transforms and independent projected rectangles", () => {
  const cameraVP = multiply(perspective(60, 1), lookAt([0, 0, 8], [0, 0, 0])),
    lightVP = multiply(orthographic(10, 0.1, 100), lookAt([5, 8, 5], [0, 0, 0]));
  const surfaces: RenderSurface[] = Array.from({ length: 3 }, (_, i) => {
    const matrix = identityMatrix();
    matrix[12] = (i - 1) * 2;
    return {
      id: `stone-${i}`,
      source: "stone",
      matrix,
      mesh: {
        positions: new Float32Array(),
        normals: new Float32Array(),
        indices: new Uint32Array(),
        bounds: { min: [-1, -2, -1], max: [1, 2, 1] },
      },
      material: {
        color: [1, 1, 1],
        secondary: [1, 1, 1],
        roughness: 0.3,
        metallic: 0,
        normalStrength: 0,
        scale: 1,
        pattern: 0,
      },
      selectedRenderProduct: {
        kind: "analytic-quadric",
        key: "quadric",
        sourceKey: "source",
        algorithmVersion: "v2",
        formatVersion: 1,
        domainKey: "domain",
        assumptions: [],
        errors: [],
        byteLength: 0,
        fallbackKey: "primary",
        dependencies: [],
        primitive: shape,
      },
    };
  });
  const batch = packPrimitiveBatch(surfaces, cameraVP, lightVP);
  // Analytic camera draws can share a quadric batch; mesh-backed local shadows
  // must preserve each source mesh rather than repeating the first caster.
  expect(batchSurfaces(surfaces, (surface) => surfaces.indexOf(surface) + 1)).toHaveLength(1);
  expect(batchSurfaces(surfaces, (surface) => surfaces.indexOf(surface) + 1, undefined, true)).toHaveLength(
    3,
  );
  if (!batch) throw new Error("Batch packing failed");
  expect(batch.length).toBe(3 * PRIMITIVE_FLOATS);
  for (let i = 0; i < 3; i++) {
    const record = batch.subarray(i * PRIMITIVE_FLOATS, (i + 1) * PRIMITIVE_FLOATS);
    expect(intersectQuadric(record.subarray(0, 16), [(i - 1) * 2, 0, 5], [0, 0, -1])?.distance).toBeCloseTo(
      4,
    );
    const single = packPrimitive(shape, surfaces[i].matrix, cameraVP, lightVP);
    expect(Array.from(record)).toEqual(single ? Array.from(single) : []);
  }
  expect(packPrimitiveBatch([{ ...surfaces[0], wind: 1 }], cameraVP, lightVP)).toBeNull();
});
