import { mkdir } from "node:fs/promises";
import { compileField, extractSurface } from "@wrela/compiler";
import { referenceProject, shape } from "@wrela/examples";
import type { Bounds, FieldDefinition, FieldNode, Vec3 } from "@wrela/model";
import { compileGauge, gaugeValue } from "./gauge";
import { buildAtlas, correct, jet, lower, quadricHit, specialize, value } from "./local-program";

const median = (v: number[]) => [...v].sort((a, b) => a - b)[Math.floor(v.length / 2)];
function distribution(v: number[]) {
  const sorted = [...v].sort((a, b) => a - b);
  return {
    count: v.length,
    mean: v.reduce((a, b) => a + b, 0) / v.length,
    median: median(v),
    p95: sorted[Math.floor(v.length * 0.95)],
    max: sorted.at(-1),
  };
}
function fixtures() {
  const docs = referenceProject().documents;
  const bunny = docs.find((d) => d.kind === "character");
  const stone = docs.find((d) => d.kind === "object");
  if (bunny?.kind !== "character" || stone?.kind !== "object") throw new Error("Missing fixtures");
  const nodes: FieldNode[] = [];
  for (let x = 0; x < 4; x++)
    for (let y = 0; y < 4; y++)
      for (let z = 0; z < 4; z++)
        nodes.push(shape(`part-${nodes.length}`, "part", [x - 1.5, y - 1.5, z - 1.5], [0.36, 0.38, 0.3]));
  const cluster: FieldDefinition = {
    root: "root",
    nodes: [
      {
        ...shape("root", "root", [0, 0, 0], [1, 1, 1], "smoothUnion"),
        children: nodes.map((n) => n.id),
        blend: 0.08,
      },
      ...nodes,
    ],
    bounds: { min: [-2, -2, -2], max: [2, 2, 2] },
    resolution: 48,
  };
  return [
    { name: "reference-bunny", field: bunny.field },
    { name: "reference-stone", field: stone.field },
    {
      name: "synthetic-blended-neck",
      field: {
        root: "root",
        nodes: [
          { ...shape("root", "root", [0, 0, 0], [1, 1, 1], "smoothUnion"), children: ["a", "b"], blend: 0.3 },
          shape("a", "a", [-0.42, 0, 0], [0.7, 0.6, 0.58]),
          { ...shape("b", "b", [0.42, 0, 0], [0.7, 0.65, 0.52]), rotation: [0.1, 0.2, 0.15] as Vec3 },
        ],
        bounds: { min: [-1.3, -0.85, -0.8], max: [1.3, 0.85, 0.8] },
        resolution: 24,
      } as FieldDefinition,
    },
    { name: "synthetic-64-separated-ellipsoids", field: cluster },
  ];
}
let checksum = 0;
function queryBench(field: FieldDefinition) {
  const mesh = extractSurface(field, "interactive").mesh;
  const points: Vec3[] = [];
  for (let i = 0; i < mesh.positions.length; i += 3)
    points.push([mesh.positions[i], mesh.positions[i + 1], mesh.positions[i + 2]]);
  const original = compileField(field),
    lowered = lower(field);
  const start = performance.now(),
    atlas = buildAtlas(field, 5),
    buildMs = performance.now() - start;
  const repeats = Math.max(1, Math.ceil(40000 / points.length));
  const run = (mode: number) => {
    const start = performance.now();
    let sum = 0;
    for (let r = 0; r < repeats; r++)
      for (const p of points)
        sum += mode === 0 ? original.distance(p) : value(mode === 1 ? lowered : atlas.at(p).expression, p);
    checksum += sum;
    return performance.now() - start;
  };
  for (let i = 0; i < 3; i++) {
    run(0);
    run(1);
    run(2);
  }
  const times = [[], [], []] as number[][];
  for (let r = 0; r < 9; r++) for (const mode of r % 2 ? [2, 1, 0] : [0, 1, 2]) times[mode].push(run(mode));
  let maxValueError = 0,
    primitiveSum = 0,
    canonicalSamples = 0;
  for (const p of points) {
    const local = atlas.at(p);
    maxValueError = Math.max(maxValueError, Math.abs(original.distance(p) - value(local.expression, p)));
    primitiveSum += local.primitives;
    if (local.canonical) canonicalSamples++;
  }
  return {
    buildMs,
    samplesPerRepeat: points.length * repeats,
    originalMs: distribution(times[0]),
    loweredOnlyMs: distribution(times[1]),
    regionalMs: distribution(times[2]),
    speedupAgainstOriginal: median(times[0]) / median(times[2]),
    speedupAgainstLowered: median(times[1]) / median(times[2]),
    maxValueError,
    meanActivePrimitivesAtMeshVertices: primitiveSum / points.length,
    canonicalVertexFraction: canonicalSamples / points.length,
    ...atlas.stats,
  };
}
function qualityBench(field: FieldDefinition) {
  const f = { ...field, resolution: Math.min(24, field.resolution) },
    mesh = extractSurface(f).mesh;
  const expression = lower(field),
    original = compileField(field);
  const raw: number[] = [],
    reconstructed: number[] = [],
    polynomial: number[] = [],
    gauge: number[] = [],
    eligibleRaw: number[] = [];
  const blendRaw: number[] = [],
    blendPolynomial: number[] = [],
    blendGauge: number[] = [];
  let candidates = 0,
    eligible = 0,
    exact = 0,
    rejected = 0;
  const triangles = mesh.indices.length / 3,
    stride = Math.max(1, Math.floor(triangles / 2500));
  for (let tri = 0; tri < triangles; tri += stride) {
    const vertices = [0, 1, 2].map((j) => {
      const i = mesh.indices[tri * 3 + j] * 3;
      return [mesh.positions[i], mesh.positions[i + 1], mesh.positions[i + 2]] as Vec3;
    });
    const center = [0, 1, 2].map((j) => (vertices[0][j] + vertices[1][j] + vertices[2][j]) / 3) as Vec3;
    const diameter = Math.max(...vertices.map((p) => Math.hypot(...p.map((v, i) => v - center[i]))));
    const pad = diameter * 0.35 + 1e-5;
    const bounds: Bounds = {
      min: [0, 1, 2].map((i) => Math.min(...vertices.map((v) => v[i])) - pad) as Vec3,
      max: [0, 1, 2].map((i) => Math.max(...vertices.map((v) => v[i])) + pad) as Vec3,
    };
    const local = specialize(expression, bounds);
    const contains = (p: Vec3) => p.every((v, i) => v >= bounds.min[i] && v <= bounds.max[i]);
    for (const weights of [
      [1 / 3, 1 / 3, 1 / 3],
      [0.1, 0.3, 0.6],
      [0.6, 0.3, 0.1],
    ]) {
      candidates++;
      const p = [0, 1, 2].map((i) => vertices.reduce((sum, v, j) => sum + v[i] * weights[j], 0)) as Vec3;
      const before = Math.abs(original.distance(p));
      raw.push(before);
      if (!local.smooth) {
        reconstructed.push(before);
        continue;
      }
      try {
        const j = jet(local.expression, center),
          length = Math.hypot(...j.g),
          n = j.g.map((v) => v / length) as Vec3;
        const pQuad = correct(j, center, p, n);
        if (!pQuad || !contains(pQuad)) {
          rejected++;
          reconstructed.push(before);
          continue;
        }
        const gaugeProgram = compileGauge(local.expression, bounds, 2);
        let t = 0;
        const at = (x: number) => p.map((v, i) => v + x * n[i]) as Vec3;
        const e = 1e-5;
        for (let step = 0; step < 3; step++) {
          const d = gaugeValue(gaugeProgram, at(t)),
            derivative =
              (gaugeValue(gaugeProgram, at(t + e)) - gaugeValue(gaugeProgram, at(t - e))) / (2 * e);
          if (Math.abs(derivative) < 1e-8) throw new Error("Grazing correction");
          t -= d / derivative;
        }
        const pGauge = at(t);
        if (!contains(pGauge)) throw new Error("Correction left certified region");
        let final = pGauge;
        if (local.canonical) {
          const hits = quadricHit(local, p, n).sort((a, b) => Math.abs(a) - Math.abs(b));
          if (!hits.length) throw new Error("No quadric hit");
          final = at(hits[0]);
          if (!contains(final)) throw new Error("Quadric root outside region");
          exact++;
        }
        if (!local.canonical) {
          blendRaw.push(before);
          blendPolynomial.push(Math.abs(original.distance(pQuad)));
          blendGauge.push(Math.abs(original.distance(pGauge)));
        }
        eligible++;
        eligibleRaw.push(before);
        polynomial.push(Math.abs(original.distance(pQuad)));
        gauge.push(Math.abs(original.distance(pGauge)));
        reconstructed.push(Math.abs(original.distance(final)));
      } catch {
        rejected++;
        reconstructed.push(before);
      }
    }
  }
  const before = distribution(raw),
    after = distribution(reconstructed),
    acceptedBefore = distribution(eligibleRaw),
    jets = distribution(polynomial),
    gauges = distribution(gauge);
  return {
    triangles,
    candidates,
    eligible,
    exact,
    rejected,
    eligibleFraction: eligible / candidates,
    metric:
      "absolute authored field residual at triangle-interior samples, not pixel error or silhouette quality",
    allSamplesBefore: before,
    allSamplesHybrid: after,
    allSamplesMeanResidualReduction: before.mean / after.mean,
    eligibleBefore: acceptedBefore,
    eligibleQuadratic: jets,
    eligibleGauge: gauges,
    eligibleQuadraticMeanResidualReduction: acceptedBefore.mean / jets.mean,
    eligibleGaugeMeanResidualReduction: acceptedBefore.mean / gauges.mean,
    blendOnly: blendRaw.length
      ? {
          before: distribution(blendRaw),
          polynomial: distribution(blendPolynomial),
          gauge: distribution(blendGauge),
          gaugeVsPolynomial: distribution(blendPolynomial).mean / distribution(blendGauge).mean,
        }
      : null,
  };
}

