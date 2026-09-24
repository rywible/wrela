import { type FrontierCandidate, frontierProductKey } from "@wrela/examples/render-frontier";
import {
  createSurfaceReliefLookdevProject,
  SURFACE_RELIEF_CAMERAS,
} from "@wrela/examples/surface-relief-lookdev";
import {
  contentKey,
  type EvaluatedScene,
  type GpuFrameTiming,
  type RenderProductMetadata,
} from "@wrela/model";
import { RENDER_KERNEL_VERSION, WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";
import {
  aggregateImageDifferences,
  compareFrontierImages,
  compareScreenErrorDelta,
  observedDistribution,
  silhouetteDistance,
} from "../frontier-image-metrics";
import { linearImage } from "../rendering-compiler/fixture";

type Choice = "automatic" | "forced-near";
type EvidenceImage = {
  choice: Choice;
  view: number;
  mode: "beauty" | "silhouette";
  pixels: Float32Array;
  image: string;
  camera: EvaluatedScene["camera"];
};
function bytes(mesh: EvaluatedScene["surfaces"][number]["mesh"]) {
  return (
    mesh.positions.byteLength +
    mesh.normals.byteLength +
    mesh.indices.byteLength +
    (mesh.colors?.byteLength ?? 0) +
    (mesh.wind?.byteLength ?? 0) +
    (mesh.reliefCoordinates?.byteLength ?? 0) +
    (mesh.reliefNormals?.byteLength ?? 0) +
    (mesh.thinCoverage?.uv.byteLength ?? 0) +
    (mesh.thinCoverage?.levels.reduce((sum, level) => sum + level.byteLength, 0) ?? 0)
  );
}
const encoded = (bytes: Uint8Array) => {
  let binary = "";
  for (let index = 0; index < bytes.length; index += 32768)
    binary += String.fromCharCode(...bytes.subarray(index, index + 32768));
  return btoa(binary);
};
export async function createFrontierReliefFixture() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Frontier canvas missing");
  const project = createSurfaceReliefLookdevProject(true, true),
    host = new BrowserSceneHost(project);
  const initializationStart = performance.now();
  host.setViewportHeight(canvas.clientHeight);
  await host.prepare("surface-relief-specimens", "surface-relief-stage", "review");
  const hostPreparationMs = performance.now() - initializationStart;
  const failures: string[] = [];
  const renderer = await WebGPURenderer.create(canvas, {
    quality: "balanced",
    pixelRatio: 1,
    resolutionScale: 1,
    antialiasing: "spatial",
    renderCompiler: { geometry: "auto", maxGeometryErrorPixels: 0.25 },
    onDiagnostic: (diagnostic) => {
      if (diagnostic.severity === "error") failures.push(diagnostic.message);
    },
  });
  const rendererPreparationMs = performance.now() - initializationStart - hostPreparationMs;
  const distances = [1, 3, 8, 18, 31];
  const cameras = distances.map((scale) => ({
    ...structuredClone(SURFACE_RELIEF_CAMERAS.near),
    position: SURFACE_RELIEF_CAMERAS.near.position.map(
      (value, index) =>
        SURFACE_RELIEF_CAMERAS.near.target[index] +
        (value - SURFACE_RELIEF_CAMERAS.near.target[index]) * scale,
    ) as [number, number, number],
  }));
  const prepare = (choice: Choice, view: number, mode: "beauty" | "silhouette" = "beauty") => {
    const start = performance.now();
    host.updateView(cameras[view]);
    const scene = host.extract(cameras[view], mode);
    scene.time = 0;
    scene.grid = false;
    if (choice === "forced-near")
      scene.surfaces = scene.surfaces.map((surface) => ({
        ...surface,
        renderProducts: surface.renderProducts?.filter((product) => product.kind === "direct-mesh"),
      }));
    return { scene, milliseconds: performance.now() - start };
  };
  const assertComplete = () => {
    if (
      !renderer.completeness.complete ||
      failures.length ||
      host.diagnostics.some((diagnostic) => diagnostic.severity === "error")
    )
      throw Error(`Incomplete frontier capture: ${failures.join("; ")}`);
  };
  const renderComplete = async (scene: EvaluatedScene) => {
    for (let attempt = 0; attempt < 8; attempt++) {
      renderer.render(scene);
      await renderer.capture();
      if (renderer.completeness.complete) {
        assertComplete();
        return;
      }
    }
    throw Error("Frontier upload budget did not settle");
  };
  const images: EvidenceImage[] = [];
  return {
    async run(samples = 30) {
      if (!Number.isInteger(samples) || samples < 30 || samples > 120 || samples % cameras.length)
        throw Error("Choose 30–120 samples, divisible by five views");
      if (images.length) throw Error("Create a fresh fixture for another timing trial");
      const choices: Choice[] = ["forced-near", "automatic"];
      // Warm both representations/cameras before matched, alternating timing trials.
      for (let view = 0; view < cameras.length; view++)
        for (const choice of choices) await renderComplete(prepare(choice, view).scene);
      await renderer.flushGpuTimings();
      renderer.drainGpuTimings();
      const tags = new Map<number, { choice: Choice; view: number; preparationMs: number }>(),
        gpuFrames: GpuFrameTiming[] = [];
      const products = new Map<Choice, Map<string, FrontierCandidate["products"][number]>>(
        choices.map((choice) => [choice, new Map()]),
      );
      const selectedBytes: Record<Choice, number[]> = { automatic: [], "forced-near": [] };
      const selections: {
        frame: number;
        choice: Choice;
        view: number;
        products: string[];
        selectedBytes: number;
      }[] = [];
      for (let sample = 0; sample < samples; sample++) {
        const view = sample % cameras.length,
          order = Math.floor(sample / cameras.length) % 2 ? [...choices].reverse() : choices;
        for (const choice of order) {
          await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
          const prepared = prepare(choice, view);
          renderer.render(prepared.scene);
          assertComplete();
          const frame = renderer.measurements.frame;
          tags.set(frame, { choice, view, preparationMs: prepared.milliseconds });
          const chosen: RenderProductMetadata[] = [];
          let payload = 0;
          for (const surface of prepared.scene.surfaces) {
            const decision = renderer.completeness.realizations?.find((value) => value.id === surface.id);
            const product = surface.renderProducts?.find((value) => value.key === decision?.key);
            if (!product) throw Error("Every frontier specimen needs selected compiled-product evidence");
            const { key, sourceKey, algorithmVersion, domainKey } = product;
            products.get(choice)?.set(key, { key, sourceKey, algorithmVersion, domainKey });
            chosen.push(product);
            payload += bytes(product.kind === "parametric-mesh" ? product.mesh : surface.mesh);
          }
          selectedBytes[choice].push(payload);
          selections.push({
            frame,
            choice,
            view,
            products: chosen.map((product) => product.key),
            selectedBytes: payload,
          });
          gpuFrames.push(...renderer.drainGpuTimings());
        }
      }
      await renderer.flushGpuTimings();
      gpuFrames.push(...renderer.drainGpuTimings());
      const measured = gpuFrames.flatMap((frame) => {
        const tag = tags.get(frame.frame);
        return tag ? [{ ...frame, ...tag }] : [];
      });
      if (new Set(measured.map((frame) => frame.frame)).size !== measured.length)
        throw Error("Duplicate timing frames");
      for (const choice of choices)
        if (measured.filter((frame) => frame.choice === choice).length < samples)
          throw Error("Frame-tagged GPU timing samples are incomplete");
      for (let view = 0; view < cameras.length; view++)
        for (const mode of ["beauty", "silhouette"] as const)
          for (const choice of choices) {
            const scene = prepare(choice, view, mode).scene;
            await renderComplete(scene);
            const pixels = await linearImage(renderer),
              blob = await renderer.capture();
            const image = await new Promise<string>((resolve, reject) => {
              const reader = new FileReader();
              reader.onload = () => resolve(String(reader.result));
              reader.onerror = () => reject(reader.error);
              reader.readAsDataURL(blob);
            });
            images.push({ choice, view, mode, pixels, image, camera: cameras[view] });
          }
      const resolution = renderer.measurements.renderResolution;
      if (!resolution) throw Error("Renderer did not record linear target resolution");
      const [width, height] = resolution;
      const image = (choice: Choice, view: number, mode: "beauty" | "silhouette") => {
        const value = images.find(
          (value) => value.choice === choice && value.view === view && value.mode === mode,
        );
        if (!value) throw Error("Missing comparison image");
        return value.pixels;
      };
      const sourceKey = contentKey(project),
        referenceKey = contentKey({
          sourceKey,
          products: [...(products.get("forced-near")?.keys() ?? [])].sort(),
          cameras,
          width,
          height,
          format: "rgba16float",
          mode: "finite-near-reference",
        });
      const domain = {
        sourceKey,
        fixture: "relief-five-view-trajectory",
        cameraKey: contentKey(cameras),
        lightingKey: contentKey(
          project.documents.filter((document) => ["lighting", "environment"].includes(document.kind)),
        ),
        motionKey: "static-scene-camera-distance-trajectory",
        width,
        height,
        adapter: renderer.measurements.adapter,
        browser: navigator.userAgent,
        kernelVersion: RENDER_KERNEL_VERSION,
        referenceKey,
        errorDomain: `sampled-linear-masks-screen-error-delta-${referenceKey}`,
        cpuScope: "preparation" as const,
        memoryScope: "selected-payload" as const,
      };
      const perView: unknown[] = [];
      const candidates: FrontierCandidate[] = choices.map((choice) => {
        const linear = [],
          coverage = [],
          temporal = [],
          silhouette: (number | null)[] = [];
        for (let view = 0; view < cameras.length; view++) {
          const reference = image("forced-near", view, "beauty"),
            current = image(choice, view, "beauty"),
            a = image("forced-near", view, "silhouette"),
            b = image(choice, view, "silhouette");
          const radiance = compareFrontierImages(reference, current),
            mask = compareFrontierImages(a, b),
            contour = silhouetteDistance(a, b, width, height);
          linear.push(radiance);
          coverage.push(mask);
          silhouette.push(contour);
          if (view)
            temporal.push(
              compareScreenErrorDelta(
                image("forced-near", view - 1, "beauty"),
                image(choice, view - 1, "beauty"),
                reference,
                current,
              ),
            );
          perView.push({
            choice,
            view,
            camera: cameras[view],
            radiance,
            coverage: mask,
            silhouetteMaximumPixels: contour,
          });
        }
        const radiance = aggregateImageDifferences(linear),
          mask = aggregateImageDifferences(coverage),
          delta = aggregateImageDifferences(temporal);
        const quality: FrontierCandidate["quality"] = [];
        for (const [prefix, measurement, units, metric] of [
          ["linear-radiance", radiance, "linear-radiance", "linear-radiance"],
          ["temporal", delta, "linear-radiance-difference", "temporal"],
        ] as const)
          for (const suffix of ["rms", "max"] as const)
            quality.push({
              coordinate: `${prefix}-${suffix}`,
              units,
              evidence: {
                kind: "measured",
                metric,
                rms: measurement.rms,
                maximum: measurement.maximum,
                domain: domain.errorDomain,
                reference: referenceKey,
                samples: measurement.samples,
              },
              uncertainty: choice === "forced-near" ? 0 : measurement.halfFormatBound,
            });
        for (const suffix of ["rms", "max"] as const)
          quality.push({
            coordinate: `coverage-${suffix}`,
            units: "fractional-coverage",
            evidence: {
              kind: "measured-coverage",
              rms: mask.rms,
              maximum: mask.maximum,
              domain: domain.errorDomain,
              reference: referenceKey,
              samples: mask.samples,
            },
            uncertainty: 0,
          });
        quality.push({
          coordinate: "silhouette-max-pixels",
          units: "pixels",
          evidence: silhouette.some((value) => value === null)
            ? { kind: "unknown", reason: "A contour vanished in one finite sampled view" }
            : {
                kind: "measured",
                metric: "silhouette",
                rms: Math.sqrt(
                  silhouette.reduce<number>((sum, value) => sum + (value ?? 0) ** 2, 0) / silhouette.length,
                ),
                maximum: Math.max(...(silhouette as number[])),
                domain: domain.errorDomain,
                reference: referenceKey,
                samples: cameras.length,
              },
          uncertainty: 0,
        });
        const timings = measured.filter((frame) => frame.choice === choice),
          gpu = observedDistribution(timings.map((frame) => frame.gpuMs)),
          cpu = observedDistribution(timings.map((frame) => frame.preparationMs));
        const selected = [...(products.get(choice)?.values() ?? [])].sort((a, b) =>
          a.key.localeCompare(b.key),
        );
        return {
          id: choice,
          label: choice === "forced-near" ? "Finite near reference" : "Compiler automatic selection",
          products: selected,
          domain,
          quality,
          cost: {
            observation: {
              productKey: frontierProductKey(selected),
              adapter: domain.adapter,
              browser: domain.browser,
              kernelVersion: domain.kernelVersion,
              fixture: domain.fixture,
              gpuP50Ms: gpu.p50,
              gpuP95Ms: gpu.p95,
              preparationMs: cpu.p95,
            },
            cpuMs: cpu.p95,
            ownedBytes: Math.max(...selectedBytes[choice]),
            samples: timings.length,
            uncertainty: { gpuMs: gpu.uncertainty, cpuMs: cpu.uncertainty, ownedBytes: 0 },
          },
          provenance: [{ artifact: "measurements.json", pointer: `/candidates/${choices.indexOf(choice)}` }],
        };
      });
      return {
        project,
        candidates,
        perView,
        timingFrames: measured,
        selections,
        hostPreparationMs,
        rendererPreparationMs,
        hostResources: host.resourceUsage,
        rendererMeasurements: renderer.measurements,
        reference: {
          key: referenceKey,
          scope:
            "Highest triangle budget among tested candidates: finite compiled near mesh; not ideal authored field or convergence proof",
          triangles: host.surfaceReliefReviews.map((review) => ({
            source: review.source,
            triangles: review.triangles,
            unresolved: review.budgetLimited,
          })),
        },
        imageCount: images.length,
        failures,
        notes: [
          "Five fixed distance views, one unchanged source/light, spatial rendering, time zero. Three alternating-order rounds per ordering give 30 GPU/CPU samples per candidate.",
          "Radiance uses actual scene-linear binary16 targets promoted exactly to binary32. Auto uncertainty includes a conservative pairwise binary16 quantization envelope; reference identity has exact zero error to the stored finite reference.",
          "Coverage/contours use native diagnostic sample occupancy (binary pixel-center masks), not analytic continuous silhouette area or resolved needle coverage. Temporal metric is change in matched screen-space error across these views, without motion reprojection.",
          "Timing uncertainty encloses the actual retained sample range around p95; it is not a confidence guarantee for future frames. CPU is cached scene-packet preparation; initial host compilation and renderer setup are reported separately. Owned bytes are peak selected mesh payload, not freed renderer residency.",
          "Acceptance is only against the explicitly named sampled finite reference and requested coordinates. No artistic approval, continuous-view bound or universal Pareto claim follows.",
        ],
      };
    },
    evidence(index: number) {
      const value = images[index];
      if (!value) throw Error("Unknown evidence image");
      const { pixels, ...metadata } = value;
      return {
        ...metadata,
        hdrBase64: encoded(new Uint8Array(pixels.buffer, pixels.byteOffset, pixels.byteLength)),
        format: "little-endian rgba32float promoted from rgba16float",
      };
    },
    dispose() {
      images.length = 0;
      renderer.dispose();
      host.dispose();
    },
  };
}
