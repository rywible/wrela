import type { RadianceLightingField, Vec3 } from "@wrela/model";
import { type IndirectGeometry, indirectDot } from "./indirect-query";
import { surfaceTileVisibility } from "./indirect-surface-cache";
import { traceDiffuseReceiverSteps } from "./radiance-surface";

/** Research carrier: interpolation stays on one source triangle and side.
 * A whole-cell visibility certificate also rejects partitions crossing that triangle. */
export type RadianceSurfaceLattice = {
  triangle: number;
  normal: Vec3;
  resolution: number;
  validCells: Uint8Array;
  positions: Vec3[];
  transfer: Float32Array;
  directEmission: Float32Array;
  rays: number;
};

export const surfaceLatticeIndex = (n: number, x: number, y: number) => (y * (2 * n + 3 - y)) / 2 + x;

export function surfaceLatticeBinding(n: number, barycentric: Vec3) {
  if (
    !Number.isInteger(n) ||
    n < 1 ||
    n > 64 ||
    barycentric.some((w) => !Number.isFinite(w) || w < -1e-6) ||
    Math.abs(barycentric.reduce((sum, w) => sum + w, 0) - 1) > 1e-6
  )
    throw Error("Invalid surface lattice coordinate");
  // Normalize tiny floating-point edge errors before selecting a triangle.
  const sum = barycentric.reduce((s, w) => s + Math.max(0, w), 0);
  const x = (n * Math.max(0, barycentric[1])) / sum,
    y = (n * Math.max(0, barycentric[2])) / sum;
  let ix = Math.floor(x),
    iy = Math.floor(y);
  if (ix + iy >= n) {
    if (ix > 0) ix--;
    else iy--;
  }
  const u = x - ix,
    v = y - iy;
  if (u + v <= 1 || ix + iy >= n - 1) {
    return {
      cell: iy * (2 * n - iy) + 2 * ix,
      ids: [
        surfaceLatticeIndex(n, ix, iy),
        surfaceLatticeIndex(n, ix + 1, iy),
        surfaceLatticeIndex(n, ix, iy + 1),
      ],
      weights: [Math.max(0, 1 - u - v), u, v],
    };
  }
  return {
    cell: iy * (2 * n - iy) + 2 * ix + 1,
    ids: [
      surfaceLatticeIndex(n, ix + 1, iy),
      surfaceLatticeIndex(n, ix + 1, iy + 1),
      surfaceLatticeIndex(n, ix, iy + 1),
    ],
    weights: [1 - v, u + v - 1, 1 - u],
  };
}

export function surfaceBarycentric(geometry: IndirectGeometry, triangle: number, position: Vec3): Vec3 {
  const t = geometry.triangles[triangle],
    delta = position.map((v, a) => v - t.a[a]) as Vec3;
  const bb = indirectDot(t.ab, t.ab),
    cc = indirectDot(t.ac, t.ac),
    bc = indirectDot(t.ab, t.ac);
  const xb = indirectDot(delta, t.ab),
    xc = indirectDot(delta, t.ac),
    determinant = bb * cc - bc * bc;
  const u = (cc * xb - bc * xc) / determinant,
    v = (bb * xc - bc * xb) / determinant;
  return [1 - u - v, u, v];
}

