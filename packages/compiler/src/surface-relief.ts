import {
  add,
  cross,
  type Diagnostic,
  dot,
  type MeshData,
  normalize,
  type SurfaceRelief,
  type SurfaceReliefAppearance,
  scale,
  sub,
  surfaceReliefSchema,
  type Vec3,
  validThinCoverage,
} from "@wrela/model";

import { surfaceReliefDepth, surfaceReliefSlopeVariance } from "./surface-relief-pattern";
import {
  surfaceReliefEdgePriority,
  surfaceReliefGeometryWeights,
  surfaceReliefPatternSpacing,
} from "./surface-relief-sampling";
import { sourceThicknessQuery } from "./surface-relief-thickness";

/** Flat shading seams need at most three output vertices per triangle. */
export const MAX_SURFACE_RELIEF_TRIANGLES = 80_000;
export type SurfaceReliefReview = {
  applied: boolean;
  sourceTriangles: number;
  triangles: number;
  byteLength: number;
  maxDisplacement: number;
  /** Certified distance between this generated near mesh and the original coarse mesh. */
  coarseDistanceBound: number;
  targetEdgeLength: number;
  achievedMaxEdgeLength: number;
  budgetLimited: boolean;
  boundaryVertices: number;
  curvatureLimitedVertices: number;
  thicknessLimitedVertices: number;
  orientationBackoffs: number;
  sourceClosed: boolean;
  normalError: "unknown";
  patternEdgeLength: number;
  geometryBandWeights: Vec3;
  residualBandWeights: Vec3;
};
export type SurfaceReliefResult = {
  mesh: MeshData;
  diagnostics: Diagnostic[];
  review: SurfaceReliefReview;
  appearance: SurfaceReliefAppearance;
};
type Vertex = { point: Vec3; cap: number; pinned: boolean };
type Corner = {
  vertex: number;
  normal: Vec3;
  color?: Vec3;
  wind?: [number, number, number, number];
  source?: string;
  coverageUv?: [number, number];
};
type Triangle = { corners: [Corner, Corner, Corner]; reference: Vec3; group: number };
type Edge = { a: number; b: number; uses: number; activeUses: number; balance: number; length: number };
const keyOf = (a: number, b: number) => (a < b ? `${a}:${b}` : `${b}:${a}`);
function meshBytes(mesh: MeshData): number {
  return (
    mesh.positions.byteLength +
    mesh.normals.byteLength +
    mesh.indices.byteLength +
    (mesh.reliefCoordinates?.byteLength ?? 0) +
    (mesh.reliefNormals?.byteLength ?? 0) +
    (mesh.thinCoverage
      ? mesh.thinCoverage.uv.byteLength +
        (mesh.thinCoverage.layer?.byteLength ?? 0) +
        mesh.thinCoverage.levels.reduce((sum, level) => sum + level.byteLength, 0)
      : 0) +
    (mesh.colors?.byteLength ?? 0) +
    (mesh.wind?.byteLength ?? 0)
  );
}
function edgesOf(triangles: Triangle[], vertices: Vertex[], activeGroups?: Set<number>): Map<string, Edge> {
  const result = new Map<string, Edge>();
  for (const triangle of triangles)
    for (let i = 0; i < 3; i++) {
      const a = triangle.corners[i].vertex,
        b = triangle.corners[(i + 1) % 3].vertex;
      if (a === b) continue;
      const key = keyOf(a, b),
        existing = result.get(key);
      const active = !activeGroups || activeGroups.has(triangle.group);
      if (existing) {
        existing.uses++;
        existing.activeUses += Number(active);
        existing.balance += a < b ? 1 : -1;
      } else
        result.set(key, {
          a,
          b,
          uses: 1,
          activeUses: Number(active),
          balance: a < b ? 1 : -1,
          length: Math.hypot(...sub(vertices[a].point, vertices[b].point)),
        });
    }
  return result;
}
function midpoint(a: Corner, b: Corner, vertex: number): Corner {
  return {
    vertex,
    normal: normalize(add(a.normal, b.normal)),
    color: a.color && b.color ? scale(add(a.color, b.color), 0.5) : undefined,
    wind:
      a.wind && b.wind
        ? (a.wind.map((value, index) => (value + (b.wind?.[index] ?? 0)) / 2) as Corner["wind"])
        : undefined,
    source:
      a.source === b.source
        ? a.source
        : [a.source, b.source].filter((value): value is string => !!value).sort()[0],
    coverageUv:
      a.coverageUv && b.coverageUv
        ? [(a.coverageUv[0] + b.coverageUv[0]) / 2, (a.coverageUv[1] + b.coverageUv[1]) / 2]
        : undefined,
  };
}
function splitTriangles(
  triangles: Triangle[],
  selected: Map<string, number>,
  vertices: Vertex[],
): Triangle[] {
  return triangles.flatMap((triangle) => {
    const corners = triangle.corners;
    const mids = corners.map((a, i) => {
      const b = corners[(i + 1) % 3],
        vertex = selected.get(keyOf(a.vertex, b.vertex));
      return vertex === undefined ? undefined : midpoint(a, b, vertex);
    });
    const count = mids.filter(Boolean).length;
    const make = (a: Corner, b: Corner, c: Corner): Triangle => ({ ...triangle, corners: [a, b, c] });
    if (count === 0) return [triangle];
    if (count === 3) {
      const [a, b, c] = corners,
        [ab, bc, ca] = mids as [Corner, Corner, Corner];
      return [make(a, ab, ca), make(ab, b, bc), make(ca, bc, c), make(ab, bc, ca)];
    }
    if (count === 1) {
      const i = mids.findIndex(Boolean),
        a = corners[i],
        b = corners[(i + 1) % 3],
        c = corners[(i + 2) % 3];
      const middle = mids[i] as Corner;
      return [make(a, middle, c), make(middle, b, c)];
    }
    const i = mids.findIndex((value, index) => value && mids[(index + 1) % 3]);
    const a = corners[i],
      b = corners[(i + 1) % 3],
      c = corners[(i + 2) % 3];
    const ab = mids[i] as Corner,
      bc = mids[(i + 1) % 3] as Corner;
    // A conforming two-edge split leaves a quad. Choose its shorter diagonal,
    // avoiding gratuitous long thin triangles on anisotropic source fans.
    const acrossA = Math.hypot(...sub(vertices[a.vertex].point, vertices[bc.vertex].point));
    const acrossC = Math.hypot(...sub(vertices[c.vertex].point, vertices[ab.vertex].point));
    return acrossA < acrossC
      ? [make(b, bc, ab), make(a, ab, bc), make(a, bc, c)]
      : [make(b, bc, ab), make(a, ab, c), make(ab, bc, c)];
  });
}
function geometricNormals(triangles: Triangle[], vertices: Vertex[]): Vec3[] {
  const normals = vertices.map((): Vec3 => [0, 0, 0]);
  for (const triangle of triangles)
    for (let i = 0; i < 3; i++) {
      const corner = triangle.corners[i],
        p = vertices[corner.vertex].point;
      const a = normalize(sub(vertices[triangle.corners[(i + 1) % 3].vertex].point, p));
      const b = normalize(sub(vertices[triangle.corners[(i + 2) % 3].vertex].point, p));
      const angle = Math.acos(Math.max(-1, Math.min(1, dot(a, b))));
      normals[corner.vertex] = add(normals[corner.vertex], scale(triangle.reference, angle));
    }
  return normals.map(normalize);
}
function normalKey(corner: Corner): string {
  return `${corner.vertex}/${corner.normal.map((value) => Math.round(value * 10000)).join(",")}`;
}

