import type {
  CompiledGroomDetail,
  CreatureAnchor,
  CreatureAnchorResolution,
  CreatureCorrectiveProduct,
  CreatureDefinition,
  CreatureSurfaceCoordinate,
} from "@wrela/model";

import {
  add,
  type CharacterDefinition,
  type CompiledCharacter,
  contentKey,
  creaturePatchPoint,
  cross,
  type Diagnostic,
  dot,
  evaluateCreatureSculpt,
  type Joint,
  type MeshData,
  normalize,
  type Quality,
  scale,
  sub,
  type Vec3,
} from "@wrela/model";

import { ProductCache } from "./cache";
import { cachedCreatureProduct, clearCreaturePreparationCaches } from "./creature-cache";
import { createCreatureBodyProjector, creatureBodyProjectionKey } from "./creature-projection";
import { refineCreatureSculptSurface } from "./creature-refinement";

export { creaturePreparationCacheMetrics } from "./creature-cache";

import { bindCreatureAppearanceFields } from "./groom-appearance-binding";
import { compileCreatureGroom } from "./groom-creature";
import { geometryKey, materialBindingKey } from "./products";

/** Charts are source coordinates, never generated triangle identities. All positions
 * returned by this module are creature rest-space metres. */
export type CreatureChartSample = CreatureSurfaceCoordinate & {
  position: Vec3;
  normal: Vec3;
  tangent: Vec3;
};
export type CreatureGeometry = {
  key: string;
  mesh: MeshData;
  coordinates: (CreatureSurfaceCoordinate | null)[];
  regions: (string | null)[];
  diagnostics: Diagnostic[];
};
const VERSION = 2;
const MAX_VERTICES = 250_000;
const cache = new ProductCache<CreatureGeometry>(32 * 1024 * 1024);
const clamp01 = (v: number) => Math.max(0, Math.min(1, v));
const lerp = (a: Vec3, b: Vec3, t: number): Vec3 => add(scale(a, 1 - t), scale(b, t));
const error = (code: string, message: string, node?: string): Diagnostic => ({
  severity: "error",
  code,
  message,
  node,
});

/** XYZ Euler rotation matches model-space authored region frames. */
export function creatureRotate(p: Vec3, r: Vec3): Vec3 {
  const cx = Math.cos(r[0]),
    sx = Math.sin(r[0]);
  const cy = Math.cos(r[1]),
    sy = Math.sin(r[1]);
  const cz = Math.cos(r[2]),
    sz = Math.sin(r[2]);
  const y = p[1] * cx - p[2] * sx,
    z = p[1] * sx + p[2] * cx;
  const x = p[0] * cy + z * sy,
    zz = -p[0] * sy + z * cy;
  return [x * cz - y * sz, x * sz + y * cz, zz];
}
export function creatureRegionPoint(creature: CreatureDefinition, regionId: string, p: Vec3): Vec3 {
  const region = creature.regions.find((r) => r.id === regionId);
  if (!region) throw new Error(`Unknown anatomical region ${regionId}`);
  return add(region.frame.position, creatureRotate(p, region.frame.rotation));
}
export function creatureRegionLocalPoint(creature: CreatureDefinition, regionId: string, p: Vec3): Vec3 {
  const region = creature.regions.find((r) => r.id === regionId);
  if (!region) throw new Error(`Unknown anatomical region ${regionId}`);
  const delta = sub(p, region.frame.position);
  const x = creatureRotate([1, 0, 0], region.frame.rotation),
    y = creatureRotate([0, 1, 0], region.frame.rotation),
    z = creatureRotate([0, 0, 1], region.frame.rotation);
  return [dot(delta, x), dot(delta, y), dot(delta, z)];
}
function rawChart(creature: CreatureDefinition, chartId: string, uvw: Vec3): Vec3 {
  const chart = creature.charts.find((c) => c.id === chartId);
  if (!chart) throw new Error(`Unknown creature chart ${chartId}`);
  if (uvw.some((v) => !Number.isFinite(v)) || uvw[0] < 0 || uvw[0] > 1 || uvw[1] < 0 || uvw[1] > 1)
    throw new Error(`Chart ${chartId} coordinates must have finite u,v in [0,1]`);
  const [u, v, w] = uvw;
  if (chart.kind === "patch") {
    const [a, b, c, d] = chart.points;
    const p = chart.controlOffsets ? creaturePatchPoint(chart, u, v) : lerp(lerp(a, b, u), lerp(c, d, u), v);
    const eps = 1e-4;
    const du = chart.controlOffsets
      ? sub(
          creaturePatchPoint(chart, Math.min(1, u + eps), v),
          creaturePatchPoint(chart, Math.max(0, u - eps), v),
        )
      : lerp(sub(b, a), sub(d, c), v);
    const dv = chart.controlOffsets
      ? sub(
          creaturePatchPoint(chart, u, Math.min(1, v + eps)),
          creaturePatchPoint(chart, u, Math.max(0, v - eps)),
        )
      : lerp(sub(c, a), sub(d, b), u);
    const n = normalize(cross(du, dv));
    if (Math.hypot(...n) < 0.5) throw new Error(`Degenerate patch ${chartId}`);
    return add(p, scale(n, w * chart.thickness * 0.5));
  }
  if (chart.points.length < 2 || chart.points.length !== chart.radii.length)
    throw new Error(`Sweep ${chartId} requires matching points and radii`);
  const lengths = chart.points.slice(1).map((p, i) => Math.hypot(...sub(p, chart.points[i])));
  if (lengths.some((l) => !Number.isFinite(l) || l < 1e-8))
    throw new Error(`Sweep ${chartId} contains coincident centers`);
  const total = lengths.reduce((s, l) => s + l, 0);
  let distance = u * total,
    segment = 0;
  while (segment < lengths.length - 1 && distance > lengths[segment]) distance -= lengths[segment++];
  const t = clamp01(distance / lengths[segment]);
  const center = lerp(chart.points[segment], chart.points[segment + 1], t);
  const tangentAt = (i: number): Vec3 =>
    normalize(sub(chart.points[Math.min(chart.points.length - 1, i + 1)], chart.points[Math.max(0, i - 1)]));
  const tangent = normalize(lerp(tangentAt(segment), tangentAt(segment + 1), t));
  if (Math.hypot(...tangent) < 0.5) throw new Error(`Sweep ${chartId} reverses onto itself`);
  // Parallel transport a frame from the first authored segment. Unlike choosing
  // a fresh world axis at every sample this does not introduce chart flips.
  const initial = tangentAt(0);
  let x = normalize(cross(Math.abs(initial[1]) < 0.9 ? [0, 1, 0] : [0, 0, 1], initial));
  let previous = initial;
  for (let i = 1; i <= segment + 1; i++) {
    const next = i === segment + 1 ? tangent : tangentAt(i);
    const axis = cross(previous, next),
      sine = Math.hypot(...axis),
      cosine = dot(previous, next);
    if (cosine < -0.9999) throw new Error(`Sweep ${chartId} has an ambiguous reversing frame`);
    if (sine > 1e-10) {
      const n = scale(axis, 1 / sine);
      x = add(add(scale(x, cosine), scale(cross(n, x), sine)), scale(n, dot(n, x) * (1 - cosine)));
    }
    previous = next;
  }
  const y = normalize(cross(tangent, x));
  const radiiA = chart.crossSections?.[segment] ?? [chart.radii[segment], chart.radii[segment]];
  const radiiB = chart.crossSections?.[segment + 1] ?? [chart.radii[segment + 1], chart.radii[segment + 1]];
  const rx = radiiA[0] * (1 - t) + radiiB[0] * t,
    ry = radiiA[1] * (1 - t) + radiiB[1] * t;
  const twist = (chart.twists?.[segment] ?? 0) * (1 - t) + (chart.twists?.[segment + 1] ?? 0) * t;
  const angle = v * Math.PI * 2 + twist;
  return add(center, add(scale(x, rx * Math.cos(angle) * w), scale(y, ry * Math.sin(angle) * w)));
}
function sculptPoint(creature: CreatureDefinition, regionId: string, p: Vec3, sourceId?: string): Vec3 {
  return evaluateCreatureSculpt(
    creature.sculpts.filter((s) => !s.nodeIds || (sourceId !== undefined && s.nodeIds.includes(sourceId))),
    regionId,
    p,
  );
}