export function* compileSurfaceLatticeSteps(
  geometry: IndirectGeometry,
  triangle: number,
  normal: Vec3,
  lights: RadianceLightingField["lights"],
  options: { spacing: number; samples: number; maxResolution?: number; resolution?: number },
): Generator<void, RadianceSurfaceLattice> {
  const t = geometry.triangles[triangle];
  if (
    !t ||
    normal.length !== 3 ||
    !normal.every(Number.isFinite) ||
    Math.abs(indirectDot(normal, normal) - 1) > 1e-6 ||
    !Number.isInteger(options.samples) ||
    options.samples < 1 ||
    options.samples > 1048576 ||
    !(options.spacing > 0) ||
    !Number.isFinite(options.spacing) ||
    !Number.isInteger(options.maxResolution ?? 16) ||
    (options.maxResolution ?? 16) < 1 ||
    (options.maxResolution ?? 16) > 64 ||
    (options.resolution !== undefined &&
      (!Number.isInteger(options.resolution) || options.resolution < 1 || options.resolution > 64)) ||
    Math.abs(indirectDot(t.normal, normal)) < 1 - 1e-6
  )
    throw Error("Surface lattice requires a valid planar source triangle");
  const edge = Math.max(
    Math.hypot(...t.ab),
    Math.hypot(...t.ac),
    Math.hypot(...t.ab.map((v, a) => v - t.ac[a])),
  );
  const resolution =
    options.resolution ??
    Math.min(options.maxResolution ?? 16, Math.max(1, Math.ceil(edge / options.spacing)));
  const count = ((resolution + 1) * (resolution + 2)) / 2;
  const transfer = new Float32Array(count * 108),
    directEmission = new Float32Array(count * 3);
  const positions: Vec3[] = [],
    receivers: Vec3[] = [];
  let rays = 0;
  for (let y = 0; y <= resolution; y++)
    for (let x = 0; x <= resolution - y; x++) {
      const u = x / resolution,
        v = y / resolution;
      const toward = t.ab.map((b, a) => b * (1 / 3 - u) + t.ac[a] * (1 / 3 - v)) as Vec3;
      const inset = Math.min(0.01, 0.004 / Math.max(1e-9, Math.hypot(...toward)));
      const position = t.a.map(
        (a, i) => a + t.ab[i] * u + t.ac[i] * v + toward[i] * inset + normal[i] * 0.003,
      ) as Vec3;
      const id = surfaceLatticeIndex(resolution, x, y);
      positions[id] = position;
      receivers[id] = t.a.map((a, i) => a + t.ab[i] * u + t.ac[i] * v + normal[i] * 0.003) as Vec3;
      yield;
    }
  const validCells = new Uint8Array(resolution * resolution);
  const needed = new Uint8Array(count);
  for (let y = 0; y < resolution; y++)
    for (let x = 0; x < resolution - y; x++) {
      const here = surfaceLatticeIndex(resolution, x, y),
        right = surfaceLatticeIndex(resolution, x + 1, y),
        up = surfaceLatticeIndex(resolution, x, y + 1);
      const cells = [[here, right, up]];
      if (x + y < resolution - 1) cells.push([right, surfaceLatticeIndex(resolution, x + 1, y + 1), up]);
      for (let side = 0; side < cells.length; side++) {
        const ids = cells[side],
          corners = ids.map((id) => receivers[id]);
        const clear = ids.every(
          (id) => surfaceTileVisibility(geometry, corners, normal, positions[id]) === true,
        );
        validCells[y * (2 * resolution - y) + 2 * x + side] = Number(clear);
        if (clear) for (const id of ids) needed[id] = 1;
        yield;
      }
    }
  // Reject before tracing. A node referenced only by invalid cells can never
  // contribute to an admitted receiver and needs no transport work.
  for (let id = 0; id < count; id++) {
    if (!needed[id]) continue;
    const sample = yield* traceDiffuseReceiverSteps(geometry, positions[id], normal, options.samples, lights);
    transfer.set(sample.transfer, id * 108);
    directEmission.set(sample.directEmission, id * 3);
    rays += sample.rays;
  }
  return { triangle, normal, resolution, validCells, positions, transfer, directEmission, rays };
}

export function evaluateSurfaceLattice(lattice: RadianceSurfaceLattice, barycentric: Vec3) {
  const binding = surfaceLatticeBinding(lattice.resolution, barycentric);
  if (!lattice.validCells[binding.cell]) return undefined;
  const transfer = new Float64Array(108),
    directEmission: Vec3 = [0, 0, 0];
  for (let i = 0; i < 3; i++) {
    const id = binding.ids[i],
      weight = binding.weights[i];
    for (let k = 0; k < 108; k++) transfer[k] += lattice.transfer[id * 108 + k] * weight;
    for (let c = 0; c < 3; c++) directEmission[c] += lattice.directEmission[id * 3 + c] * weight;
  }
  return { transfer, directEmission };
}