const results = fixtures().map(({ name, field }) => {
  const query = queryBench(field),
    quality = qualityBench(field);
  const result = { name, primitives: field.nodes.filter((n) => !n.children.length).length, query, quality };
  console.log(
    JSON.stringify({
      name,
      fieldQuerySpeedup: query.speedupAgainstOriginal,
      algorithmOnlySpeedup: query.speedupAgainstLowered,
      activePrimitives: query.meanActivePrimitivesAtMeshVertices,
      exactRegionFraction: query.canonicalVertexFraction,
      qualityEligible: quality.eligibleFraction,
      eligibleResidualReduction: quality.eligibleGaugeMeanResidualReduction,
      wholeMeshResidualReduction: quality.allSamplesMeanResidualReduction,
    }),
  );
  return result;
});
await mkdir("output/field-research", { recursive: true });
await Bun.write(
  "output/field-research/cpu.json",
  JSON.stringify(
    {
      created: new Date().toISOString(),
      runtime: Bun.version,
      platform: process.platform,
      architecture: process.arch,
      methodology:
        "9 alternating repetitions after 3 warmups; original production evaluator, flattened control, and reduced atlas include query traversal; atlas construction reported separately. Quality uses actual production marching-tetrahedra meshes at resolution 24, with all fallbacks included.",
      checksum,
      results,
    },
    null,
    2,
  ),
);
