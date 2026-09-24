import {
  type CreatureDefinition,
  type Diagnostic,
  type MeshData,
  normalize,
  sculptMetric,
  sculptRotate,
  sub,
  type Vec3,
} from "@wrela/model";

/** Conforming local subdivision of the existing surface, before sculpt evaluation.
 * This resolves compact sculpt fields; it does not invent curvature absent from the input surface. */
export function refineCreatureSculptSurface(
  base: MeshData,
  creature: CreatureDefinition,
  maxVertices = 250_000,
) {
  const strokes = creature.sculpts.filter((s) => s.detail);
  const diagnostics: Diagnostic[] = [];
  if (!strokes.length) return { mesh: base, diagnostics };
  const mounted = new Set(creature.attachments.flatMap((a) => a.nodeIds));
  const entries = strokes.map((stroke) => {
    const region = creature.regions.find((r) => r.id === stroke.region);
    if (!region) throw Error(`Sculpt ${stroke.id} has no region`);
    const nodes = new Set(
      region.nodeIds.filter((id) => !mounted.has(id) && (!stroke.nodeIds || stroke.nodeIds.includes(id))),
    );
    const path = (stroke.path ?? [stroke.center]).map((p) => sculptMetric(stroke, p));
    const lower = [0, 1, 2].map((axis) => Math.min(...path.map((p) => p[axis])) - 1);
    const upper = [0, 1, 2].map((axis) => Math.max(...path.map((p) => p[axis])) + 1);
    return {
      stroke,
      region,
      nodes,
      lower,
      upper,
      samples: [new Map<number, Vec3>(), new Map<number, Vec3>()],
    };
  });
  const positions = Array.from(base.positions),
    normals = Array.from(base.normals),
    colors = base.colors ? Array.from(base.colors) : undefined;
  const ids = base.sourceIds?.slice() ?? Array(base.positions.length / 3).fill("");
  let triangles = Array.from({ length: base.indices.length / 3 }, (_, i) =>
    Array.from(base.indices.slice(i * 3, i * 3 + 3)),
  );
  let materials = triangles.map(
    (_, i) => base.materialGroups?.find((g) => i * 3 >= g.start && i * 3 < g.start + g.count)?.material,
  );
  const point = (i: number) => positions.slice(i * 3, i * 3 + 3) as Vec3;
  const edgeKey = (a: number, b: number) => (a < b ? `${a}:${b}` : `${b}:${a}`);
  const intersects = (triangle: number[], entry: (typeof entries)[number]) => {
    if (!triangle.some((i) => entry.nodes.has(ids[i]))) return false;
    for (let mirrored = 0; mirrored < (entry.stroke.mirror ? 2 : 1); mirrored++) {
      const cache = entry.samples[mirrored];
      const points = triangle.map((i) => {
        let value = cache.get(i);
        if (!value) {
          const local = sculptRotate(
            sub(point(i), entry.region.frame.position),
            entry.region.frame.rotation,
            true,
          );
          value = sculptMetric(entry.stroke, mirrored ? [-local[0], local[1], local[2]] : local);
          cache.set(i, value);
        }
        return value;
      });
      if (
        [0, 1, 2].every(
          (axis) =>
            Math.max(points[0][axis], points[1][axis], points[2][axis]) >= entry.lower[axis] &&
            Math.min(points[0][axis], points[1][axis], points[2][axis]) <= entry.upper[axis],
        )
      )
        return true;
    }
    return false;
  };
  const maxPasses = Math.max(...strokes.map((s) => s.detail?.passes ?? 0));
  let exhausted = false;
  for (let pass = 0; pass < maxPasses; pass++) {
    const edges = new Map<string, [number, number]>();
    for (const triangle of triangles) {
      const active = entries.filter((e) => pass < (e.stroke.detail?.passes ?? 0) && intersects(triangle, e));
      if (!active.length) continue;
      const maximum = Math.min(...active.map((e) => e.stroke.detail?.maxEdgeLength ?? Infinity));
      for (let i = 0; i < 3; i++) {
        const a = triangle[i],
          b = triangle[(i + 1) % 3];
        if (Math.hypot(...sub(point(a), point(b))) > maximum) edges.set(edgeKey(a, b), [a, b]);
      }
    }
    if (!edges.size) break;
    if (ids.length + edges.size > maxVertices || triangles.length * 4 > 1_000_000) {
      exhausted = true;
      break;
    }
    const middles = new Map<string, number>();
    for (const [key, [a, b]] of edges) {
      const index = ids.length;
      middles.set(key, index);
      ids.push(ids[Math.min(a, b)]);
      for (let axis = 0; axis < 3; axis++)
        positions.push((positions[a * 3 + axis] + positions[b * 3 + axis]) / 2);
      normals.push(
        ...normalize([0, 1, 2].map((axis) => (normals[a * 3 + axis] + normals[b * 3 + axis]) / 2) as Vec3),
      );
      if (colors)
        for (let axis = 0; axis < 3; axis++) colors.push((colors[a * 3 + axis] + colors[b * 3 + axis]) / 2);
    }
    const next: number[][] = [],
      nextMaterials: typeof materials = [];
    triangles.forEach(([a, b, c], i) => {
      const ab = middles.get(edgeKey(a, b)),
        bc = middles.get(edgeKey(b, c)),
        ca = middles.get(edgeKey(c, a));
      const emit = (...t: number[]) => {
        next.push(t);
        nextMaterials.push(materials[i]);
      };
      if (ab !== undefined && bc !== undefined && ca !== undefined) {
        emit(a, ab, ca);
        emit(ab, b, bc);
        emit(ca, bc, c);
        emit(ab, bc, ca);
      } else if (ab !== undefined && bc !== undefined) {
        emit(b, bc, ab);
        emit(a, ab, c);
        emit(ab, bc, c);
      } else if (bc !== undefined && ca !== undefined) {
        emit(c, ca, bc);
        emit(b, bc, a);
        emit(bc, ca, a);
      } else if (ca !== undefined && ab !== undefined) {
        emit(a, ab, ca);
        emit(c, ca, b);
        emit(ca, ab, b);
      } else if (ab !== undefined) {
        emit(a, ab, c);
        emit(ab, b, c);
      } else if (bc !== undefined) {
        emit(b, bc, a);
        emit(bc, c, a);
      } else if (ca !== undefined) {
        emit(c, ca, b);
        emit(ca, a, b);
      } else emit(a, b, c);
    });
    triangles = next;
    materials = nextMaterials;
  }
  for (const entry of entries) {
    let longest = 0;
    for (const triangle of triangles)
      if (intersects(triangle, entry))
        for (let i = 0; i < 3; i++)
          longest = Math.max(longest, Math.hypot(...sub(point(triangle[i]), point(triangle[(i + 1) % 3]))));
    if (longest > (entry.stroke.detail?.maxEdgeLength ?? Infinity) * 1.001)
      diagnostics.push({
        severity: "warning",
        code: "creature-sculpt-detail",
        node: entry.stroke.id,
        message: `Local sculpt edge target was not reached (${longest.toFixed(4)} m); ${exhausted ? "vertex/triangle budget" : "subdivision pass budget"} exhausted. Increase the explicit budget or widen the feature.`,
      });
  }
  const groups: NonNullable<MeshData["materialGroups"]> = [];
  materials.forEach((material, i) => {
    if (!material) return;
    const last = groups.at(-1);
    if (last?.material === material && last.start + last.count === i * 3) last.count += 3;
    else groups.push({ material, start: i * 3, count: 3 });
  });
  return {
    mesh: {
      ...base,
      positions: new Float32Array(positions),
      normals: new Float32Array(normals),
      indices: new Uint32Array(triangles.flat()),
      colors: colors ? new Float32Array(colors) : undefined,
      sourceIds: ids,
      materialGroups: base.materialGroups ? groups : undefined,
    },
    diagnostics,
  };
}
