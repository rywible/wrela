import type { CompiledGroom, CompiledGroomDetail, CompiledGroomGuide } from "@wrela/model";

import {
  add,
  contentKey,
  cross,
  type Diagnostic,
  dot,
  type MeshData,
  normalize,
  random01,
  scale,
  sub,
  type Vec3,
} from "@wrela/model";

export type { CompiledGroom, CompiledGroomDetail, CompiledGroomGuide, GroomRoot } from "@wrela/model";

/** A bounded projection may deliberately reject a valid chart root without invalidating that chart. */
export class GroomProjectionRejected extends Error {}

export type GroomChartSample = {
  projection?: CompiledGroomGuide["rootProjection"];
  position: Vec3;
  normal: Vec3;
  tangent: Vec3;
  region: string;
  chart: string;
  chartRevision: number;
};
export type GroomLayerSource = {
  evaluate?: (coordinates: Vec3) => GroomChartSample;
  representation?: "tufts" | "ribbons";
  ribbonThickness?: number;
  id: string;
  region: string;
  chart: string;
  chartRevision?: number;
  radialCoordinate?: number;
  lodFractions?: number[];
  shape?: (sample: GroomChartSample, t: number) => Vec3;
  material: string;
  seed: number;
  guideCount: number;
  density: number;
  length: number;
  width: number;
  taper: number;
  lift: number;
  flow: Vec3;
  /** Optional common rest-space comb direction; flow otherwise uses the local chart frame. */
  direction?: Vec3;
  clump: number;
  curl: number;
  frizz: number;
  rootColor: Vec3;
  tipColor: Vec3;
  stiffness: number;
  damping: number;
  /** Return correlated growth coverage, e.g. the same scar mask used for skin. */
  coverage?: (coordinates: Vec3) => number;
};
const mix = (a: Vec3, b: Vec3, t: number): Vec3 => add(scale(a, 1 - t), scale(b, t));
function radicalInverse(index: number): number {
  let result = 0,
    weight = 0.5;
  while (index > 0) {
    result += (index % 2) * weight;
    index = Math.floor(index / 2);
    weight *= 0.5;
  }
  return result;
}
function finiteVector(v: Vec3): boolean {
  return v.every(Number.isFinite);
}

/** Deterministic nested guide subsets make LOD and proportion changes preserve source identities.
 * Opaque closed tufts deliberately use the existing renderer; alpha coverage/fiber scattering
 * is not approximated by pretending these are transparent hair cards. */