function chartPoint(creature: CreatureDefinition, chartId: string, uvw: Vec3): Vec3 {
  const chart = creature.charts.find((c) => c.id === chartId);
  if (!chart) throw new Error(`Unknown creature chart ${chartId}`);
  const region = creature.regions.find((r) => r.id === chart.region);
  if (!region) throw new Error(`Chart ${chartId} references missing region ${chart.region}`);
  return add(
    region.frame.position,
    creatureRotate(
      sculptPoint(creature, region.id, rawChart(creature, chartId, uvw), chartId),
      region.frame.rotation,
    ),
  );
}
export function evaluateCreatureChart(
  creature: CreatureDefinition,
  chartId: string,
  coordinates: Vec3,
): CreatureChartSample {
  const chart = creature.charts.find((c) => c.id === chartId);
  if (!chart) throw new Error(`Unknown creature chart ${chartId}`);
  const p = chartPoint(creature, chartId, coordinates);
  if (p.some((component) => !Number.isFinite(component)))
    throw new Error(`Chart ${chartId} produced a non-finite position`);
  const eps = 1e-4,
    [u, v, w] = coordinates;
  const du = sub(
    chartPoint(creature, chartId, [Math.min(1, u + eps), v, w]),
    chartPoint(creature, chartId, [Math.max(0, u - eps), v, w]),
  );
  const before = chart.kind === "sweep" ? (v - eps + 1) % 1 : Math.max(0, v - eps);
  const after = chart.kind === "sweep" ? (v + eps) % 1 : Math.min(1, v + eps);
  const dv = sub(chartPoint(creature, chartId, [u, after, w]), chartPoint(creature, chartId, [u, before, w]));
  // Sweep v increases around the axis: dv x du points outward.
  const normal = normalize(chart.kind === "sweep" ? cross(dv, du) : cross(du, dv));
  if (normal.some((component) => !Number.isFinite(component)) || Math.hypot(...normal) < 0.5)
    throw new Error(`Chart ${chartId} is singular at the requested coordinates`);
  return {
    region: chart.region,
    chart: chart.id,
    chartRevision: chart.revision,
    coordinates: [...coordinates],
    position: p,
    normal,
    tangent: normalize(du),
  };
}

export function resolveCreatureAnchor(
  creature: CreatureDefinition,
  anchor: CreatureAnchor,
): CreatureAnchorResolution {
  const invalid = (code: string, message: string): CreatureAnchorResolution => ({
    status: "invalid",
    residual: null,
    confidence: 0,
    diagnostics: [error(code, message, anchor.id)],
  });
  const region = creature.regions.find((r) => r.id === anchor.region);
  if (!region)
    return invalid(
      "creature.anchor.region",
      `Anchor ${anchor.id} lost anatomical region ${anchor.region}; explicit repair is required.`,
    );
  if (!anchor.chart)
    return {
      status: "resolved",
      position: creatureRegionPoint(creature, region.id, add(anchor.coordinates, [0, anchor.offset, 0])),
      normal: creatureRotate([0, 1, 0], region.frame.rotation),
      residual: 0,
      confidence: 1,
      diagnostics: [],
    };
  const chart = creature.charts.find((c) => c.id === anchor.chart);
  if (!chart || chart.region !== anchor.region)
    return invalid("creature.anchor.chart", `Anchor ${anchor.id} has no compatible anatomical chart.`);
  if (anchor.chartRevision !== chart.revision)
    return invalid(
      "creature.anchor.topology",
      `Anchor ${anchor.id} chart revision ${anchor.chartRevision} does not match ${chart.revision}; topology transfer was not established.`,
    );
  try {
    const sample = evaluateCreatureChart(creature, chart.id, anchor.coordinates);
    return {
      status: "resolved",
      position: add(sample.position, scale(sample.normal, anchor.offset)),
      normal: sample.normal,
      residual: 0,
      confidence: 1,
      diagnostics: [],
    };
  } catch (cause) {
    return invalid("creature.anchor.singular", String(cause));
  }
}

/** Bounded closest-chart repair. Never searches another anatomical region and
 * never resolves equal-distance competing sheets by array order. */
