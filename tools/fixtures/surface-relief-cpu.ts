import { compileAssemblyMesh } from "@wrela/compiler";
import { assemblySchema, type MeshData, normalize, type SurfaceRelief, type Vec3 } from "@wrela/model";

export const SURFACE_RELIEF_CPU_BUDGETS = [
  { name: "low", maxTriangles: 2_000 },
  { name: "review", maxTriangles: 12_000 },
  { name: "high", maxTriangles: 60_000 },
] as const;
export const SURFACE_RELIEF_CPU_RECIPES: { name: string; recipe: SurfaceRelief }[] = [
  {
    name: "fine-grain",
    recipe: {
      kind: "stone",
      amplitude: 0.012,
      scale: 0.075,
      seed: 19,
      targetEdgeLength: 0.008,
      direction: [0, 1, 0],
    },
  },
  {
    name: "weathered",
    recipe: {
      kind: "stone",
      amplitude: 0.014,
      scale: 0.1,
      seed: 41,
      targetEdgeLength: 0.008,
      direction: [0, 1, 0],
    },
  },
  {
    name: "broad-chip",
    recipe: {
      kind: "stone",
      amplitude: 0.018,
      scale: 0.16,
      seed: 73,
      targetEdgeLength: 0.008,
      direction: [0, 1, 0],
    },
  },
];
export const SURFACE_RELIEF_CPU_CAP = "measured-stone-cap";
export const SURFACE_RELIEF_CPU_DOMAIN = { min: [-0.2, -0.25], max: [0.2, 0.25] } as const;

/** The same closed angular extrusion and original cap fan are used for every run. */
export function createSurfaceReliefCpuSource(): { mesh: MeshData; planeZ: number } {
  const { mesh } = compileAssemblyMesh(
    assemblySchema.parse({
      grid: 0.01,
      clearances: [],
      parts: [
        {
          id: "planar-stone",
          name: "Planar stone cap",
          profile: {
            kind: "polygon",
            points: [
              [-0.44, -0.36],
              [-0.31, -0.52],
              [0.21, -0.49],
              [0.46, -0.29],
              [0.42, 0.32],
              [0.17, 0.56],
              [-0.27, 0.51],
              [-0.49, 0.23],
            ],
          },
          path: [
            [0, 0, -0.3],
            [0, 0, 0.3],
          ],
          bevel: 0,
          position: [0, 0, 0],
          rotation: [0, 0, 0],
          repeat: { count: 1, offset: [0, 0, 0] },
          sockets: [],
          wear: { amount: 0, scale: 1, seed: 0 },
        },
      ],
    }),
    "support",
  );
  const planeZ = Math.max(...mesh.positions.filter((_value, index) => index % 3 === 2));
  mesh.materialGroups = [];
  for (let start = 0; start < mesh.indices.length; start += 3) {
    const cap = [0, 1, 2].every(
      (corner) => Math.abs(mesh.positions[mesh.indices[start + corner] * 3 + 2] - planeZ) < 1e-7,
    );
    const material = cap ? SURFACE_RELIEF_CPU_CAP : "support";
    const previous = mesh.materialGroups.at(-1);
    if (previous?.material === material) previous.count += 3;
    else mesh.materialGroups.push({ material, start, count: 3 });
  }
  return { mesh, planeZ };
}

export type PlanarSample = { point: Vec3; normal: Vec3; faceNormal: Vec3 };
export type ErrorSummary = { rms: number; mean: number; p95: number; maximum: number };
export function summarizeErrors(values: number[]): ErrorSummary {
  if (!values.length || values.some((value) => !Number.isFinite(value)))
    throw new Error("Missing or invalid relief samples");
  const sorted = [...values].sort((a, b) => a - b);
  return {
    rms: Math.sqrt(values.reduce((sum, value) => sum + value * value, 0) / values.length),
    mean: values.reduce((sum, value) => sum + value, 0) / values.length,
    p95: sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))],
    maximum: sorted[sorted.length - 1],
  };
}
const angle = (a: Vec3, b: Vec3) =>
  (Math.acos(Math.max(-1, Math.min(1, a[0] * b[0] + a[1] * b[1] + a[2] * b[2]))) * 180) / Math.PI;