export function compileGroom(
  layers: GroomLayerSource[],
  evaluate: (chart: string, coordinates: Vec3) => GroomChartSample,
  options: { maxGuides?: number; maxVertices?: number; sourceKey?: string } = {},
): CompiledGroom {
  const requestedGuides = Math.min(8192, Math.max(0, Math.floor(options.maxGuides ?? 4096)));
  const maxVertices = Math.min(250_000, Math.max(0, Math.floor(options.maxVertices ?? 150_000)));
  if (!Number.isFinite(requestedGuides) || !Number.isFinite(maxVertices))
    throw new Error("Groom budgets must be finite");
  const diagnostics: Diagnostic[] = [],
    guides: CompiledGroomGuide[] = [];
  // Reserve enough room for every retained root in the hero realization; lower detail
  // products can therefore never reveal a root omitted from the hero product by its budget.
  const maxGuides = Math.min(requestedGuides, Math.floor(maxVertices / 34));
  if (maxGuides < requestedGuides)
    diagnostics.push({
      severity: "warning",
      code: "groom-vertex-budget",
      message: `Root budget reduced to ${maxGuides} to fit complete hero tufts within ${maxVertices} vertices`,
    });
  const requests = layers.map((layer) => Math.ceil(Math.min(16384, layer.guideCount) * layer.density));
  const total = requests.reduce((sum, count) => sum + count, 0);
  const quotas = requests.map((count) =>
    total > maxGuides ? Math.floor((count * maxGuides) / total) : count,
  );
  let remaining = Math.min(maxGuides, total) - quotas.reduce((sum, count) => sum + count, 0);
  for (let i = 0; i < quotas.length && remaining > 0; i++)
    if (quotas[i] < requests[i]) {
      quotas[i]++;
      remaining--;
    }
  const ids = new Set<string>();
  for (const [layerIndex, layer] of layers.entries()) {
    const fractions = layer.lodFractions ?? [1, 0.5, 0.25];
    if (
      !fractions.length ||
      fractions.some((v, i) => !Number.isFinite(v) || v < 0 || v > 1 || (i > 0 && v > fractions[i - 1]))
    )
      throw new Error(`Groom detail fractions must be finite and nonincreasing for ${layer.id}`);
    if (ids.has(layer.id)) throw new Error(`Duplicate groom layer ${layer.id}`);
    ids.add(layer.id);
    const scalars = [
      layer.ribbonThickness ?? 0.08,
      layer.seed,
      layer.guideCount,
      layer.density,
      layer.length,
      layer.width,
      layer.taper,
      layer.lift,
      layer.clump,
      layer.curl,
      layer.frizz,
      layer.stiffness,
      layer.damping,
    ];
    if (
      scalars.some((v) => !Number.isFinite(v)) ||
      (layer.representation !== undefined &&
        layer.representation !== "tufts" &&
        layer.representation !== "ribbons") ||
      !finiteVector(layer.flow) ||
      (layer.direction !== undefined && !finiteVector(layer.direction)) ||
      !finiteVector(layer.rootColor) ||
      !finiteVector(layer.tipColor) ||
      (layer.ribbonThickness ?? 0.08) < 0.001 ||
      (layer.ribbonThickness ?? 0.08) > 1 ||
      layer.guideCount < 0 ||
      !Number.isInteger(layer.guideCount) ||
      layer.length <= 0 ||
      layer.width <= 0 ||
      layer.density < 0 ||
      layer.density > 1 ||
      layer.taper < 0 ||
      layer.taper > 1 ||
      layer.lift < 0 ||
      layer.lift > 1 ||
      layer.clump < 0 ||
      layer.clump > 1 ||
      layer.frizz < 0 ||
      layer.curl < 0 ||
      layer.stiffness < 0 ||
      layer.damping < 0 ||
      [...layer.rootColor, ...layer.tipColor].some((v) => v < 0 || v > 1)
    )
      throw new Error(`Invalid groom parameters in ${layer.id}`);
    const shiftU = random01(layer.seed + 193),
      shiftV = random01(layer.seed + 887);
    let invalid = 0,
      budgeted = 0,
      accepted = 0;
    // Explicitly bounded candidate work, even if all growth masks reject roots.
    const candidates = Math.min(layer.guideCount, 16384);
    if (candidates < layer.guideCount)
      diagnostics.push({
        severity: "warning",
        code: "groom-candidate-budget",
        node: layer.id,
        message: `Root candidates capped at ${candidates}`,
      });
    for (let index = 0; index < candidates; index++) {
      const coordinates: Vec3 = [
        (radicalInverse(index + 1) + shiftU) % 1,
        ((index + 1) * 0.6180339887498949 + shiftV) % 1,
        layer.radialCoordinate ?? 1,
      ];
      const coverage = layer.coverage?.(coordinates) ?? 1;
      if (!Number.isFinite(coverage) || coverage < 0 || coverage > 1)
        throw new Error(`Invalid growth coverage for ${layer.id}`);
      if (random01(layer.seed + index * 71 + 1009) >= layer.density * coverage) continue;
      if (guides.length >= maxGuides || accepted >= quotas[layerIndex]) {
        budgeted++;
        continue;
      }
      let sample: GroomChartSample;
      try {
        sample = layer.evaluate ? layer.evaluate(coordinates) : evaluate(layer.chart, coordinates);
      } catch (error) {
        if (!(error instanceof GroomProjectionRejected)) invalid++;
        continue;
      }
      if (
        sample.region !== layer.region ||
        sample.chart !== layer.chart ||
        (layer.chartRevision !== undefined && sample.chartRevision !== layer.chartRevision) ||
        !finiteVector(sample.position) ||
        !finiteVector(sample.normal) ||
        !finiteVector(sample.tangent) ||
        Math.hypot(...sample.normal) < 1e-8 ||
        Math.hypot(...cross(sample.normal, sample.tangent)) < 1e-8
      ) {
        invalid++;
        continue;
      }
      const normal = normalize(sample.normal),
        tangent = normalize(sub(sample.tangent, scale(normal, dot(sample.tangent, normal)))),
        bitangent = normalize(cross(normal, tangent));
      const authored =
        layer.direction ??
        add(
          scale(tangent, layer.flow[0]),
          add(scale(bitangent, layer.flow[1]), scale(normal, layer.flow[2])),
        );
      const flow = Math.hypot(...authored) > 1e-8 ? normalize(authored) : tangent;
      const mixedDirection = add(scale(flow, 1 - layer.lift), scale(normal, layer.lift));
      const direction = Math.hypot(...mixedDirection) > 1e-8 ? normalize(mixedDirection) : normal;
      // A local chart lattice defines shared clump direction without nearest-triangle reassignment.
      const clusterU = (Math.floor(coordinates[0] * 8) + 0.5) / 8,
        clusterV = (Math.floor(coordinates[1] * 8) + 0.5) / 8;
      const clump = add(
        scale(tangent, (clusterU - coordinates[0]) * 8),
        scale(bitangent, (clusterV - coordinates[1]) * 8),
      );
      const length = layer.length * (0.85 + 0.3 * random01(layer.seed + index * 19 + 7));
      const phase = random01(layer.seed + index * 31 + 11) * Math.PI * 2;
      const points: Vec3[] = [];
      for (let step = 0; step <= 8; step++) {
        const t = step / 8,
          amplitude = Math.sin((t * Math.PI) / 2) * length;
        const curl = add(
          scale(tangent, Math.sin(t * Math.PI * 2 + phase) - Math.sin(phase)),
          scale(bitangent, Math.cos(t * Math.PI * 2 + phase) - Math.cos(phase)),
        );
        const frizz = Math.sin(t * Math.PI * 6 + phase) - Math.sin(phase);
        points.push(
          layer.shape
            ? layer.shape(sample, t)
            : add(
                sample.position,
                add(
                  scale(direction, length * t),
                  add(
                    scale(clump, layer.clump * length * t * t * 0.35),
                    add(
                      scale(curl, layer.curl * amplitude * 0.15),
                      scale(bitangent, frizz * layer.frizz * amplitude * 0.04),
                    ),
                  ),
                ),
              ),
        );
      }
      if (points.some((point) => !finiteVector(point)))
        throw new Error(`Nonfinite authored guide in ${layer.id}`);
      const rootTangent = normalize(sub(points[1], points[0]));
      const widthCandidate = cross(normal, rootTangent);
      const widthDirection = Math.hypot(...widthCandidate) > 1e-8 ? normalize(widthCandidate) : tangent;
      accepted++;
      guides.push({
        ...(sample.projection ? { rootProjection: sample.projection } : {}),
        representation: layer.representation ?? "tufts",
        ribbonThickness: layer.ribbonThickness ?? 0.08,
        widthDirection,
        root: {
          id: `${layer.id}-root-${index}`,
          layer: layer.id,
          region: layer.region,
          chart: layer.chart,
          chartRevision: sample.chartRevision,
          coordinates,
        },
        points,
        normal,
        width: layer.width,
        taper: layer.taper,
        material: layer.material,
        rootColor: layer.rootColor,
        tipColor: layer.tipColor,
        stiffness: layer.stiffness,
        damping: layer.damping,
      });
    }
    if (invalid)
      diagnostics.push({
        severity: "error",
        code: "groom-invalid-anchor",
        node: layer.id,
        message: `${invalid} roots rejected: missing, changed, degenerate, or incompatible chart`,
      });
    if (budgeted)
      diagnostics.push({
        severity: "warning",
        code: "groom-guide-budget",
        node: layer.id,
        message: `${budgeted} accepted roots omitted by the ${maxGuides} guide budget`,
      });
  }
  // Keep the six directional support tips of every layer in all nonempty products.
  // This preserves a measured rest-coat envelope without claiming full silhouette equivalence.
  const protectedRoots = new Set<string>();
  for (const layer of layers) {
    const members = guides.filter((guide) => guide.root.layer === layer.id);
    for (let axis = 0; axis < 3; axis++)
      for (const sign of [-1, 1]) {
        let best: CompiledGroomGuide | undefined;
        for (const guide of members)
          if (!best || guide.points[8][axis] * sign > best.points[8][axis] * sign) best = guide;
        if (best) protectedRoots.add(best.root.id);
      }
  }
  const detailCount = Math.max(1, ...layers.map((layer) => (layer.lodFractions ?? [1, 0.5, 0.25]).length));
  const details: CompiledGroomDetail[] = Array.from({ length: detailCount }, (_, index) =>
    buildDetail(
      guides,
      ["hero", "gameplay", "distant"][index] ?? `detail-${index}`,
      index,
      layers,
      protectedRoots,
      index === 0 ? 8 : index === 1 ? 4 : index === 2 ? 2 : 1,
      index === 0 ? 4 : 3,
      maxVertices,
    ),
  );
  for (const [detailIndex, detail] of details.entries()) {
    const expected = guides.filter((guide) => selectGuide(guide, layers, detailIndex, protectedRoots)).length;
    if (detail.cost.guides < expected)
      diagnostics.push({
        severity: "warning",
        code: "groom-vertex-budget",
        message: `${detail.label} contains ${detail.cost.guides} of ${expected} guides under ${maxVertices} vertices`,
      });
  }
  return {
    key: contentKey({
      version: 6,
      source: options.sourceKey,
      guides,
      maxVertices,
      fractions: layers.map((layer) => layer.lodFractions),
    }),
    guides,
    details,
    diagnostics,
    representation:
      layers.every((layer) => layer.representation === "ribbons") && layers.length
        ? "opaque-ribbons"
        : layers.some((layer) => layer.representation === "ribbons")
          ? "mixed-opaque"
          : "opaque-tufts",
  };
}