export function projectCreatureAnchor(
  creature: CreatureDefinition,
  region: string,
  position: Vec3,
  tolerance: number,
  steps = 16,
): CreatureAnchorResolution & { coordinate?: CreatureSurfaceCoordinate } {
  if (!(tolerance > 0) || !Number.isFinite(tolerance))
    throw new Error("Projection requires a positive finite tolerance");
  const n = Math.max(4, Math.min(64, Math.floor(steps)));
  const candidates: { sample: CreatureChartSample; distance: number }[] = [];
  for (const chart of creature.charts.filter((c) => c.region === region)) {
    const grid: { sample: CreatureChartSample; distance: number }[] = [];
    const coordinateDistance = (a: CreatureChartSample, b: CreatureChartSample) => {
      const du = Math.abs(a.coordinates[0] - b.coordinates[0]);
      const raw = Math.abs(a.coordinates[1] - b.coordinates[1]);
      const dv = chart.kind === "sweep" ? Math.min(raw, 1 - raw) : raw;
      return Math.hypot(du, dv);
    };
    for (let i = 0; i <= n; i++)
      for (let j = 0; j <= n; j++) {
        try {
          const sample = evaluateCreatureChart(creature, chart.id, [
            i / n,
            j / n,
            chart.kind === "sweep" ? 1 : 0,
          ]);
          grid.push({ sample, distance: Math.hypot(...sub(sample.position, position)) });
        } catch {
          /* Singular chart samples are invalid candidates. */
        }
      }
    grid.sort((a, b) => a.distance - b.distance);
    const seeds: typeof grid = [];
    for (const candidate of grid) {
      if (seeds.every((other) => coordinateDistance(candidate.sample, other.sample) > 2 / n))
        seeds.push(candidate);
      if (seeds.length === 8) break;
    }
    for (const seed of seeds) {
      let best = seed,
        step = 1 / n;
      for (let iteration = 0; iteration < 14; iteration++) {
        const center: Vec3 = [...best.sample.coordinates];
        for (const du of [-step, 0, step])
          for (const dv of [-step, 0, step]) {
            const v = chart.kind === "sweep" ? (center[1] + dv + 1) % 1 : clamp01(center[1] + dv);
            try {
              const sample = evaluateCreatureChart(creature, chart.id, [
                clamp01(center[0] + du),
                v,
                center[2],
              ]);
              const distance = Math.hypot(...sub(sample.position, position));
              if (distance < best.distance) best = { sample, distance };
            } catch {
              /* Keep the current valid candidate. */
            }
          }
        step *= 0.5;
      }
      const existing = candidates.find(
        (other) => other.sample.chart === chart.id && coordinateDistance(best.sample, other.sample) < 0.001,
      );
      if (existing) {
        if (best.distance < existing.distance) {
          existing.sample = best.sample;
          existing.distance = best.distance;
        }
      } else candidates.push(best);
    }
  }
  candidates.sort((a, b) => a.distance - b.distance);
  const best = candidates[0];
  if (!best || best.distance > tolerance)
    return {
      status: "invalid",
      residual: best?.distance ?? null,
      confidence: 0,
      diagnostics: [
        error(
          "creature.anchor.projection",
          "No compatible surface lies within the declared repair tolerance.",
          region,
        ),
      ],
    };
  const ambiguous = candidates
    .slice(1)
    .some((other) => Math.abs(other.distance - best.distance) <= Math.max(1e-6, tolerance * 0.01));
  if (ambiguous)
    return {
      status: "ambiguous",
      residual: best.distance,
      confidence: 0,
      diagnostics: [
        error(
          "creature.anchor.ambiguous",
          "Several distinct anatomical chart locations satisfy the projection; select the intended chart and neighborhood explicitly.",
          region,
        ),
      ],
    };
  return {
    status: "resolved",
    position: best.sample.position,
    normal: best.sample.normal,
    residual: best.distance,
    confidence: Math.max(0, 1 - best.distance / tolerance),
    diagnostics: [],
    coordinate: best.sample,
  };
}

export function creatureGeometryProductKeys(
  creature: CreatureDefinition,
  quality: Quality = "review",
  joints: Joint[] = [],
) {
  const geometry = contentKey({
    version: VERSION,
    quality,
    regions: creature.regions.map((r) => ({ id: r.id, frame: r.frame })),
    charts: creature.charts.map(({ material: _material, ...chart }) => chart),
    sculpts: creature.sculpts,
  });
  return {
    geometry,
    correspondence: contentKey({
      version: VERSION,
      regions: creature.regions.map((r) => [r.id, r.nodeIds]),
      charts: creature.charts.map((c) => [c.id, c.region, c.revision]),
      anchors: creature.anchors,
    }),
    binding: contentKey({
      version: VERSION,
      geometry,
      joints,
      rules: creature.influenceRules,
      regions: creature.regions.map((r) => [r.id, r.nodeIds, r.jointIds]),
    }),
    correctives: contentKey({ version: VERSION, geometry, correctives: creature.correctives }),
    material: contentKey({
      version: VERSION,
      regions: creature.regions.map((r) => [r.id, r.material]),
      charts: creature.charts.map((c) => [c.id, c.material]),
      cloth: creature.cloth.map((c) => [c.chart, c.material]),
    }),
  };
}

