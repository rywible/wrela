import {
  add,
  type CreatureDefinition,
  type CreatureGroom,
  contentKey,
  cross,
  dot,
  normalize,
  scale,
  sub,
  type Vec3,
} from "@wrela/model";

import {
  type CompiledGroom,
  compileGroom,
  type GroomChartSample,
  type GroomLayerSource,
  GroomProjectionRejected,
} from "./groom";
import { creatureAppearanceCoverage } from "./groom-appearance";

export type GroomRootProjection = {
  status: "resolved" | "ambiguous" | "no-hit" | "wrong-region";
  position?: Vec3;
  normal?: Vec3;
  distance?: number;
  sourceNode?: string;
  interval?: [number, number];
  reason?: string;
};
export type CreatureGroomContext = {
  projectRoot?: (groom: CreatureGroom, sample: GroomChartSample) => GroomRootProjection;
  evaluate: (chart: string, coordinates: Vec3) => GroomChartSample;
  toLocal: (region: string, point: Vec3) => Vec3;
  toWorld: (region: string, point: Vec3) => Vec3;
  resolveAnchor?: (region: string, id: string) => Vec3 | undefined;
};

/** Integrates physical-density source, region-local bald masks, authored guide curves and
 * stable surface charts. Density controls expected roots per square metre, maxCards bounds work. */
