import type { IndirectLightingField, Vec3 } from "@wrela/model";

import type { IndirectGeometry } from "./indirect-query";
import { indirectRegionSteps } from "./indirect-regions";

export const INDIRECT_VISIBILITY_STEPS = 128;
/** Two vec4s/node and three vec4s/triangle. Depth-first threaded nodes remove
 * the per-pixel traversal stack: min.w is the escape index; max.w packs
 * firstTriangle*8+count (0 for an internal node). Leaves have at most six
 * triangles, and the million-triangle limit keeps indices exact in float32. */
export function* indirectVisibilitySteps(
  geometry: IndirectGeometry,
  origin: Vec3,
): Generator<void, NonNullable<IndirectLightingField["visibility"]>> {
  if (!geometry.triangles.length) return { nodes: new Float32Array(), triangles: new Float32Array() };
  const nodes = new Float32Array(geometry.nodes.length * 8),
    triangles = new Float32Array(geometry.order.length * 12);
  const traversal: number[] = [],
    escapeIndices: number[] = [];
  const pending: { id: number; exit?: number }[] = [{ id: 0 }];
  while (pending.length) {
    const task = pending.pop();
    if (!task) break;
    if (task.exit !== undefined) {
      escapeIndices[task.exit] = traversal.length;
      continue;
    }
    const index = traversal.length,
      node = geometry.nodes[task.id];
    traversal.push(task.id);
    pending.push({ id: task.id, exit: index });
    // Same right-first order as the former bounded stack, including exhaustion.
    if (!node.count) pending.push({ id: node.left }, { id: node.right });
    if (index % 1024 === 1023) yield;
  }
  for (let i = 0; i < geometry.nodes.length; i++) {
    const node = geometry.nodes[traversal[i]];
    for (let axis = 0; axis < 3; axis++) {
      // Outward bounds include fp32 packing uncertainty; triangle intersections
      // are still numerical tests, not an exact-arithmetic visibility certificate.
      const extent = Math.max(
        Math.abs(node.min[axis] - origin[axis]),
        Math.abs(node.max[axis] - origin[axis]),
      );
      const pad = Math.max(0.00001, extent * 2e-7);
      nodes[i * 8 + axis] = node.min[axis] - origin[axis] - pad;
      nodes[i * 8 + axis + 4] = node.max[axis] - origin[axis] + pad;
    }
    nodes[i * 8 + 3] = escapeIndices[i];
    nodes[i * 8 + 7] = node.count ? node.start * 8 + node.count : 0;
    if (i % 1024 === 1023) yield;
  }
  for (let i = 0; i < geometry.order.length; i++) {
    const t = geometry.triangles[geometry.order[i]];
    triangles.set(
      t.a.map((v, axis) => v - origin[axis]),
      i * 12,
    );
    triangles.set(t.ab, i * 12 + 4);
    triangles.set(t.ac, i * 12 + 8);
    if (i % 1024 === 1023) yield;
  }
  return { nodes, triangles };
}
/** CPU mirror of the shader's bounded receiver/probe segment query. Exhaustion
 * returns blocked; it cannot turn missed traversal work into a light leak. */