export function compileCreatureGeometry(
  creature: CreatureDefinition,
  quality: Quality = "review",
  defaultMaterial = "",
): CreatureGeometry {
  const key = creatureGeometryProductKeys(creature, quality).geometry;
  const cacheKey = key;
  const cached = cache.get(cacheKey);
  if (cached) {
    const result = structuredClone(cached);
    for (const group of result.mesh.materialGroups ?? []) {
      const chart = creature.charts.find(
        (item) => item.id === result.mesh.sourceIds?.[result.mesh.indices[group.start]],
      );
      if (chart)
        group.material =
          creature.cloth.find((cloth) => cloth.chart === chart.id)?.material ??
          chart.material ??
          creature.regions.find((region) => region.id === chart.region)?.material ??
          defaultMaterial;
    }
    return result;
  }
  const positions: number[] = [],
    normals: number[] = [],
    indices: number[] = [];
  const coordinates: CreatureSurfaceCoordinate[] = [],
    regions: string[] = [],
    sourceIds: string[] = [];
  const materialGroups: NonNullable<MeshData["materialGroups"]> = [],
    diagnostics: Diagnostic[] = [];
  const defaultSegments = quality === "interactive" ? 12 : quality === "review" ? 24 : 48;
  const vertex = (sample: CreatureChartSample, overrideNormal?: Vec3) => {
    if (coordinates.length >= MAX_VERTICES)
      throw new Error(`Creature surface exceeds ${MAX_VERTICES} vertices; reduce chart count or quality.`);
    const id = positions.length / 3;
    positions.push(...sample.position);
    normals.push(...(overrideNormal ?? sample.normal));
    coordinates.push({
      region: sample.region,
      chart: sample.chart,
      chartRevision: sample.chartRevision,
      coordinates: sample.coordinates,
    });
    regions.push(sample.region);
    sourceIds.push(sample.chart);
    return id;
  };
  const tri = (a: number, b: number, c: number) => {
    const p = (i: number): Vec3 => [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
    const normal = cross(sub(p(b), p(a)), sub(p(c), p(a)));
    if (Math.hypot(...normal) < 1e-15) return;
    const expected: Vec3 = [
      normals[a * 3] + normals[b * 3] + normals[c * 3],
      normals[a * 3 + 1] + normals[b * 3 + 1] + normals[c * 3 + 1],
      normals[a * 3 + 2] + normals[b * 3 + 2] + normals[c * 3 + 2],
    ];
    if (dot(normal, expected) >= 0) indices.push(a, b, c);
    else indices.push(a, c, b);
  };
  for (const chart of creature.charts) {
    if (chart.realization === "correspondence-only") continue;
    const start = indices.length;
    const detail = Math.min(
      Infinity,
      ...creature.sculpts
        .filter((s) => s.region === chart.region && (!s.nodeIds || s.nodeIds.includes(chart.id)))
        .map((s) => s.detail?.maxEdgeLength ?? Infinity),
    );
    const circumference =
      chart.kind === "sweep"
        ? 2 * Math.PI * Math.max(...chart.radii, ...(chart.crossSections?.flat() ?? []))
        : 0;
    const requestedSegments = Math.max(defaultSegments, Math.ceil(circumference / detail));
    const segments = Math.min(128, requestedSegments);
    if (requestedSegments > 128)
      diagnostics.push({
        severity: "warning",
        code: "creature-sculpt-detail",
        node: chart.id,
        message: "Chart circumferential detail reached its 128-segment budget",
      });
    if (chart.kind === "sweep") {
      // Refine by source path sections, independently of legacy extraction grid.
      const pathLength = chart.points
        .slice(1)
        .reduce((sum, p, i) => sum + Math.hypot(...sub(p, chart.points[i])), 0);
      const wantedRows = Math.max(
        4,
        (chart.points.length - 1) * (quality === "interactive" ? 3 : quality === "review" ? 6 : 12),
        Math.ceil(pathLength / detail),
      );
      const rows = Math.min(256, wantedRows);
      if (wantedRows > 256)
        diagnostics.push({
          severity: "warning",
          code: "creature-sculpt-detail",
          node: chart.id,
          message: "Chart path detail reached its 256-row budget",
        });
      const rings: number[][] = [];
      for (let i = 0; i <= rows; i++) {
        const ring: number[] = [];
        for (let j = 0; j <= segments; j++)
          ring.push(vertex(evaluateCreatureChart(creature, chart.id, [i / rows, j / segments, 1])));
        rings.push(ring);
      }
      for (let i = 0; i < rows; i++)
        for (let j = 0; j < segments; j++) {
          tri(rings[i][j], rings[i + 1][j], rings[i + 1][j + 1]);
          tri(rings[i][j], rings[i + 1][j + 1], rings[i][j + 1]);
        }
      if (chart.caps !== false)
        for (const end of [0, 1]) {
          const rim = evaluateCreatureChart(creature, chart.id, [end, 0, 1]);
          const center = chartPoint(creature, chart.id, [end, 0, 0]);
          const capNormal = scale(rim.tangent, end === 0 ? -1 : 1);
          const middle = vertex({ ...rim, coordinates: [end, 0, 0], position: center }, capNormal);
          const cap: number[] = [];
          for (let j = 0; j <= segments; j++)
            cap.push(vertex(evaluateCreatureChart(creature, chart.id, [end, j / segments, 1]), capNormal));
          for (let j = 0; j < segments; j++) tri(middle, cap[j], cap[j + 1]);
        }
    } else {
      const edgeLength = Math.max(
        ...[
          [0, 1],
          [0, 2],
          [1, 3],
          [2, 3],
        ].map(([a, b]) => Math.hypot(...sub(chart.points[a], chart.points[b]))),
      );
      const wanted = Math.max(
        2,
        Math.ceil(segments / 2),
        Math.ceil((edgeLength * Math.SQRT2) / detail),
        chart.controlOffsets
          ? Math.max(chart.controlOffsets.length - 1, chart.controlOffsets[0].length - 1) * 2
          : 0,
      );
      const n = Math.min(128, wanted),
        sheets: number[][][] = [];
      if (wanted > 128)
        diagnostics.push({
          severity: "warning",
          code: "creature-sculpt-detail",
          node: chart.id,
          message: "Chart patch detail reached its 128-row budget",
        });
      for (const side of chart.thickness > 0 ? [-1, 1] : [0]) {
        const grid: number[][] = [];
        for (let i = 0; i <= n; i++) {
          const row: number[] = [];
          for (let j = 0; j <= n; j++) {
            const sample = evaluateCreatureChart(creature, chart.id, [i / n, j / n, side]);
            row.push(vertex(sample, side === -1 ? scale(sample.normal, -1) : sample.normal));
          }
          grid.push(row);
        }
        for (let i = 0; i < n; i++)
          for (let j = 0; j < n; j++) {
            tri(grid[i][j], grid[i + 1][j], grid[i + 1][j + 1]);
            tri(grid[i][j], grid[i + 1][j + 1], grid[i][j + 1]);
          }
        sheets.push(grid);
      }
      if (sheets.length === 2) {
        const boundary = (grid: number[][]) => [
          ...grid[0],
          ...grid.slice(1).map((row) => row[n]),
          ...grid[n].slice(0, n).reverse(),
          ...grid
            .slice(1, n)
            .reverse()
            .map((row) => row[0]),
        ];
        const low = boundary(sheets[0]),
          high = boundary(sheets[1]);
        // Side walls are wound from the patch boundary; averaged opposing sheet
        // normals cannot determine wall orientation.
        for (let i = 0; i < low.length; i++) {
          const j = (i + 1) % low.length;
          indices.push(low[i], high[i], high[j], low[i], high[j], low[j]);
        }
      }
    }
    if (indices.length > start)
      materialGroups.push({
        material:
          creature.cloth.find((cloth) => cloth.chart === chart.id)?.material ??
          chart.material ??
          creature.regions.find((r) => r.id === chart.region)?.material ??
          defaultMaterial,
        start,
        count: indices.length - start,
      });
  }
  const min: Vec3 = [Infinity, Infinity, Infinity],
    max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < positions.length; i++) {
    min[i % 3] = Math.min(min[i % 3], positions[i]);
    max[i % 3] = Math.max(max[i % 3], positions[i]);
  }
  if (!positions.length) {
    min.fill(0);
    max.fill(0);
  }
  const mesh: MeshData = {
    positions: new Float32Array(positions),
    normals: new Float32Array(normals),
    indices: new Uint32Array(indices),
    sourceIds,
    materialGroups,
    bounds: { min, max },
  };
  const result: CreatureGeometry = { key, mesh, coordinates, regions, diagnostics };
  cache.set(
    cacheKey,
    result,
    mesh.positions.byteLength + mesh.normals.byteLength + mesh.indices.byteLength + coordinates.length * 100,
  );
  return structuredClone(result);
}
export function clearCreatureCompilerCache() {
  cache.clear();
  clearCreaturePreparationCaches();
}
export function creatureCompilerCacheMetrics() {
  return cache.metrics;
}

