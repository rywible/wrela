import { createHash } from "node:crypto";
import { mkdir, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { compileSurfaceRelief, surfaceReliefDepth } from "@wrela/compiler";
import { contentKey, type Vec3 } from "@wrela/model";

import {
  createSurfaceReliefCpuSource,
  measurePlanarReliefReference,
  SURFACE_RELIEF_CPU_BUDGETS,
  SURFACE_RELIEF_CPU_CAP,
  SURFACE_RELIEF_CPU_DOMAIN,
  SURFACE_RELIEF_CPU_RECIPES,
  samplePlanarReliefMesh,
} from "./fixtures/surface-relief-cpu";

const revision = process.argv.find((value) => value.startsWith("--revision="))?.slice(11) ?? "multiscale-v1";
if (!/^[a-z0-9-]+$/.test(revision)) throw new Error("Use a simple revision name");
const output = resolve("output/surface-relief-cpu", revision);
await mkdir(output, { recursive: true });
const files = [
  "packages/compiler/src/surface-relief.ts",
  "packages/compiler/src/surface-relief-pattern.ts",
  "packages/compiler/src/surface-relief-sampling.ts",
  "packages/compiler/src/surface-relief-thickness.ts",
  "packages/compiler/src/assembly.ts",
  "packages/model/src/surface-relief.ts",
  "tools/fixtures/surface-relief-cpu.ts",
  "tools/surface-relief-cpu.ts",
];
const codeIdentity = await Promise.all(
  files.map(async (path) => ({
    path,
    sha256: createHash("sha256")
      .update(await readFile(path))
      .digest("hex"),
  })),
);
const { mesh, planeZ } = createSurfaceReliefCpuSource();
const sourceMeshKey = contentKey(mesh);
const reports = [];
for (const { name, recipe } of SURFACE_RELIEF_CPU_RECIPES) {
  const recipeKey = contentKey(recipe);
  for (const budget of SURFACE_RELIEF_CPU_BUDGETS) {
    const start = performance.now();
    const result = compileSurfaceRelief(mesh, recipe, {
      material: SURFACE_RELIEF_CPU_CAP,
      maxTriangles: budget.maxTriangles,
    });
    const compileMs = performance.now() - start;
    if (!result.review.applied) throw new Error(`Relief did not compile: ${name}/${budget.name}`);
    const samples = samplePlanarReliefMesh(result.mesh);
    const reference = (weights: Vec3) =>
      measurePlanarReliefReference(
        samples,
        planeZ,
        (point) => recipe.amplitude * surfaceReliefDepth(point, [0, 0, 1], recipe, weights),
        recipe.scale * 0.0001,
      );
    if (contentKey(recipe) !== recipeKey || contentKey(mesh) !== sourceMeshKey)
      throw new Error("Study mutated matched inputs");
    reports.push({
      recipe: name,
      recipeKey,
      sourceMeshKey,
      budget: budget.name,
      maxTriangles: budget.maxTriangles,
      compileMs,
      review: result.review,
      diagnostics: result.diagnostics,
      appearance: result.appearance,
      selectedGeometryReference: reference(result.review.geometryBandWeights),
      fullAuthoredReference: reference([1, 1, 1]),
    });
  }
}
for (const file of codeIdentity) {
  const current = createHash("sha256")
    .update(await readFile(file.path))
    .digest("hex");
  if (current !== file.sha256)
    throw new Error(`Study implementation changed during comparison: ${file.path}`);
}
await Bun.write(
  resolve(output, "report.json"),
  JSON.stringify(
    {
      revision,
      codeIdentity,
      sourceMeshKey,
      source: {
        triangles: mesh.indices.length / 3,
        bounds: mesh.bounds,
        planeZ,
        material: SURFACE_RELIEF_CPU_CAP,
      },
      recipes: SURFACE_RELIEF_CPU_RECIPES,
      protocol: {
        implementation: "One unchanged compiler and source mesh for all three recipes and all three budgets",
        domain: SURFACE_RELIEF_CPU_DOMAIN,
        grid: [31, 31],
        samplePlacement:
          "Uniform cell centers in the interior of the same positive-Z planar cap; normal and depth samples are barycentrically interpolated from actual compiled triangles",
        reference:
          "Centered finite differences of the procedural inward-depth field in metres; half-step convergence reported independently",
        selectedGeometryReference: "Error against the spectrum explicitly allocated to physical geometry",
        fullAuthoredReference:
          "Error against all authored bands, including bands deferred to appearance; prevents spectral suppression from masquerading as full geometric convergence",
        limitations:
          "Interior static stone-cap samples only. Neither whole-mesh error bounds nor rendered residual normals, radiance, silhouette, animation, GPU performance, or art acceptance are measured. Compile times are single CPU observations, not a benchmark.",
      },
      reports,
    },
    null,
    2,
  ),
);
console.log(
  JSON.stringify({
    output,
    reports: reports.map((report) => ({
      recipe: report.recipe,
      budget: report.budget,
      triangles: report.review.triangles,
      weights: report.review.geometryBandWeights,
      filteredRmsDegrees: report.selectedGeometryReference.interpolatedNormalDegrees.rms,
      fullRmsDegrees: report.fullAuthoredReference.interpolatedNormalDegrees.rms,
      maxGradientHalfStepDegrees: report.fullAuthoredReference.gradientHalfStepDifferenceDegrees.maximum,
    })),
  }),
);
