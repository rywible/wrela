import { expect, test } from "bun:test";
import {
  createSurfaceReliefLookdevProject,
  SURFACE_RELIEF_CAMERAS,
} from "@wrela/examples/surface-relief-lookdev";
import type { RenderCostObservation } from "@wrela/model";
import { RENDER_KERNEL_VERSION, selectRenderProducts } from "@wrela/render-webgpu/realization";
import { BrowserSceneHost } from "@wrela/runtime";

test("host installs cached physical near meshes and renderer selects original coarse only inside projected geometry budget", async () => {
  const project = createSurfaceReliefLookdevProject();
  const host = new BrowserSceneHost(project);
  try {
    await host.prepare(project.entry, "surface-relief-stage", "review");
    const near = host.extract(SURFACE_RELIEF_CAMERAS.near);
    expect(host.surfaceReliefReviews.length).toBe(2);
    expect(
      host.surfaceReliefReviews.every((review) => review.applied && review.realization === "static-products"),
    ).toBe(true);
    expect(host.resourceUsage.reliefBytes).toBeGreaterThan(0);
    const firstMeshes = near.surfaces.map((surface) => surface.mesh);
    expect(
      selectRenderProducts(near, 720).decisions.every((decision) => decision.kind === "direct-mesh"),
    ).toBe(true);
    const repeated = host.extract(SURFACE_RELIEF_CAMERAS.near);
    expect(repeated.surfaces.map((surface) => surface.mesh)).toEqual(firstMeshes);
    expect(host.surfaceReliefReviews.every((review) => review.cacheHit)).toBe(true);
    const far = host.extract({ ...SURFACE_RELIEF_CAMERAS.far, position: [0, 2, 500] });
    const selected = selectRenderProducts(far, 720);
    expect(selected.decisions.every((decision) => decision.kind === "parametric-mesh")).toBe(true);
    for (let index = 0; index < far.surfaces.length; index++) {
      const coarse = far.surfaces[index].renderProducts?.find(
        (product) => product.kind === "parametric-mesh",
      );
      if (coarse?.kind !== "parametric-mesh") throw new Error("Missing coarse realization");
      expect(selected.scene.surfaces[index].mesh).toBe(coarse.mesh);
      expect(coarse.errors.some((error) => error.kind !== "unknown" && error.metric === "depth")).toBe(false);
      expect(
        coarse.errors.some((error) => error.kind === "unknown" && error.reason.includes("radiance")),
      ).toBe(true);
    }
    const costs: RenderCostObservation[] = far.surfaces
      .flatMap((surface) => surface.renderProducts ?? [])
      .map((product) => ({
        productKey: product.key,
        adapter: "test-adapter",
        browser: "test-browser",
        kernelVersion: RENDER_KERNEL_VERSION,
        fixture: "shared-scene",
        gpuP50Ms: product.kind === "direct-mesh" ? 1 : 100,
        gpuP95Ms: product.kind === "direct-mesh" ? 1 : 100,
        preparationMs: 0,
      }));
    const measured = selectRenderProducts(far, 720, { costs }, "test-adapter", "test-browser");
    expect(measured.decisions.every((decision) => decision.kind === "direct-mesh")).toBe(true);
    const before = host.resourceUsage.reliefBytes;
    const edited = structuredClone(project);
    const material = edited.documents.find((document) => document.id === "surface-relief-bark");
    if (material?.kind !== "material" || !material.appearance?.relief)
      throw new Error("Missing bark material");
    material.appearance.relief.seed++;
    await host.setProject(edited);
    const changed = host.extract(SURFACE_RELIEF_CAMERAS.near);
    expect(changed.surfaces[0].mesh).not.toBe(firstMeshes[0]);
    expect(changed.surfaces[1].mesh).toBe(firstMeshes[1]);
    expect(host.resourceUsage.reliefBytes).toBeGreaterThan(before);
  } finally {
    host.dispose();
  }
  expect(host.resourceUsage.reliefBytes).toBe(0);
});
