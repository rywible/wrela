import type { Vec3 } from "@wrela/model";
import type { IndirectGeometry } from "./indirect-query";

/** Coherent boolean visibility for immutable geometry. A cached blocker is
 * only a candidate: its intersection is retested for every new receiver ray.
 * A miss always visits the complete BVH. No previous clear result is reused. */
export class RadianceVisibilityQuery {
  private hints: Int32Array;
  private hintNodes: Int32Array;
  private stack: Int32Array;
  queries = 0;
  witnessHits = 0;
  constructor(
    readonly geometry: IndirectGeometry,
    slots: number,
  ) {
    if (geometry.layers?.length) throw Error("Static visibility query cannot own transient layers");
    this.hints = new Int32Array(slots).fill(-1);
    this.hintNodes = new Int32Array(slots).fill(-1);
    this.stack = new Int32Array(Math.max(1, geometry.nodes.length));
  }
  private blocked(triangle: number, o: Vec3, d: Vec3, limit: number): boolean {
    const t = this.geometry.triangles[triangle],
      b = t.ab,
      c = t.ac;
    const px = d[1] * c[2] - d[2] * c[1],
      py = d[2] * c[0] - d[0] * c[2],
      pz = d[0] * c[1] - d[1] * c[0];
    const determinant = b[0] * px + b[1] * py + b[2] * pz;
    if (Math.abs(determinant) < 1e-14) return false;
    const x = o[0] - t.a[0],
      y = o[1] - t.a[1],
      z = o[2] - t.a[2];
    const u = (x * px + y * py + z * pz) / determinant;
    if (u < -1e-9 || u > 1 + 1e-9) return false;
    const qx = y * b[2] - z * b[1],
      qy = z * b[0] - x * b[2],
      qz = x * b[1] - y * b[0];
    const v = (d[0] * qx + d[1] * qy + d[2] * qz) / determinant;
    if (v < -1e-9 || u + v > 1 + 1e-9) return false;
    const distance = (c[0] * qx + c[1] * qy + c[2] * qz) / determinant;
    return distance > 1e-5 && distance < limit;
  }
  visible(origin: Vec3, direction: Vec3, limit: number, slot: number): boolean {
    this.queries++;
    const hint = this.hints[slot];
    // Triangle-edge tolerance must not bypass the original BVH's leaf bounds.
    if (
      hint >= 0 &&
      this.intersects(this.hintNodes[slot], origin, direction, limit) &&
      this.blocked(hint, origin, direction, limit)
    ) {
      this.witnessHits++;
      return false;
    }
    let size = 1;
    this.stack[0] = 0;
    while (size) {
      const nodeIndex = this.stack[--size],
        node = this.geometry.nodes[nodeIndex];
      if (!this.intersects(nodeIndex, origin, direction, limit)) continue;
      if (!node.count && node.left >= 0) {
        this.stack[size++] = node.left;
        this.stack[size++] = node.right;
        continue;
      }
      for (let i = node.start; i < node.start + node.count; i++) {
        const triangle = this.geometry.order[i];
        if (triangle === hint) continue;
        if (this.blocked(triangle, origin, direction, limit)) {
          this.hints[slot] = triangle;
          this.hintNodes[slot] = nodeIndex;
          return false;
        }
      }
    }
    return true;
  }
  private intersects(index: number, origin: Vec3, direction: Vec3, limit: number): boolean {
    const node = this.geometry.nodes[index];
    let lo = 1e-5,
      hi = limit;
    for (let a = 0; a < 3; a++) {
      if (Math.abs(direction[a]) < 1e-15) {
        if (origin[a] < node.min[a] || origin[a] > node.max[a]) hi = -1;
      } else {
        const x = (node.min[a] - origin[a]) / direction[a],
          y = (node.max[a] - origin[a]) / direction[a];
        lo = Math.max(lo, Math.min(x, y));
        hi = Math.min(hi, Math.max(x, y));
      }
    }
    return hi >= lo;
  }
}
