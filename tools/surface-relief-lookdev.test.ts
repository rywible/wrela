import { expect, test } from "bun:test";
import { compileDocument } from "@wrela/compiler";
import {
  createSurfaceReliefLookdevProject,
  SURFACE_RELIEF_CAMERAS,
} from "@wrela/examples/surface-relief-lookdev";
import { contentKey, parseProject } from "@wrela/model";
import { projectedGeometryError, selectRenderProducts } from "@wrela/render-webgpu/realization";
import { BrowserSceneHost } from "@wrela/runtime";

test("physical-relief lookdev compares identical authored shapes and flat colors", () => {
  const smooth = parseProject(createSurfaceReliefLookdevProject(false));
  const relief = parseProject(createSurfaceReliefLookdevProject(true));
  const before = smooth.documents.filter((document) => document.kind === "object");
  const after = relief.documents.filter((document) => document.kind === "object");
  expect(contentKey(before)).toBe(contentKey(after));
  for (const material of relief.documents) {
    if (material.kind !== "material") continue;
    const original = smooth.documents.find((document) => document.id === material.id);
    if (!original || original.kind !== "material" || !material.appearance)
      throw new Error("Missing A/B material");
    expect(material.appearance.relief?.amplitude).toBeGreaterThan(0);
    expect(original.appearance?.relief).toBeUndefined();
    expect(material.pattern).toBe("solid");
    expect(material.normalStrength).toBe(0);
    const { relief: _relief, ...appearance } = material.appearance;
    expect(contentKey({ ...material, appearance })).toBe(contentKey(original));
  }
});

test("review specimens compile to a true cylinder and clipped angular stone with separate material groups", () => {
  const project = parseProject(createSurfaceReliefLookdevProject());
  const specimen = project.documents.find((document) => document.id === project.entry);
  if (!specimen || specimen.kind !== "object") throw new Error("Missing specimens");
  expect(specimen.assembly?.parts[0].profile.kind).toBe("circle");
  expect(specimen.assembly?.parts[1].profile.kind).toBe("polygon");
  const artifact = compileDocument(specimen, "review");
  if (!artifact || artifact.kind !== "surface") throw new Error("Missing compiled specimen");
  expect(artifact.mesh.materialGroups?.map((group) => group.material)).toEqual([
    "surface-relief-bark",
    "surface-relief-stone",
  ]);
  expect(artifact.mesh.indices.length / 3).toBeLessThan(1000);
  expect(artifact.mesh.bounds.max[1]).toBeCloseTo(1.65);
  expect(artifact.diagnostics.filter((diagnostic) => diagnostic.severity === "error")).toEqual([]);
});

test("real host supplies near relief and bounded coarse candidates selected at matched study distances", async () => {
  const project = parseProject(createSurfaceReliefLookdevProject(true));
  const host = new BrowserSceneHost(project);
  try {
    host.setViewportHeight(640);
    await host.prepare(project.entry, "surface-relief-stage", "review");
    const near = host.extract(SURFACE_RELIEF_CAMERAS.near, "clay");
    expect(host.surfaceReliefReviews).toHaveLength(2);
    for (const review of host.surfaceReliefReviews) {
      expect(review.applied).toBe(true);
      expect(review.maxDisplacement).toBeGreaterThan(0);
      expect(review.triangles).toBeGreaterThan(review.sourceTriangles);
      expect(review.triangles).toBeLessThanOrEqual(12000);
    }
    expect(selectRenderProducts(near, 640).decisions.map((decision) => decision.kind)).toEqual([
      "direct-mesh",
      "direct-mesh",
    ]);
    const far = host.extract(SURFACE_RELIEF_CAMERAS.far, "clay");
    expect(selectRenderProducts(far, 640).decisions.map((decision) => decision.kind)).toEqual([
      "parametric-mesh",
      "parametric-mesh",
    ]);
    for (const surface of far.surfaces) {
      const coarse = surface.renderProducts?.find((product) => product.kind === "parametric-mesh");
      const evidence = coarse?.errors.find(
        (error) => error.kind === "real-bound" && error.metric === "silhouette",
      );
      if (!evidence || evidence.kind !== "real-bound") throw new Error("Missing coarse geometric bound");
      expect(projectedGeometryError(far, surface, evidence.maximum, 640)).toBeLessThanOrEqual(0.25);
    }
    const forced = {
      ...far,
      surfaces: far.surfaces.map((surface) => ({
        ...surface,
        renderProducts: surface.renderProducts?.filter((product) => product.kind === "direct-mesh"),
      })),
    };
    expect(selectRenderProducts(forced, 640).decisions.map((decision) => decision.kind)).toEqual([
      "direct-mesh",
      "direct-mesh",
    ]);
  } finally {
    host.dispose();
  }
});