export function compileCreatureGroom(
  creature: CreatureDefinition,
  context: CreatureGroomContext,
  options: { maxGuides?: number; maxVertices?: number; defaultMaterial?: string } = {},
): CompiledGroom {
  const layers: GroomLayerSource[] = [];
  const anchorOrigins = new Map(
    creature.appearance
      .filter((source) => source.anchor)
      .map((source) => [source.id, context.resolveAnchor?.(source.region, source.anchor ?? "")]),
  );
  const errors: CompiledGroom["diagnostics"] = [];
  const projectionReports: (() => void)[] = [];
  for (const groom of creature.grooms) {
    const charts = creature.charts.filter(
      (chart) => chart.region === groom.region && (!groom.chart || chart.id === groom.chart),
    );
    if (charts.length !== 1) {
      errors.push({
        severity: "error",
        code: "groom-chart-ambiguous",
        node: groom.id,
        message: `Groom requires exactly one compatible chart; found ${charts.length}. Select an explicit chart.`,
      });
      continue;
    }
    const chart = charts[0],
      radial = chart.kind === "patch" ? 0 : 1;
    if (groom.chart && groom.chartRevision !== chart.revision) {
      errors.push({
        severity: "error",
        code: "groom-invalid-anchor",
        node: groom.id,
        message: "Explicit groom charts require a matching authored chart revision",
      });
      continue;
    }
    let area = 0;
    try {
      // Geometry-independent chart sampling keeps the density estimate stable across tessellation.
      for (let u = 0; u < 8; u++)
        for (let v = 0; v < 16; v++) {
          const a = context.evaluate(chart.id, [u / 8, v / 16, radial]).position,
            b = context.evaluate(chart.id, [(u + 1) / 8, v / 16, radial]).position,
            c = context.evaluate(chart.id, [u / 8, (v + 1) / 16, radial]).position,
            d = context.evaluate(chart.id, [(u + 1) / 8, (v + 1) / 16, radial]).position;
          area +=
            0.5 * (Math.hypot(...cross(sub(b, a), sub(c, a))) + Math.hypot(...cross(sub(c, d), sub(b, d))));
        }
    } catch {
      errors.push({
        severity: "error",
        code: "groom-invalid-anchor",
        node: groom.id,
        message: "Cannot evaluate groom growth chart",
      });
      continue;
    }
    if (!Number.isFinite(area) || area <= 1e-12) {
      errors.push({
        severity: "error",
        code: "groom-degenerate-area",
        node: groom.id,
        message: "Growth chart has no finite surface area",
      });
      continue;
    }
    const requested = Math.ceil(area * groom.density),
      count = Math.min(groom.maxCards, requested);
    if (requested > groom.maxCards)
      errors.push({
        severity: "warning",
        code: "groom-density-budget",
        node: groom.id,
        message: `${requested} roots requested at ${groom.density}/m²; capped to ${groom.maxCards}`,
      });
    const origin = context.toWorld(groom.region, [0, 0, 0]);
    // Authored directions are region-local, whereas low-level guides use a chart tangent frame.
    const directionWorld = normalize(sub(context.toWorld(groom.region, groom.direction), origin));
    const middle = context.evaluate(chart.id, [0.5, 0.5, radial]),
      tangent = normalize(middle.tangent),
      bitangent = normalize(cross(middle.normal, tangent));
    const material = `${groom.id.slice(0, 80)}-fiber`;
    const growthCache = new Map<string, GroomChartSample | Error>();
    const projectionFailures = new Map<string, { count: number; reason: string }>();
    const evaluateGrowth = (coordinates: Vec3): GroomChartSample => {
      const key = coordinates.join(",");
      const cached = growthCache.get(key);
      if (cached instanceof Error) throw cached;
      if (cached) return cached;
      const sample = context.evaluate(chart.id, coordinates);
      if (!groom.rootProjection) {
        growthCache.set(key, sample);
        return sample;
      }
      const result = context.projectRoot?.(groom, sample) ?? {
        status: "no-hit" as const,
        reason: "No compiled-body projection provider installed",
      };
      if (
        result.status !== "resolved" ||
        !result.position ||
        !result.normal ||
        !result.position.every(Number.isFinite) ||
        !result.normal.every(Number.isFinite) ||
        Math.hypot(...result.normal) < 1e-8 ||
        Math.hypot(...sub(result.position, sample.position)) > groom.rootProjection.maxDistance + 1e-6 ||
        !Number.isFinite(result.distance) ||
        Math.abs(result.distance ?? 0) > groom.rootProjection.maxDistance + 1e-6 ||
        (groom.rootProjection.direction === "outward" && (result.distance ?? 0) < -1e-6)
      ) {
        const status = result.status === "resolved" ? "invalid-result" : result.status;
        const failure = projectionFailures.get(status) ?? {
          count: 0,
          reason: result.reason ?? "Projection did not resolve a compatible bounded body surface",
        };
        failure.count++;
        projectionFailures.set(status, failure);
        const error = new GroomProjectionRejected(failure.reason);
        growthCache.set(key, error);
        throw error;
      }
      const normal = normalize(result.normal);
      const projectedTangent = sub(sample.tangent, scale(normal, dot(sample.tangent, normal)));
      const tangent =
        Math.hypot(...projectedTangent) > 1e-8
          ? normalize(projectedTangent)
          : normalize(cross(normal, Math.abs(normal[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0]));
      const projected = {
        ...sample,
        position: result.position,
        normal,
        tangent,
        projection: {
          domain: "compiled-body" as const,
          chartPosition: sample.position,
          sourceNode: result.sourceNode,
          distance: result.distance ?? 0,
          interval: result.interval,
        },
      };
      growthCache.set(key, projected);
      return projected;
    };

    projectionReports.push(() => {
      for (const [status, failure] of projectionFailures)
        errors.push({
          severity: !context.projectRoot || status === "invalid-result" ? "error" : "warning",
          code: `groom-projection-${status}`,
          node: groom.id,
          message: `${failure.count} chart roots omitted: ${failure.reason}`,
        });
    });
    layers.push({
      id: groom.id,
      evaluate: evaluateGrowth,
      representation: groom.representation,
      ribbonThickness: groom.ribbonThickness,
      region: groom.region,
      chart: chart.id,
      chartRevision: groom.chartRevision ?? chart.revision,
      radialCoordinate: radial,
      material,
      seed: groom.seed,
      guideCount: count,
      density: 1,
      length: groom.length,
      width: groom.width,
      taper: groom.taper,
      lift: groom.lift,
      direction: directionWorld,
      flow: [
        dot(directionWorld, tangent),
        dot(directionWorld, bitangent),
        dot(directionWorld, normalize(middle.normal)),
      ],
      clump: groom.clump,
      curl: groom.curl,
      frizz: groom.frizz,
      rootColor: groom.rootColor,
      tipColor: groom.tipColor,
      stiffness: groom.stiffness,
      damping: groom.damping,
      lodFractions: groom.lodFractions,
      coverage: (coordinates) => {
        let point: Vec3;
        try {
          point = evaluateGrowth(coordinates).position;
        } catch {
          return 1;
        } // Geometry evaluation records/rejects this root explicitly.
        const local = context.toLocal(groom.region, point);
        let coverage = 1;
        for (const mask of groom.masks) {
          const t = Math.min(1, Math.hypot(...sub(local, mask.center)) / mask.radius);
          const falloff = 1 - t * t * (3 - 2 * t);
          coverage *= 1 - mask.strength * falloff;
        }
        for (const appearance of creature.appearance) {
          if (appearance.region === groom.region)
            coverage *=
              1 -
              appearance.growthSuppression *
                creatureAppearanceCoverage(appearance, local, anchorOrigins.get(appearance.id));
        }
        return Math.max(0, Math.min(1, coverage));
      },
      ...(groom.guides.length
        ? {
            shape: (sample: GroomChartSample, t: number): Vec3 => {
              const local = context.toLocal(groom.region, sample.position);
              const guide = groom.guides.reduce((best, candidate) =>
                Math.hypot(...sub(local, candidate.points[0])) < Math.hypot(...sub(local, best.points[0]))
                  ? candidate
                  : best,
              );
              const segmentLengths = guide.points
                  .slice(1)
                  .map((p, index) => Math.hypot(...sub(p, guide.points[index]))),
                length = segmentLengths.reduce((a, b) => a + b, 0);
              if (length < 1e-8) throw new Error(`Authored guide ${guide.id} has zero length`);
              let distance = t * length,
                segment = 0;
              while (segment < segmentLengths.length - 1 && distance > segmentLengths[segment]) {
                distance -= segmentLengths[segment];
                segment++;
              }
              const f = Math.min(1, distance / Math.max(1e-12, segmentLengths[segment]));
              const p = add(scale(guide.points[segment], 1 - f), scale(guide.points[segment + 1], f));
              const delta = scale(sub(p, guide.points[0]), groom.length / length);
              return add(sample.position, sub(context.toWorld(groom.region, delta), origin));
            },
          }
        : {}),
    });
  }
  const result = compileGroom(layers, context.evaluate, {
    ...options,
    sourceKey: contentKey({
      grooms: creature.grooms,
      appearance: creature.appearance,
      anchors: creature.anchors,
      charts: creature.charts,
      regions: creature.regions,
      sculpts: creature.sculpts,
    }),
  });
  for (const report of projectionReports) report();
  for (const groom of creature.grooms) {
    const inward = result.guides.filter(
      (guide) =>
        guide.root.layer === groom.id && dot(sub(guide.points[1], guide.points[0]), guide.normal) < -1e-7,
    );
    if (inward.length)
      errors.push({
        severity: "warning",
        code: "groom-inward-guides",
        node: groom.id,
        message: `${inward.length} authored guides initially enter their growth surface; revise guide flow or the growth region`,
      });
  }
  result.diagnostics.unshift(...errors);
  return result;
}