function fractionFor(guide: CompiledGroomGuide, layers: GroomLayerSource[], detail: number): number {
  const fractions = layers.find((layer) => layer.id === guide.root.layer)?.lodFractions ?? [1, 0.5, 0.25];
  return fractions[Math.min(detail, fractions.length - 1)] ?? 1;
}
function selectGuide(
  guide: CompiledGroomGuide,
  layers: GroomLayerSource[],
  detail: number,
  protectedRoots: Set<string>,
): boolean {
  const ordinal = Number(guide.root.id.slice(guide.root.id.lastIndexOf("-") + 1));
  const fraction = fractionFor(guide, layers, detail);
  return fraction > 0 && (protectedRoots.has(guide.root.id) || radicalInverse(ordinal) < fraction);
}
function buildDetail(
  guides: CompiledGroomGuide[],
  label: CompiledGroomDetail["label"],
  detail: number,
  layers: GroomLayerSource[],
  protectedRoots: Set<string>,
  segments: number,
  sides: number,
  maxVertices: number,
): CompiledGroomDetail {
  const positions: number[] = [],
    normals: number[] = [],
    indices: number[] = [],
    colors: number[] = [],
    sourceIds: string[] = [],
    vertexGuides: number[] = [],
    guideIds: string[] = [],
    groups: NonNullable<MeshData["materialGroups"]> = [];
  const min: Vec3 = [Infinity, Infinity, Infinity],
    max: Vec3 = [-Infinity, -Infinity, -Infinity];
  const vertex = (p: Vec3, n: Vec3, color: Vec3, guide: CompiledGroomGuide, guideIndex: number) => {
    const index = positions.length / 3;
    positions.push(...p);
    normals.push(...n);
    colors.push(...color);
    sourceIds.push(guide.root.id);
    vertexGuides.push(guideIndex);
    for (let axis = 0; axis < 3; axis++) {
      min[axis] = Math.min(min[axis], p[axis]);
      max[axis] = Math.max(max[axis], p[axis]);
    }
    return index;
  };
  const selected = guides.map((guide) => selectGuide(guide, layers, detail, protectedRoots));
  const layerCounts = new Map(
    layers.map((layer) => [
      layer.id,
      {
        source: guides.filter((guide) => guide.root.layer === layer.id).length,
        selected: guides.filter((guide, index) => selected[index] && guide.root.layer === layer.id).length,
      },
    ]),
  );
  let maxGuideInterpolationError = 0;
  const realizations = new Set<"tufts" | "ribbons">();
  for (let guideIndex = 0; guideIndex < guides.length; guideIndex++) {
    const guide = guides[guideIndex];
    const ribbon = guide.representation === "ribbons";
    const guideSegments = ribbon ? Math.max(1, segments / 2) : segments;
    const guideSides = ribbon ? 4 : sides;
    const verticesPerGuide = guideSegments * guideSides + 2;
    if (!selected[guideIndex]) continue;
    const counts = layerCounts.get(guide.root.layer);
    const stride = counts ? counts.source / Math.max(1, counts.selected) : 1;
    if (positions.length / 3 + verticesPerGuide > maxVertices) break;
    guideIds.push(guide.root.id);
    realizations.add(ribbon ? "ribbons" : "tufts");
    for (let point = 0; point < 9; point++) {
      const stride = 8 / guideSegments,
        start = Math.min(8 - stride, Math.floor(point / stride) * stride);
      const approximate = mix(guide.points[start], guide.points[start + stride], (point - start) / stride);
      maxGuideInterpolationError = Math.max(
        maxGuideInterpolationError,
        Math.hypot(...sub(guide.points[point], approximate)),
      );
    }
    let widthDirection = guide.widthDirection ?? ([1, 0, 0] as Vec3);
    const start = positions.length / 3,
      indexStart = indices.length;
    for (let ring = 0; ring < guideSegments; ring++) {
      const pointIndex = ring * (8 / guideSegments),
        t = ring / guideSegments,
        axis = normalize(sub(guide.points[pointIndex + 8 / guideSegments], guide.points[pointIndex])),
        projectedWidth = sub(widthDirection, scale(axis, dot(widthDirection, axis))),
        fallbackWidth = normalize(cross(axis, Math.abs(axis[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0])),
        u = ribbon && Math.hypot(...projectedWidth) > 1e-8 ? normalize(projectedWidth) : fallbackWidth,
        v = cross(axis, u);
      if (ribbon) widthDirection = u;
      // Width compensation reduces coverage loss, capped to prevent distant inflated silhouettes.
      const radius =
        guide.width * 0.5 * Math.min(Math.sqrt(stride), 1.7) * (1 - t) ** (0.5 + guide.taper * 1.5);
      for (let side = 0; side < guideSides; side++) {
        const angle = (side / guideSides) * Math.PI * 2 + (ribbon ? Math.PI / 4 : 0),
          thickness = guide.ribbonThickness ?? 0.08,
          width = ribbon ? Math.sign(Math.cos(angle)) : Math.cos(angle),
          depth = ribbon ? Math.sign(Math.sin(angle)) : Math.sin(angle),
          radial = add(scale(u, width), scale(v, depth * (ribbon ? thickness : 1))),
          normal = ribbon ? normalize(add(scale(u, width * thickness), scale(v, depth))) : radial;
        vertex(
          add(guide.points[pointIndex], scale(radial, radius)),
          normal,
          mix(guide.rootColor, guide.tipColor, t),
          guide,
          guideIndex,
        );
      }
    }
    for (let ring = 0; ring < guideSegments - 1; ring++)
      for (let side = 0; side < guideSides; side++) {
        const a = start + ring * guideSides + side,
          b = start + ring * guideSides + ((side + 1) % guideSides);
        indices.push(a, b, a + guideSides, b, b + guideSides, a + guideSides);
      }
    const tip = vertex(
        guide.points[8],
        normalize(sub(guide.points[8], guide.points[7])),
        guide.tipColor,
        guide,
        guideIndex,
      ),
      root = vertex(guide.points[0], scale(guide.normal, -1), guide.rootColor, guide, guideIndex);
    for (let side = 0; side < guideSides; side++) {
      const a = start + (guideSegments - 1) * guideSides + side,
        b = start + (guideSegments - 1) * guideSides + ((side + 1) % guideSides);
      indices.push(a, b, tip, root, start + ((side + 1) % guideSides), start + side);
    }
    const previous = groups[groups.length - 1];
    if (previous?.material === guide.material) previous.count += indices.length - indexStart;
    else groups.push({ material: guide.material, start: indexStart, count: indices.length - indexStart });
  }
  const mesh: MeshData = {
    positions: new Float32Array(positions),
    normals: new Float32Array(normals),
    colors: new Float32Array(colors),
    indices: new Uint32Array(indices),
    sourceIds,
    materialGroups: groups,
    bounds: positions.length ? { min, max } : { min: [0, 0, 0], max: [0, 0, 0] },
  };
  const vertexGuideIndices = new Uint32Array(vertexGuides);
  const selectedIds = new Set(guideIds);
  const selectedGuides = guides.filter((guide) => selectedIds.has(guide.root.id));
  const tipBoundsError: Vec3 = [0, 0, 0];
  if (selectedGuides.length)
    for (let axis = 0; axis < 3; axis++) {
      const sourceTips = guides.map((guide) => guide.points[8][axis]);
      const selectedTips = selectedGuides.map((guide) => guide.points[8][axis]);
      tipBoundsError[axis] = Math.max(
        Math.abs(Math.min(...sourceTips) - Math.min(...selectedTips)),
        Math.abs(Math.max(...sourceTips) - Math.max(...selectedTips)),
      );
    }
  return {
    label,
    fidelity: {
      scope: "rest-guide-envelope",
      sourceGuides: guides.length,
      retainedGuides: guideIds.length,
      tipBoundsError: selectedGuides.length || !guides.length ? tipBoundsError : null,
      coverageError: null,
      drawGroups: groups.length,
      realizations: [...realizations],
      maxGuideInterpolationError,
    },
    mesh,
    vertexGuideIndices,
    guideIds,
    maxError: null,
    cost: {
      vertices: positions.length / 3,
      triangles: indices.length / 3,
      guides: guideIds.length,
      bytes:
        mesh.positions.byteLength +
        mesh.normals.byteLength +
        (mesh.colors?.byteLength ?? 0) +
        mesh.indices.byteLength +
        vertexGuideIndices.byteLength,
    },
  };
}