/** Uniform sample sites are fixed across budgets; count is independent of generated mesh density. */
export function samplePlanarReliefMesh(mesh: MeshData, gridSize = 31): PlanarSample[] {
  if (!Number.isInteger(gridSize) || gridSize < 3 || gridSize > 101)
    throw new Error("Use a 3–101 point relief sample grid");
  const { min, max } = SURFACE_RELIEF_CPU_DOMAIN;
  const step = [(max[0] - min[0]) / gridSize, (max[1] - min[1]) / gridSize];
  const samples = new Array<PlanarSample | undefined>(gridSize * gridSize);
  const coordinate = (axis: number, index: number) => min[axis] + (index + 0.5) * step[axis];
  for (const group of mesh.materialGroups ?? []) {
    if (group.material !== SURFACE_RELIEF_CPU_CAP) continue;
    for (let start = group.start; start < group.start + group.count; start += 3) {
      const indices = [0, 1, 2].map((corner) => mesh.indices[start + corner]);
      const points = indices.map((index) => [...mesh.positions.subarray(index * 3, index * 3 + 3)] as Vec3);
      const [a, b, c] = points;
      const determinant = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
      if (Math.abs(determinant) < 1e-15) continue;
      const first = [0, 1].map((axis) =>
        Math.max(
          0,
          Math.ceil((Math.min(...points.map((p) => p[axis])) - min[axis]) / step[axis] - 0.5 - 1e-8),
        ),
      );
      const last = [0, 1].map((axis) =>
        Math.min(
          gridSize - 1,
          Math.floor((Math.max(...points.map((p) => p[axis])) - min[axis]) / step[axis] - 0.5 + 1e-8),
        ),
      );
      const ab = b.map((value, axis) => value - a[axis]),
        ac = c.map((value, axis) => value - a[axis]);
      const faceNormal = normalize([
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
      ]);
      for (let y = first[1]; y <= last[1]; y++)
        for (let x = first[0]; x <= last[0]; x++) {
          const sampleIndex = y * gridSize + x;
          if (samples[sampleIndex]) continue;
          const px = coordinate(0, x),
            py = coordinate(1, y);
          const wa = ((b[1] - c[1]) * (px - c[0]) + (c[0] - b[0]) * (py - c[1])) / determinant;
          const wb = ((c[1] - a[1]) * (px - c[0]) + (a[0] - c[0]) * (py - c[1])) / determinant;
          const weights = [wa, wb, 1 - wa - wb];
          if (weights.some((weight) => weight < -1e-7)) continue;
          const normal = normalize(
            [0, 1, 2].map((axis) =>
              indices.reduce(
                (sum, index, corner) => sum + mesh.normals[index * 3 + axis] * weights[corner],
                0,
              ),
            ) as Vec3,
          );
          samples[sampleIndex] = {
            point: [px, py, points.reduce((sum, point, corner) => sum + point[2] * weights[corner], 0)],
            normal,
            faceNormal,
          };
        }
    }
  }
  if (
    samples.some((sample) => sample === undefined) ||
    samples.filter(Boolean).length !== gridSize * gridSize
  )
    throw new Error("The relief sample domain is not fully covered by the tagged cap");
  return samples as PlanarSample[];
}

/** Reference depths are metres inward from the original positive-Z plane. */
export function measurePlanarReliefReference(
  samples: PlanarSample[],
  planeZ: number,
  depthAt: (point: Vec3) => number,
  differenceStep: number,
) {
  if (!(differenceStep > 0) || !Number.isFinite(differenceStep))
    throw new Error("Invalid relief gradient step");
  const vertexNormalErrors: number[] = [],
    faceNormalErrors: number[] = [],
    depthErrors: number[] = [],
    referenceConvergence: number[] = [];
  const referenceNormal = (point: Vec3, h: number): Vec3 =>
    normalize([
      (depthAt([point[0] + h, point[1], planeZ]) - depthAt([point[0] - h, point[1], planeZ])) / (2 * h),
      (depthAt([point[0], point[1] + h, planeZ]) - depthAt([point[0], point[1] - h, planeZ])) / (2 * h),
      1,
    ]);
  for (const sample of samples) {
    const reference = referenceNormal(sample.point, differenceStep);
    vertexNormalErrors.push(angle(sample.normal, reference));
    faceNormalErrors.push(angle(sample.faceNormal, reference));
    depthErrors.push(
      Math.abs(planeZ - sample.point[2] - depthAt([sample.point[0], sample.point[1], planeZ])),
    );
    referenceConvergence.push(angle(reference, referenceNormal(sample.point, differenceStep / 2)));
  }
  return {
    samples: samples.length,
    interpolatedNormalDegrees: summarizeErrors(vertexNormalErrors),
    triangleNormalDegrees: summarizeErrors(faceNormalErrors),
    depthMetres: summarizeErrors(depthErrors),
    gradientStepMetres: differenceStep,
    gradientHalfStepDifferenceDegrees: summarizeErrors(referenceConvergence),
  };
}