/**
 * A bounded near realization. Coincident source positions share one geometric
 * vertex even at hard normal/material seams. Edge splits are conforming on every
 * incident face, and all displacement is inward and clipped to the old envelope.
 * The untouched input is the distant candidate and authoritative collision mesh.
 */
export function compileSurfaceRelief(
  mesh: MeshData,
  authored: SurfaceRelief,
  options: { maxTriangles?: number; material?: string } = {},
): SurfaceReliefResult {
  const source = surfaceReliefSchema.parse(authored);
  const sourceTriangles = mesh.indices.length / 3;
  const budget = Math.floor(
    Math.min(MAX_SURFACE_RELIEF_TRIANGLES, Math.max(1, options.maxTriangles ?? 12_000)),
  );
  if (!Number.isFinite(budget) || !Number.isInteger(sourceTriangles))
    throw new Error("Invalid relief triangle budget or source layout");
  const diagnostics: Diagnostic[] = [];
  const review: SurfaceReliefReview = {
    applied: false,
    sourceTriangles,
    triangles: sourceTriangles,
    byteLength: meshBytes(mesh),
    maxDisplacement: 0,
    coarseDistanceBound: 0,
    targetEdgeLength: source.targetEdgeLength,
    achievedMaxEdgeLength: 0,
    budgetLimited: false,
    boundaryVertices: 0,
    curvatureLimitedVertices: 0,
    thicknessLimitedVertices: 0,
    orientationBackoffs: 0,
    sourceClosed: false,
    normalError: "unknown",
    patternEdgeLength: 0,
    geometryBandWeights: [0, 0, 0],
    residualBandWeights: [1, 1, 1],
  };
  const appearance: SurfaceReliefAppearance = {
    recipe: source,
    geometryWeights: [0, 0, 0],
    residualWeights: [1, 1, 1],
    slopeVariance: surfaceReliefSlopeVariance(source),
  };
  const unchanged = () => ({ mesh, diagnostics, review, appearance });
  if (mesh.thinCoverage?.layer) {
    diagnostics.push({
      severity: "info",
      code: "RELIEF_LAYERED_COVERAGE",
      message: "Layered foliage retains its source geometry; relief is evaluated in appearance.",
    });
    return unchanged();
  }
  if (source.amplitude === 0 || sourceTriangles === 0) return unchanged();
  if (sourceTriangles > budget) {
    diagnostics.push({
      severity: "warning",
      code: "surface-relief.source-budget",
      message: `Source has ${sourceTriangles} triangles, above the ${budget} relief budget; retained the original mesh.`,
    });
    review.budgetLimited = true;
    return unchanged();
  }
  const count = mesh.positions.length / 3;
  if (
    !Number.isInteger(count) ||
    mesh.normals.length !== mesh.positions.length ||
    (mesh.colors && mesh.colors.length !== mesh.positions.length) ||
    (mesh.wind && mesh.wind.length !== count * 4) ||
    (mesh.sourceIds && mesh.sourceIds.length !== count) ||
    (mesh.thinCoverage && !validThinCoverage(mesh.thinCoverage, count)) ||
    mesh.indices.some((index) => index >= count) ||
    ![...mesh.bounds.min, ...mesh.bounds.max].every(Number.isFinite) ||
    mesh.positions.some((value) => !Number.isFinite(value))
  )
    throw new Error("Malformed surface relief source mesh");
  const vertices: Vertex[] = [],
    welded = new Map<string, number>();
  const activeGroups =
    options.material && mesh.materialGroups
      ? new Set(
          mesh.materialGroups.flatMap((group, index) => (group.material === options.material ? [index] : [])),
        )
      : undefined;
  if (activeGroups && activeGroups.size === 0) return unchanged();
  const cornerAt = (index: number): Corner => {
    const point = [...mesh.positions.subarray(index * 3, index * 3 + 3)] as Vec3;
    const key = point.join(",");
    let vertex = welded.get(key);
    if (vertex === undefined) {
      vertex = vertices.length;
      welded.set(key, vertex);
      vertices.push({ point, cap: Infinity, pinned: false });
    }
    return {
      vertex,
      normal: normalize([...mesh.normals.subarray(index * 3, index * 3 + 3)] as Vec3),
      color: mesh.colors ? ([...mesh.colors.subarray(index * 3, index * 3 + 3)] as Vec3) : undefined,
      wind: mesh.wind ? ([...mesh.wind.subarray(index * 4, index * 4 + 4)] as Corner["wind"]) : undefined,
      source: mesh.sourceIds?.[index],
      coverageUv: mesh.thinCoverage
        ? [mesh.thinCoverage.uv[index * 2], mesh.thinCoverage.uv[index * 2 + 1]]
        : undefined,
    };
  };
  let group = 0;
  let triangles: Triangle[] = Array.from({ length: sourceTriangles }, (_, index) => {
    while (
      mesh.materialGroups &&
      group + 1 < mesh.materialGroups.length &&
      index * 3 >= mesh.materialGroups[group].start + mesh.materialGroups[group].count
    )
      group++;
    const corners = [
      cornerAt(mesh.indices[index * 3]),
      cornerAt(mesh.indices[index * 3 + 1]),
      cornerAt(mesh.indices[index * 3 + 2]),
    ] as Triangle["corners"];
    const [a, b, c] = corners.map((corner) => vertices[corner.vertex].point);
    return { corners, reference: normalize(cross(sub(b, a), sub(c, a))), group };
  });
  const originalEdges = edgesOf(triangles, vertices, activeGroups);
  review.sourceClosed = [...originalEdges.values()].every((edge) => edge.uses === 2 && edge.balance === 0);
  for (const edge of originalEdges.values()) {
    for (const index of [edge.a, edge.b]) {
      vertices[index].cap = Math.min(vertices[index].cap, edge.length * 0.18, source.scale * 0.3);
      if (edge.uses !== 2 || edge.balance !== 0 || edge.activeUses !== edge.uses)
        vertices[index].pinned = true;
    }
  }
  for (let pass = 0; pass < 12; pass++) {
    const edges = edgesOf(triangles, vertices, activeGroups);
    const candidates = [...edges.values()]
      .map((edge) => ({
        ...edge,
        priority: surfaceReliefEdgePriority(sub(vertices[edge.a].point, vertices[edge.b].point), source),
      }))
      .filter((edge) => edge.activeUses > 0 && edge.priority > source.targetEdgeLength * 1.001)
      .sort((a, b) => b.priority - a.priority || a.a - b.a || a.b - b.b);
    if (!candidates.length) break;
    let available = budget - triangles.length;
    const selected = new Map<string, number>();
    for (const edge of candidates) {
      if (edge.uses > available) continue;
      available -= edge.uses;
      const a = vertices[edge.a],
        b = vertices[edge.b];
      selected.set(keyOf(edge.a, edge.b), vertices.length);
      vertices.push({
        point: scale(add(a.point, b.point), 0.5),
        cap: Math.min(a.cap, b.cap),
        pinned: edge.uses !== 2 || edge.balance !== 0 || edge.activeUses !== edge.uses,
      });
    }
    if (!selected.size) break;
    triangles = splitTriangles(triangles, selected, vertices);
    if (triangles.length > budget)
      throw new Error("Relief conforming subdivision exceeded its triangle budget");
  }
  review.achievedMaxEdgeLength = [...edgesOf(triangles, vertices, activeGroups).values()].reduce(
    (maximum, edge) => (edge.activeUses ? Math.max(maximum, edge.length) : maximum),
    0,
  );
  review.budgetLimited = review.achievedMaxEdgeLength > source.targetEdgeLength * 1.001;
  review.patternEdgeLength = [...edgesOf(triangles, vertices, activeGroups).values()].reduce(
    (maximum, edge) =>
      edge.activeUses
        ? Math.max(
            maximum,
            surfaceReliefPatternSpacing(sub(vertices[edge.a].point, vertices[edge.b].point), source),
          )
        : maximum,
    0,
  );
  review.geometryBandWeights = surfaceReliefGeometryWeights(review.patternEdgeLength, source);
  review.residualBandWeights = review.geometryBandWeights.map((weight) => 1 - weight) as Vec3;
  appearance.geometryWeights = review.geometryBandWeights;
  appearance.residualWeights = review.residualBandWeights;
  if (review.residualBandWeights.some((weight) => weight > 0.001))
    diagnostics.push({
      severity: "info",
      code: "surface-relief.filtered-bands",
      message: `Unresolved relief frequencies transfer to appearance; geometric band weights ${review.geometryBandWeights.map((value) => value.toFixed(3)).join(", ")}. Normal and radiance error remain uncertified.`,
    });
  if (review.budgetLimited)
    diagnostics.push({
      severity: "warning",
      code: "surface-relief.detail-budget",
      message: `Relief reached ${triangles.length} triangles; achieved edge spacing ${review.achievedMaxEdgeLength.toFixed(4)}m exceeds requested ${source.targetEdgeLength}m.`,
    });
  review.boundaryVertices = vertices.filter((vertex) => vertex.pinned).length;
  const inward = geometricNormals(triangles, vertices);
  const creaseVertices = new Set<number>();
  for (const triangle of triangles)
    for (const corner of triangle.corners)
      if (dot(inward[corner.vertex], triangle.reference) < 0.98) creaseVertices.add(corner.vertex);
  const thicknessAt = sourceThicknessQuery(mesh);
  const depths = vertices.map((vertex, index) => {
    if (vertex.pinned || Math.hypot(...inward[index]) < 0.5) return 0;
    const curvatureCap = creaseVertices.has(index) ? vertex.cap : source.scale * 0.3;
    const thicknessCap = thicknessAt(vertex.point, scale(inward[index], -1)) * 0.24;
    if (curvatureCap < source.amplitude) review.curvatureLimitedVertices++;
    if (thicknessCap < Math.min(source.amplitude, curvatureCap)) review.thicknessLimitedVertices++;
    return (
      Math.min(source.amplitude, curvatureCap, thicknessCap) *
      surfaceReliefDepth(vertex.point, inward[index], source, review.geometryBandWeights)
    );
  });
  let realized: Vec3[] = [];
  const attenuations = new Float64Array(vertices.length).fill(1);
  for (let attempt = 0; attempt <= 8; attempt++) {
    realized = vertices.map(
      (vertex, index) =>
        vertex.point.map((value, axis) =>
          Math.fround(
            Math.max(
              mesh.bounds.min[axis],
              Math.min(
                mesh.bounds.max[axis],
                value - inward[index][axis] * depths[index] * attenuations[index],
              ),
            ),
          ),
        ) as Vec3,
    );
    const invertedVertices = new Set<number>();
    for (const triangle of triangles) {
      const [a, b, c] = triangle.corners.map((corner) => realized[corner.vertex]);
      const [ra, rb, rc] = triangle.corners.map((corner) => vertices[corner.vertex].point);
      const before = cross(sub(rb, ra), sub(rc, ra)),
        after = cross(sub(b, a), sub(c, a));
      if (Math.hypot(...before) > 1e-12 && dot(before, after) <= dot(before, before) * 1e-4)
        for (const corner of triangle.corners) invertedVertices.add(corner.vertex);
    }
    if (invertedVertices.size === 0) {
      review.orientationBackoffs = attempt;
      break;
    }
    if (attempt === 8) {
      diagnostics.push({
        severity: "warning",
        code: "surface-relief.orientation",
        message:
          "Relief could not preserve triangle orientation at this feature scale; retained the original mesh.",
      });
      return unchanged();
    }
    for (const index of invertedVertices) attenuations[index] *= 0.5;
  }
  const normals = new Map<string, Vec3>();
  for (const triangle of triangles) {
    const [a, b, c] = triangle.corners.map((corner) => realized[corner.vertex]);
    const face = normalize(cross(sub(b, a), sub(c, a)));
    for (let i = 0; i < 3; i++) {
      const corner = triangle.corners[i],
        point = realized[corner.vertex];
      const ab = normalize(sub(realized[triangle.corners[(i + 1) % 3].vertex], point));
      const ac = normalize(sub(realized[triangle.corners[(i + 2) % 3].vertex], point));
      const angle = Math.acos(Math.max(-1, Math.min(1, dot(ab, ac))));
      const key = normalKey(corner);
      normals.set(key, add(normals.get(key) ?? [0, 0, 0], scale(face, angle)));
    }
  }
  const positions = new Float32Array(triangles.length * 9),
    outputNormals = new Float32Array(positions.length),
    reliefCoordinates = new Float32Array(positions.length),
    reliefNormals = new Float32Array(positions.length),
    indices = new Uint32Array(triangles.length * 3);
  const colors = mesh.colors ? new Float32Array(positions.length) : undefined,
    wind = mesh.wind ? new Float32Array(triangles.length * 12) : undefined;
  const coverageUv = mesh.thinCoverage ? new Float32Array(triangles.length * 6) : undefined;
  const sourceIds = mesh.sourceIds ? ([] as string[]) : undefined;
  const materialGroups = mesh.materialGroups?.map((source) => ({
    material: source.material,
    start: 0,
    count: 0,
  }));
  const bounds = {
    min: [Infinity, Infinity, Infinity] as Vec3,
    max: [-Infinity, -Infinity, -Infinity] as Vec3,
  };
  for (const [triangleIndex, triangle] of triangles.entries())
    for (let i = 0; i < 3; i++) {
      const corner = triangle.corners[i],
        index = triangleIndex * 3 + i,
        point = realized[corner.vertex];
      positions.set(point, index * 3);
      reliefCoordinates.set(vertices[corner.vertex].point, index * 3);
      reliefNormals.set(corner.normal, index * 3);
      const sum = normals.get(normalKey(corner)) ?? corner.normal;
      outputNormals.set(Math.hypot(...sum) > 1e-12 ? normalize(sum) : corner.normal, index * 3);
      indices[index] = index;
      if (colors && corner.color) colors.set(corner.color, index * 3);
      if (wind && corner.wind) wind.set(corner.wind, index * 4);
      if (coverageUv && corner.coverageUv) coverageUv.set(corner.coverageUv, index * 2);
      sourceIds?.push(corner.source ?? "");
      if (materialGroups) materialGroups[triangle.group].count++;
      for (let axis = 0; axis < 3; axis++) {
        bounds.min[axis] = Math.min(bounds.min[axis], point[axis]);
        bounds.max[axis] = Math.max(bounds.max[axis], point[axis]);
      }
    }
  let start = 0;
  for (const group of materialGroups ?? []) {
    group.start = start;
    start += group.count;
  }
  const result: MeshData = {
    ...mesh,
    positions,
    normals: outputNormals,
    reliefCoordinates,
    reliefNormals,
    indices,
    colors,
    wind,
    thinCoverage: mesh.thinCoverage && coverageUv ? { ...mesh.thinCoverage, uv: coverageUv } : undefined,
    sourceIds,
    materialGroups,
    bounds,
    fidelity: {
      sampleSpacing: [
        review.achievedMaxEdgeLength,
        review.achievedMaxEdgeLength,
        review.achievedMaxEdgeLength,
      ],
      maxError: null,
      unresolvedFeatures: review.budgetLimited ? ["surface-relief-target-spacing"] : [],
    },
  };
  review.maxDisplacement = vertices.reduce(
    (maximum, vertex, index) => Math.max(maximum, Math.hypot(...sub(realized[index], vertex.point))),
    0,
  );
  // Piecewise-linear correspondence on the same subdivision bounds both Hausdorff directions.
  review.coarseDistanceBound = review.maxDisplacement;
  review.applied = review.maxDisplacement > 0;
  review.triangles = triangles.length;
  review.byteLength = meshBytes(result);
  if (review.orientationBackoffs)
    diagnostics.push({
      severity: "info",
      code: "surface-relief.curvature",
      message: `Relief depth was locally reduced over ${review.orientationBackoffs} passes to preserve triangle orientation.`,
    });
  return { mesh: result, diagnostics, review, appearance };
}