/** Local sculpt fields also operate on the legacy extracted body. Original
 * extraction is immutable/cacheable; only the anatomically scoped product changes. */
export function sculptLegacyCreatureGeometry(base: MeshData, creature: CreatureDefinition): MeshData {
  if (!creature.sculpts.length) return base;
  const positions = base.positions.slice(),
    normals = base.normals.slice();
  const min: Vec3 = [Infinity, Infinity, Infinity],
    max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (let vertex = 0; vertex < positions.length / 3; vertex++) {
    const regions = creature.regions.filter((r) => r.nodeIds.includes(base.sourceIds?.[vertex] ?? ""));
    const mounted = creature.attachments.some((attachment) =>
      attachment.nodeIds.includes(base.sourceIds?.[vertex] ?? ""),
    );
    const relevant = mounted
      ? []
      : regions.filter((region) => creature.sculpts.some((stroke) => stroke.region === region.id));
    if (relevant.length > 1)
      throw new Error(
        `Sculpt vertex has ambiguous anatomical ownership: ${relevant.map((r) => r.id).join(", ")}`,
      );
    if (relevant.length) {
      const region = relevant[0].id;
      const deform = (p: Vec3) =>
        creatureRegionPoint(
          creature,
          region,
          sculptPoint(
            creature,
            region,
            creatureRegionLocalPoint(creature, region, p),
            base.sourceIds?.[vertex],
          ),
        );
      const p: Vec3 = [positions[vertex * 3], positions[vertex * 3 + 1], positions[vertex * 3 + 2]],
        changed = deform(p);
      if (Math.hypot(...sub(changed, p)) > 1e-12) {
        const n: Vec3 = [normals[vertex * 3], normals[vertex * 3 + 1], normals[vertex * 3 + 2]];
        const t = normalize(cross(n, Math.abs(n[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0])),
          b = cross(n, t);
        const eps = 1e-4;
        const dt = sub(deform(add(p, scale(t, eps))), deform(add(p, scale(t, -eps))));
        const db = sub(deform(add(p, scale(b, eps))), deform(add(p, scale(b, -eps))));
        positions.set(changed, vertex * 3);
        normals.set(normalize(cross(dt, db)), vertex * 3);
      }
    }
    for (let axis = 0; axis < 3; axis++) {
      min[axis] = Math.min(min[axis], positions[vertex * 3 + axis]);
      max[axis] = Math.max(max[axis], positions[vertex * 3 + axis]);
    }
  }
  if (!positions.length) {
    min.fill(0);
    max.fill(0);
  }
  return { ...base, positions, normals, bounds: { min, max } };
}

export function combineCreatureGeometry(
  base: MeshData,
  generated: CreatureGeometry,
  creature: CreatureDefinition,
  defaultMaterial: string,
): CreatureGeometry {
  const offset = base.positions.length / 3;
  const floats = (a: Float32Array, b: Float32Array) => {
    const result = new Float32Array(a.length + b.length);
    result.set(a);
    result.set(b, a.length);
    return result;
  };
  const indices = new Uint32Array(base.indices.length + generated.mesh.indices.length);
  indices.set(base.indices);
  indices.set(
    generated.mesh.indices.map((i) => i + offset),
    base.indices.length,
  );
  const legacyRegions = Array.from({ length: offset }, (_, i) => {
    const matches = creature.regions.filter((r) => r.nodeIds.includes(base.sourceIds?.[i] ?? ""));
    return matches.length === 1 ? matches[0].id : null;
  });
  const materialGroups = [
    ...(base.materialGroups ?? [{ material: defaultMaterial, start: 0, count: base.indices.length }]),
    ...(generated.mesh.materialGroups ?? []).map((group) => ({
      ...group,
      start: group.start + base.indices.length,
    })),
  ];
  const mesh: MeshData = {
    positions: floats(base.positions, generated.mesh.positions),
    normals: floats(base.normals, generated.mesh.normals),
    indices,
    sourceIds: [
      ...(base.sourceIds ?? Array.from({ length: offset }, () => "legacy")),
      ...(generated.mesh.sourceIds ?? []),
    ],
    materialGroups,
    bounds: {
      min: base.bounds.min.map((v, i) =>
        Math.min(v, generated.mesh.positions.length ? generated.mesh.bounds.min[i] : v),
      ) as Vec3,
      max: base.bounds.max.map((v, i) =>
        Math.max(v, generated.mesh.positions.length ? generated.mesh.bounds.max[i] : v),
      ) as Vec3,
    },
  };
  // One index range per material gives each rendered source/material pair one
  // stable identity and avoids repeated draws for independent source components.
  const buckets = new Map<string, number[]>();
  for (const group of materialGroups) {
    const bucket = buckets.get(group.material) ?? [];
    for (let i = group.start; i < group.start + group.count; i++) bucket.push(indices[i]);
    if (bucket.length) buckets.set(group.material, bucket);
  }
  let rangeStart = 0;
  mesh.materialGroups = [];
  for (const [material, bucket] of buckets) {
    mesh.indices.set(bucket, rangeStart);
    mesh.materialGroups.push({ material, start: rangeStart, count: bucket.length });
    rangeStart += bucket.length;
  }
  if (base.colors || generated.mesh.colors)
    mesh.colors = floats(
      base.colors ?? new Float32Array(base.positions.length).fill(1),
      generated.mesh.colors ?? new Float32Array(generated.mesh.positions.length).fill(1),
    );
  if (mesh.positions.length / 3 > MAX_VERTICES)
    throw new Error("Combined creature surface exceeds vertex budget");
  return {
    key: contentKey([generated.key, base.positions, base.indices]),
    mesh,
    coordinates: [...Array.from({ length: offset }, () => null), ...generated.coordinates],
    regions: [...legacyRegions, ...generated.regions],
    diagnostics: [...generated.diagnostics],
  };
}

export function bindCreatureGeometry(
  joints: Joint[],
  creature: CreatureDefinition,
  geometry: CreatureGeometry,
) {
  if (!joints.length || joints.length > 65535) throw new Error("Creature binding needs 1–65535 joints");
  const count = geometry.mesh.positions.length / 3,
    weights = new Float32Array(count * 4),
    jointIndices = new Uint16Array(count * 4);
  const diagnostics: Diagnostic[] = [],
    byId = new Map(joints.map((j) => [j.id, j]));
  let outside = 0;
  for (let vertex = 0; vertex < count; vertex++) {
    const region = creature.regions.find((r) => r.id === geometry.regions[vertex]);
    const rules = creature.influenceRules.filter((r) => r.region === region?.id);
    const attachedJoint = creature.attachments.find((attachment) =>
      attachment.nodeIds.includes(geometry.mesh.sourceIds?.[vertex] ?? ""),
    )?.rigidJoint;
    const rigid = attachedJoint
      ? [attachedJoint]
      : rules.map((r) => r.rigidJoint).filter((id): id is string => !!id);
    if (new Set(rigid).size > 1) throw new Error(`Conflicting rigid influence ownership in ${region?.id}`);
    const allowed = rules.flatMap((r) => r.allowedJoints),
      excluded = new Set(rules.flatMap((r) => r.excludedJoints));
    const p: Vec3 = [
      geometry.mesh.positions[vertex * 3],
      geometry.mesh.positions[vertex * 3 + 1],
      geometry.mesh.positions[vertex * 3 + 2],
    ];
    const candidates = joints
      .flatMap((joint, index) => {
        if (
          excluded.has(joint.id) ||
          (rigid.length
            ? joint.id !== rigid[0]
            : allowed.length
              ? !allowed.includes(joint.id)
              : region?.jointIds.length
                ? !region.jointIds.includes(joint.id)
                : false)
        )
          return [];
        const parent = joint.parent ? byId.get(joint.parent) : undefined;
        const a = parent?.position ?? joint.position,
          ab = sub(joint.position, a);
        const t = clamp01(dot(sub(p, a), ab) / (dot(ab, ab) || 1));
        const distance =
          Math.hypot(...sub(p, add(a, scale(ab, t)))) /
          Math.max(1e-6, (parent?.radius ?? joint.radius) * (1 - t) + joint.radius * t);
        return [{ index, distance, weight: rigid.length ? 1 : Math.max(0, 1 - distance) ** 2 }];
      })
      .sort((a, b) => b.weight - a.weight || a.distance - b.distance || a.index - b.index)
      .slice(0, 4);
    if (!candidates.length)
      throw new Error(
        `No permitted influence for creature region ${region?.id ?? "unmapped"}; binding cannot cross anatomical exclusions.`,
      );
    let total = candidates.reduce((sum, c) => sum + c.weight, 0);
    if (total <= 1e-12) {
      candidates[0].weight = 1;
      total = 1;
      outside++;
    }
    for (let i = 0; i < candidates.length; i++) {
      jointIndices[vertex * 4 + i] = candidates[i].index;
      weights[vertex * 4 + i] = candidates[i].weight / total;
    }
  }
  if (outside)
    diagnostics.push({
      severity: "warning",
      code: "creature.binding.outside-envelope",
      message: `${outside} vertices use the closest anatomically permitted joint outside its envelope.`,
    });
  return { jointIndices, weights, diagnostics };
}

export function compileCreatureCorrectives(
  creature: CreatureDefinition,
  geometry: CreatureGeometry,
): CreatureCorrectiveProduct[] {
  const mountedPoints = new Map<string, { region: string; position: Vec3 }>();
  for (const attachment of creature.attachments) {
    const anchor = creature.anchors.find((item) => item.id === attachment.anchor);
    const resolved = anchor ? resolveCreatureAnchor(creature, anchor) : undefined;
    if (anchor && resolved?.status === "resolved" && resolved.position)
      for (const node of attachment.nodeIds)
        mountedPoints.set(node, { region: anchor.region, position: resolved.position });
  }
  return creature.correctives.map((corrective) => {
    const region = creature.regions.find((r) => r.id === corrective.region);
    if (!region) throw new Error(`Corrective ${corrective.id} lost region ${corrective.region}`);
    const center = add(region.frame.position, creatureRotate(corrective.center, region.frame.rotation));
    const displacement = creatureRotate(corrective.displacement, region.frame.rotation);
    const vertices: number[] = [],
      values: number[] = [];
    for (let i = 0; i < geometry.regions.length; i++) {
      const mounted = mountedPoints.get(geometry.mesh.sourceIds?.[i] ?? "");
      if ((mounted?.region ?? geometry.regions[i]) !== corrective.region) continue;
      // Mounted components move rigidly with their anchored material point; a
      // shoulder bulge must not bend a metal plate or shear its attachment.
      const p: Vec3 = mounted?.position ?? [
        geometry.mesh.positions[i * 3],
        geometry.mesh.positions[i * 3 + 1],
        geometry.mesh.positions[i * 3 + 2],
      ];
      const d = Math.hypot(...sub(p, center)) / corrective.radius;
      if (d >= 1) continue;
      vertices.push(i);
      values.push(...scale(displacement, (1 - d * d) ** 2));
    }
    return {
      id: corrective.id,
      region: corrective.region,
      joint: corrective.joint,
      axis: corrective.axis,
      angle: corrective.angle,
      vertices: new Uint32Array(vertices),
      displacements: new Float32Array(values),
    };
  });
}

/** Returns a fresh rest surface; callers skin it after applying pose corrections.
 * Drivers interpolate from rest to their authored angle and saturate beyond it. */
export function applyCreatureCorrectives(
  mesh: MeshData,
  products: CreatureCorrectiveProduct[],
  rotations: Record<string, Vec3>,
): MeshData {
  const active = products.some((product) => {
    const axis = product.axis === "x" ? 0 : product.axis === "y" ? 1 : 2;
    return Math.abs(product.angle) > 1e-8 && (rotations[product.joint]?.[axis] ?? 0) / product.angle > 0;
  });
  if (!active) return mesh;
  const positions = mesh.positions.slice();
  for (const product of products) {
    const axis = product.axis === "x" ? 0 : product.axis === "y" ? 1 : 2;
    const angle = rotations[product.joint]?.[axis] ?? 0;
    const activation = Math.abs(product.angle) > 1e-8 ? clamp01(angle / product.angle) : 0;
    if (!activation) continue;
    for (let i = 0; i < product.vertices.length; i++)
      for (let a = 0; a < 3; a++)
        positions[product.vertices[i] * 3 + a] += product.displacements[i * 3 + a] * activation;
  }
  const normals = new Float32Array(positions.length),
    min: Vec3 = [Infinity, Infinity, Infinity],
    max: Vec3 = [-Infinity, -Infinity, -Infinity];
  const p = (i: number): Vec3 => [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]];
  for (let i = 0; i < mesh.indices.length; i += 3) {
    const a = mesh.indices[i],
      b = mesh.indices[i + 1],
      c = mesh.indices[i + 2];
    const normal = cross(sub(p(b), p(a)), sub(p(c), p(a)));
    for (const v of [a, b, c]) for (let axis = 0; axis < 3; axis++) normals[v * 3 + axis] += normal[axis];
  }
  for (let i = 0; i < positions.length; i += 3) {
    const normal = normalize([normals[i], normals[i + 1], normals[i + 2]]);
    normals.set(normal, i);
    for (let axis = 0; axis < 3; axis++) {
      min[axis] = Math.min(min[axis], positions[i + axis]);
      max[axis] = Math.max(max[axis], positions[i + axis]);
    }
  }
  return { ...mesh, positions, normals, bounds: { min, max } };
}