export function indirectProbeVisible(
  field: IndirectLightingField,
  world: Vec3,
  normal: Vec3,
  probe: Vec3,
  staticReceiver = false,
): boolean {
  const data = field.visibility;
  if (!data?.triangles.length) return true;
  const origin = world.map((v, i) => v - field.origin[i] + normal[i] * 0.0002) as Vec3;
  const offset = probe.map((v, i) => v - field.origin[i] - origin[i]) as Vec3,
    length = Math.hypot(...offset);
  if (length < 0.0004) return true;
  const direction = offset.map((v) => v / length);
  const intersects = (triangle: number): boolean => {
    const t = triangle * 12,
      a = [data.triangles[t], data.triangles[t + 1], data.triangles[t + 2]],
      b = [data.triangles[t + 4], data.triangles[t + 5], data.triangles[t + 6]],
      c = [data.triangles[t + 8], data.triangles[t + 9], data.triangles[t + 10]];
    const p = [
        direction[1] * c[2] - direction[2] * c[1],
        direction[2] * c[0] - direction[0] * c[2],
        direction[0] * c[1] - direction[1] * c[0],
      ],
      det = b[0] * p[0] + b[1] * p[1] + b[2] * p[2];
    if (Math.abs(det) < 1e-12) return false;
    const o = origin.map((v, i) => v - a[i]),
      u = (o[0] * p[0] + o[1] * p[1] + o[2] * p[2]) / det;
    if (u < -1e-6 || u > 1 + 1e-6) return false;
    const q = [o[1] * b[2] - o[2] * b[1], o[2] * b[0] - o[0] * b[2], o[0] * b[1] - o[1] * b[0]],
      v = (direction[0] * q[0] + direction[1] * q[1] + direction[2] * q[2]) / det;
    if (v < -1e-6 || u + v > 1 + 1e-6) return false;
    const distance = (c[0] * q[0] + c[1] * q[1] + c[2] * q[2]) / det;
    return distance > 0.00001 && distance < length - 0.0001;
  };
  if (data.cells) {
    const dims = field.dimensions.map((n) => n - 1);
    const c = world.map((v, a) =>
      Math.min(
        dims[a] - 1,
        Math.max(
          0,
          Math.floor(
            (v - field.origin[a] + normal[a] * Math.min(...field.spacing) * 0.02) / field.spacing[a],
          ),
        ),
      ),
    );
    const offset = (c[0] + dims[0] * (c[1] + dims[1] * c[2])) * 4;
    let region =
      staticReceiver && data.cells[offset + 3] > 0 ? data.cells[offset + 3] : data.cells[offset + 2];
    if (region > 0) {
      const q = world.map(
        (v, a) => (v - field.origin[a] + normal[a] * Math.min(...field.spacing) * 0.02) / field.spacing[a],
      );
      const f = q.map((v, a) => Math.max(0, Math.min(1, v - c[a])));
      const corner = probe.reduce(
        (mask, v, a) => mask | (Math.round((v - field.origin[a]) / field.spacing[a]) - c[a] > 0 ? 1 << a : 0),
        0,
      );
      const bit = 1 << corner;
      for (let level = 0; level < 3; level++) {
        const r = region * 4;
        if (data.cells[r] & bit) return true;
        if (data.cells[r + 1] & bit) return false;
        const next = data.cells[r + 2],
          count = data.cells[r + 3];
        if (next > 0) {
          let child = 0;
          for (let a = 0; a < 3; a++) {
            const b = f[a] >= 0.5 ? 1 : 0;
            child |= b << a;
            f[a] = f[a] * 2 - b;
          }
          region = next + child;
        } else {
          if (count >= 0) {
            const start = (-next - 1) * 4;
            for (let i = 0; i < count; i++)
              if (data.cells[start + i * 2 + 1] & bit && intersects(data.cells[start + i * 2])) return false;
            return true;
          }
          break;
        }
      }
    }
    const first = data.cells[offset] * 4,
      count = data.cells[offset + 1];
    if (count >= 0) {
      for (let i = 0; i < count; i++) if (intersects(data.cells[first + i])) return false;
      return true;
    }
  }
  let current = 0;
  for (let step = 0; step < INDIRECT_VISIBILITY_STEPS && current < data.nodes.length / 8; step++) {
    const index = current * 8;
    let lo = 0.00001,
      hi = length - 0.0001;
    for (let axis = 0; axis < 3; axis++) {
      if (Math.abs(direction[axis]) < 1e-12) {
        if (origin[axis] < data.nodes[index + axis] || origin[axis] > data.nodes[index + axis + 4]) hi = -1;
      } else {
        const a = (data.nodes[index + axis] - origin[axis]) / direction[axis],
          b = (data.nodes[index + axis + 4] - origin[axis]) / direction[axis];
        lo = Math.max(lo, Math.min(a, b));
        hi = Math.min(hi, Math.max(a, b));
      }
    }
    const nextNode = data.nodes[index + 3],
      packed = data.nodes[index + 7];
    if (hi < lo) {
      current = nextNode;
      continue;
    }
    if (packed === 0) {
      current++;
      continue;
    }
    const first = Math.floor(packed / 8),
      count = packed % 8;
    for (let triangle = first; triangle < first + count; triangle++) {
      if (intersects(triangle)) return false;
    }
    current = nextNode;
  }
  return current === data.nodes.length / 8;
}

/** Every receiver/probe segment in a biased interpolation cell lies inside this
 * expanded box. Overlapping triangle bounds form a conservative candidate list;
 * large lists fall back to the original BVH, never omit an arbitrary blocker. */
