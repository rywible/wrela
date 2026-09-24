import type { Vec3 } from "@wrela/model";
import type { IndirectGeometry } from "./indirect-query";
import { RadianceVisibilityQuery } from "./radiance-visibility-query";

type Node = { sample: number; min: Vec3; max: Vec3; left: number; right: number };
/** Exact bounded nearest-neighbor queries for an immutable probe set. Ties
 * retain source order, matching the original exhaustive receiver search. */
export class RadianceSampleIndex {
  private nodes: Node[] = [];
  private root: number;
  readonly visibility?: RadianceVisibilityQuery;
  constructor(
    readonly positions: readonly Vec3[],
    geometry?: IndirectGeometry,
  ) {
    if (positions.length > 512 || positions.some((p) => p.length !== 3 || !p.every(Number.isFinite)))
      throw Error("Invalid radiance sample index");
    const build = (ids: number[]): number => {
      if (!ids.length) return -1;
      const min = [0, 1, 2].map((a) => Math.min(...ids.map((i) => positions[i][a]))) as Vec3;
      const max = [0, 1, 2].map((a) => Math.max(...ids.map((i) => positions[i][a]))) as Vec3;
      let axis = 0;
      for (let a = 1; a < 3; a++) if (max[a] - min[a] > max[axis] - min[axis]) axis = a;
      ids.sort((a, b) => positions[a][axis] - positions[b][axis] || a - b);
      const mid = ids.length >>> 1,
        id = this.nodes.length;
      this.nodes.push({ sample: ids[mid], min, max, left: -1, right: -1 });
      this.nodes[id].left = build(ids.slice(0, mid));
      this.nodes[id].right = build(ids.slice(mid + 1));
      return id;
    };
    this.root = build(Array.from({ length: positions.length }, (_, i) => i));
    if (geometry) this.visibility = new RadianceVisibilityQuery(geometry, positions.length);
  }
  nearest(position: Vec3, limit: number): { i: number; d: number }[] {
    if (!Number.isInteger(limit) || limit < 1 || limit > 512) throw Error("Invalid nearest-sample limit");
    const result: { i: number; d: number }[] = [];
    const bound = (id: number) => {
      if (id < 0) return Infinity;
      const n = this.nodes[id];
      let d = 0;
      for (let a = 0; a < 3; a++) {
        const delta = Math.max(n.min[a] - position[a], 0, position[a] - n.max[a]);
        d += delta * delta;
      }
      return d;
    };
    const visit = (id: number, lower: number) => {
      if (id < 0 || lower > (result.length === limit ? Math.min(576, result[limit - 1].d) : 576)) return;
      const node = this.nodes[id],
        i = node.sample,
        p = this.positions[i];
      // Match the exhaustive path's arithmetic exactly.
      const x = p[0] - position[0],
        y = p[1] - position[1],
        z = p[2] - position[2],
        d = x * x + y * y + z * z;
      const last = result[result.length - 1];
      if (d <= 576 && (result.length < limit || d < last.d || (d === last.d && i < last.i))) {
        let at = result.length;
        while (at > 0 && (d < result[at - 1].d || (d === result[at - 1].d && i < result[at - 1].i))) at--;
        result.splice(at, 0, { i, d });
        if (result.length > limit) result.pop();
      }
      const left = bound(node.left),
        right = bound(node.right);
      if (left <= right) {
        visit(node.left, left);
        visit(node.right, right);
      } else {
        visit(node.right, right);
        visit(node.left, left);
      }
    };
    visit(this.root, 0);
    return result;
  }
}