/** Bind anatomical appearance to triangles while preserving source vertices and
 * correspondence. Region-local masks deliberately do not require UV unwraps. */
export function bindCreatureAppearance(
  creature: CreatureDefinition,
  geometry: CreatureGeometry,
  defaultMaterial: string,
) {
  return bindCreatureAppearanceFields(
    creature,
    geometry,
    defaultMaterial,
    (region, point) => creatureRegionPoint(creature, region, point),
    (region, point) => creatureRegionLocalPoint(creature, region, point),
    (region, id) => {
      const anchor = creature.anchors.find((item) => item.id === id && item.region === region);
      const resolved = anchor ? resolveCreatureAnchor(creature, anchor) : undefined;
      return resolved?.status === "resolved" && resolved.position
        ? creatureRegionLocalPoint(creature, region, resolved.position)
        : undefined;
    },
  );
}

export function finalizeCreatureCharacter(
  doc: CharacterDefinition,
  base: CompiledCharacter,
  quality: Quality = "review",
): CompiledCharacter {
  const creature = doc.creature;
  if (!creature) return base;
  const shapeKeys = creatureGeometryProductKeys(creature, quality);
  const rawBodyKey = contentKey({
    version: 1,
    field: geometryKey(doc, quality),
    chart: shapeKeys.geometry,
    regions: creature.regions.map((region) => [region.id, region.nodeIds]),
    attachments: creature.attachments,
    anchors: creature.anchors,
    materials: [materialBindingKey(doc), shapeKeys.material],
  });
  const rawBody = cachedCreatureProduct("body", rawBodyKey, () => {
    const refined = refineCreatureSculptSurface(base.mesh, creature);
    const combined = combineCreatureGeometry(
      sculptLegacyCreatureGeometry(refined.mesh, creature),
      compileCreatureGeometry(creature, quality, doc.material),
      creature,
      doc.material,
    );
    combined.diagnostics.push(...refined.diagnostics);
    return combined;
  });
  const appearanceKey = contentKey({
    version: 2,
    body: rawBodyKey,
    appearance: creature.appearance,
    anchors: creature.anchors,
    // These are precisely the groom properties used to compile fiber materials.
    fibers: creature.grooms.map((groom) => [groom.id, groom.region, groom.direction]),
  });
  const appearance = cachedCreatureProduct("appearance", appearanceKey, () =>
    bindCreatureAppearance(creature, rawBody, doc.material),
  );
  const body = appearance.geometry,
    bodyCount = body.regions.length;
  const projectionKey = creature.grooms.some((groom) => groom.rootProjection)
    ? creatureBodyProjectionKey(body)
    : undefined;
  const groomKey = contentKey({
    version: 1,
    projectionKey,
    projectionScope: projectionKey
      ? {
          nodes: doc.field.nodes.map((node) => [node.id, node.children]),
          regions: creature.regions.map((region) => [region.id, region.nodeIds]),
        }
      : undefined,
    shape: shapeKeys.geometry,
    grooms: creature.grooms,
    growth: creature.appearance
      .filter((field) => field.growthSuppression > 0)
      .map((field) => ({
        id: field.id,
        region: field.region,
        anchor: field.anchor,
        mask: field.mask,
        growthSuppression: field.growthSuppression,
      })),
    anchors: creature.anchors,
    maxVertices: MAX_VERTICES - bodyCount,
    defaultMaterial: doc.material,
  });
  const groom = cachedCreatureProduct("groom", groomKey, () => {
    const projector = projectionKey ? createCreatureBodyProjector(doc, body) : undefined;
    return compileCreatureGroom(
      creature,
      {
        evaluate: (chart, coordinates) => evaluateCreatureChart(creature, chart, coordinates),
        projectRoot: projector?.projectRoot,
        toLocal: (region, point) => creatureRegionLocalPoint(creature, region, point),
        toWorld: (region, point) => creatureRegionPoint(creature, region, point),
        resolveAnchor: (region, id) => {
          const anchor = creature.anchors.find((item) => item.id === id && item.region === region);
          const resolved = anchor ? resolveCreatureAnchor(creature, anchor) : undefined;
          return resolved?.status === "resolved" && resolved.position
            ? creatureRegionLocalPoint(creature, region, resolved.position)
            : undefined;
        },
      },
      { defaultMaterial: doc.material, maxVertices: Math.max(0, MAX_VERTICES - bodyCount) },
    );
  });
  // Product identity uses only declared groom inputs, independent of whether
  // this compile reused a product built before an unrelated appearance edit.
  groom.key = groomKey;
  const roots: CreatureGeometry = {
    key: "groom-roots",
    coordinates: groom.guides.map((guide) => guide.root),
    regions: groom.guides.map((guide) => guide.root.region),
    diagnostics: [],
    mesh: {
      positions: new Float32Array(groom.guides.flatMap((guide) => guide.points[0])),
      normals: new Float32Array(groom.guides.flatMap((guide) => guide.normal)),
      indices: new Uint32Array(),
      sourceIds: groom.guides.map((guide) => guide.root.id),
      bounds: { min: [0, 0, 0], max: [0, 0, 0] },
    },
  };
  const prepareDeformation = (geometry: CreatureGeometry) => {
    // Material assignment and triangle order cannot change skeletal influences.
    // Actual position/provenance fingerprints cover appearance seam duplication.
    const positionsKey = contentKey({
      positions: geometry.mesh.positions,
      regions: geometry.regions,
      sources: geometry.mesh.sourceIds,
    });
    const bindingKey = contentKey({
      version: 1,
      positionsKey,
      joints: doc.joints.map(({ name: _name, ...joint }) => joint),
      regions: creature.regions.map((region) => [region.id, region.jointIds]),
      rules: creature.influenceRules,
      attachments: creature.attachments.map((attachment) => [attachment.nodeIds, attachment.rigidJoint]),
    });
    const binding = cachedCreatureProduct("binding", bindingKey, () =>
      bindCreatureGeometry(doc.joints, creature, geometry),
    );
    const correctivesKey = contentKey({
      version: 1,
      positionsKey,
      correctives: creature.correctives,
      regions: creature.regions.map((region) => [region.id, region.frame]),
      attachments: creature.attachments,
      anchors: creature.anchors,
      chart: shapeKeys.geometry,
    });
    const correctives = cachedCreatureProduct("correctives", correctivesKey, () =>
      compileCreatureCorrectives(creature, geometry),
    );
    return { binding, correctives, bindingKey, correctivesKey };
  };
  const bodyDeformation = prepareDeformation(body),
    rootDeformation = prepareDeformation(roots);
  const assemble = (detail?: CompiledGroomDetail) =>
    cachedCreatureProduct(
      "assembly",
      contentKey({
        version: 1,
        appearanceKey,
        groomKey,
        label: detail?.label,
        binding: [bodyDeformation.bindingKey, rootDeformation.bindingKey],
        correctives: [bodyDeformation.correctivesKey, rootDeformation.correctivesKey],
      }),
      () => {
        let geometry = body;
        if (detail?.mesh.positions.length) {
          const coordinates = Array.from(detail.vertexGuideIndices, (guide) => groom.guides[guide].root);
          geometry = combineCreatureGeometry(
            body.mesh,
            {
              key: groom.key,
              mesh: detail.mesh,
              coordinates,
              regions: coordinates.map((coordinate) => coordinate.region),
              diagnostics: [],
            },
            creature,
            doc.material,
          );
          for (let i = 0; i < bodyCount; i++) {
            geometry.coordinates[i] = body.coordinates[i];
            geometry.regions[i] = body.regions[i];
          }
        }
        const vertexCount = geometry.mesh.positions.length / 3,
          jointIndices = new Uint16Array(vertexCount * 4),
          weights = new Float32Array(vertexCount * 4);
        jointIndices.set(bodyDeformation.binding.jointIndices);
        weights.set(bodyDeformation.binding.weights);
        if (detail)
          for (let i = 0; i < detail.vertexGuideIndices.length; i++) {
            const guide = detail.vertexGuideIndices[i];
            jointIndices.set(
              rootDeformation.binding.jointIndices.subarray(guide * 4, guide * 4 + 4),
              (bodyCount + i) * 4,
            );
            weights.set(
              rootDeformation.binding.weights.subarray(guide * 4, guide * 4 + 4),
              (bodyCount + i) * 4,
            );
          }
        const correctives = bodyDeformation.correctives.map((product, productIndex) => {
          const rootProduct = rootDeformation.correctives[productIndex],
            rootOffsets = new Map(Array.from(rootProduct.vertices, (guide, index) => [guide, index]));
          const vertices = Array.from(product.vertices),
            displacements = Array.from(product.displacements);
          if (detail)
            for (let i = 0; i < detail.vertexGuideIndices.length; i++) {
              const offset = rootOffsets.get(detail.vertexGuideIndices[i]);
              if (offset === undefined) continue;
              vertices.push(bodyCount + i);
              displacements.push(...rootProduct.displacements.subarray(offset * 3, offset * 3 + 3));
            }
          return {
            ...product,
            vertices: new Uint32Array(vertices),
            displacements: new Float32Array(displacements),
          };
        });
        const binding = {
          jointIndices,
          weights,
          diagnostics: [
            ...bodyDeformation.binding.diagnostics,
            ...rootDeformation.binding.diagnostics.map((diagnostic) => ({
              ...diagnostic,
              message: `Groom roots: ${diagnostic.message}`,
            })),
          ],
        };
        return { geometry, binding, correctives, groomGuideIndices: detail?.vertexGuideIndices };
      },
    );
  const assembled = groom.details.map((detail) => ({ ...assemble(detail), label: detail.label }));
  const primary = assembled[0] ?? { ...assemble(), label: "hero" };
  const anchors = cachedCreatureProduct(
    "anchors",
    contentKey({ version: 1, shape: shapeKeys.geometry, anchors: creature.anchors }),
    () => creature.anchors.map((anchor) => ({ id: anchor.id, ...resolveCreatureAnchor(creature, anchor) })),
  );
  return {
    ...base,
    mesh: primary.geometry.mesh,
    weights: primary.binding.weights,
    jointIndices: primary.binding.jointIndices,
    diagnostics: [
      ...base.diagnostics,
      ...body.diagnostics,
      ...primary.binding.diagnostics,
      ...groom.diagnostics,
      ...anchors.flatMap((a) => a.diagnostics),
    ],
    creature: structuredClone(creature),
    creatureSourceKey: contentKey(doc),
    creatureCoordinates: primary.geometry.coordinates,
    creatureRegions: primary.geometry.regions,
    creatureCorrectives: primary.correctives,
    creatureAnchors: anchors,
    creatureGroom: groom,
    creatureGroomDetail: primary.label,
    creatureBodyVertexCount: bodyCount,
    creatureMaterials: appearance.materials,
    creatureGroomMaterialSources: Object.fromEntries(
      creature.grooms
        .filter((groom) => groom.material)
        .map((groom) => [`${groom.id.slice(0, 80)}-fiber`, groom.material as string]),
    ),
    creatureDetails: assembled.slice(1).map((item, index) => ({
      label: item.label,
      mesh: item.geometry.mesh,
      jointIndices: item.binding.jointIndices,
      weights: item.binding.weights,
      correctives: item.correctives,
      groomGuideIndices: item.groomGuideIndices,
      maxProjectedDiameter: 320 * 0.4 ** index,
      maxError: null,
    })),
  };
}