export function* indirectVisibilityCellSteps(
  geometry: IndirectGeometry,
  field: Pick<IndirectLightingField, "origin" | "spacing" | "dimensions"> & { data?: Float32Array },
  limit = 48,
  regions = true,
  surfaceRegions = false,
): Generator<void, Float32Array> {
  if (!Number.isInteger(limit) || limit < 1 || limit > 128) throw Error("Invalid visibility cell budget");
  const dims = field.dimensions.map((n) => n - 1),
    count = dims[0] * dims[1] * dims[2];
  const headers = new Float32Array(count * 4),
    indices: number[] = [];
  const receiverPad =
    Math.min(...field.spacing) * 0.02 +
    0.0002 +
    Math.hypot(...geometry.bounds.max.map((v, a) => v - geometry.bounds.min[a])) * 4e-6;
  for (let cell = 0; cell < count; cell++) {
    const c = [cell % dims[0], Math.floor(cell / dims[0]) % dims[1], Math.floor(cell / (dims[0] * dims[1]))];
    const lo = c.map((v, a) => field.origin[a] + v * field.spacing[a]);
    const hi = lo.map((v, a) => v + field.spacing[a]);
    const positions = Array.from({ length: 8 }, (_, corner) => {
      const p = c.map((v, a) => v + ((corner >> a) & 1));
      const index = p[0] + field.dimensions[0] * (p[1] + field.dimensions[1] * p[2]);
      return p.map(
        (v, a) => field.origin[a] + v * field.spacing[a] + (field.data?.[index * 60 + [38, 39, 42][a]] ?? 0),
      ) as Vec3;
    });
    for (const p of positions)
      for (let a = 0; a < 3; a++) {
        lo[a] = Math.min(lo[a], p[a]);
        hi[a] = Math.max(hi[a], p[a]);
      }
    for (let a = 0; a < 3; a++) {
      const pad =
        receiverPad +
        Math.max(
          0.00002,
          Math.max(Math.abs(lo[a] - field.origin[a]), Math.abs(hi[a] - field.origin[a])) * 4e-7,
        );
      lo[a] -= pad;
      hi[a] += pad;
    }
    const overlaps = (min: number[], max: number[]) => min.every((v, a) => v <= hi[a] && max[a] >= lo[a]);
    const candidates: number[] = [],
      stack = geometry.nodes.length ? [0] : [];
    let visits = 0;
    while (stack.length && candidates.length <= (regions ? Math.max(limit, 512) : limit)) {
      const id = stack.pop();
      if (id === undefined) break;
      const node = geometry.nodes[id];
      if (overlaps(node.min, node.max)) {
        if (node.count) {
          for (let i = node.start; i < node.start + node.count; i++) {
            const t = geometry.triangles[geometry.order[i]];
            if (overlaps(t.min, t.max)) candidates.push(i);
          }
        } else stack.push(node.left, node.right);
      }
      if (++visits % 128 === 0) yield;
    }
    if (candidates.length > limit) headers[cell * 4 + 1] = -1;
    else {
      headers[cell * 4] = count + indices.length / 4;
      headers[cell * 4 + 1] = candidates.length;
      indices.push(...candidates);
      while (indices.length % 4) indices.push(0);
    }
    if (regions && candidates.length > 4 && candidates.length <= 512 && indices.length < 524288) {
      const address = count + indices.length / 4;
      const cellOrigin = c.map((v, a) => field.origin[a] + v * field.spacing[a]) as Vec3;
      const compiled = yield* indirectRegionSteps(
        geometry,
        cellOrigin,
        field.spacing,
        candidates,
        address,
        Math.min(16384, 524288 - indices.length),
        positions,
      );
      if (compiled) {
        headers[cell * 4 + 2] = address;
        for (const value of compiled) indices.push(value);
      }
      if (surfaceRegions && indices.length < 524288) {
        const staticAddress = count + indices.length / 4;
        const receiver = yield* indirectRegionSteps(
          geometry,
          cellOrigin,
          field.spacing,
          candidates,
          staticAddress,
          Math.min(16384, 524288 - indices.length),
          positions,
          true,
        );
        if (receiver) {
          headers[cell * 4 + 3] = staticAddress;
          for (const value of receiver) indices.push(value);
        }
      }
    }
    yield;
  }
  const result = new Float32Array(headers.length + indices.length);
  result.set(headers);
  result.set(indices, headers.length);
  return result;
}
