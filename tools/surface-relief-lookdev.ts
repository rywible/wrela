import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { createSurfaceReliefLookdevProject } from "@wrela/examples/surface-relief-lookdev";
import { parseProject } from "@wrela/model";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import {
  type createSurfaceReliefLookdevFixture,
  SURFACE_RELIEF_SHOTS,
} from "./fixtures/surface-relief-lookdev";

const argument = (key: string, fallback: string) =>
  process.argv.find((value) => value.startsWith(`--${key}=`))?.slice(key.length + 3) ?? fallback;
const revision = argument("revision", "v1");
const frames = Number(argument("frames", "12"));
if (!/^[a-z0-9-]+$/.test(revision) || !Number.isInteger(frames) || frames < 4 || frames > 60)
  throw new Error("Use a simple revision name and 4–60 timing frames");
const root = resolve("output/surface-relief-lookdev", revision);
await mkdir(root, { recursive: true });
for (const displaced of [false, true])
  await Bun.write(
    resolve(root, displaced ? "source-relief.json" : "source-smooth.json"),
    JSON.stringify(parseProject(createSurfaceReliefLookdevProject(displaced)), null, 2),
  );
type Capture = Awaited<ReturnType<Awaited<ReturnType<typeof createSurfaceReliefLookdevFixture>>["capture"]>>;
await withBrowser(
  async (view, _output, errors) => {
    const server = await fixtureServer(
      `import {createSurfaceReliefLookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/surface-relief-lookdev.ts"))};createSurfaceReliefLookdevFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      root,
    );
    const reports: Omit<Capture, "image">[] = [];
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw new Error(String(failure));
      for (const shot of SURFACE_RELIEF_SHOTS)
        for (const displaced of [false, true]) {
          const capture = await view.evaluate<Capture>(
            `fixture.capture(${displaced},${JSON.stringify(shot)},${frames})`,
          );
          const { image, ...report } = capture;
          reports.push(report);
          await Bun.write(
            resolve(root, `${shot}-${displaced ? "relief" : "smooth"}.png`),
            Buffer.from(image.split(",")[1], "base64"),
          );
          await Bun.write(
            resolve(root, "capture.json"),
            JSON.stringify(
              {
                revision,
                reports,
                errors,
                visualAcceptance: "requires visual inspection",
                measurementScope:
                  "Identical flat-color source recipes; physical geometry is produced through BrowserSceneHost. Silhouette differences count rendered pixels. Timings are adapter-specific; null GPU timing means unavailable.",
              },
              null,
              2,
            ),
          );
          if (!capture.completeness.complete || capture.failures.length)
            throw new Error(`Incomplete ${shot}: ${JSON.stringify(report)}`);
          if (/swiftshader|software|llvmpipe/i.test(capture.measurements.adapter))
            throw new Error("Hardware rendering required");
        }
      const forced = await view.evaluate<Capture>(`fixture.capture(true,"far-clay",${frames},true)`);
      const { image: forcedImage, ...forcedReport } = forced;
      reports.push(forcedReport);
      await Bun.write(
        resolve(root, "far-clay-relief-forced-near.png"),
        Buffer.from(forcedImage.split(",")[1], "base64"),
      );
      if (!forced.completeness.complete || forced.failures.length)
        throw new Error("Incomplete forced-near control");
      const automatic = reports.find(
        (entry) =>
          entry.shot === "far-clay" && entry.variant === "physical-relief" && entry.choice === "automatic",
      );
      const matchedViewChoice = {
        camera: forced.camera,
        automatic: automatic && {
          triangles: automatic.measurements.triangles,
          geometryPayloadBytes: automatic.selectedGeometryBytes,
          gpuOwnedBytes: automatic.measurements.gpuBytes,
          gpuBufferBytes: automatic.measurements.bufferBytes,
          timing: automatic.timing,
          selection: automatic.completeness.realizations,
          candidates: automatic.geometry.map((surface) => surface.candidates),
        },
        forcedNear: {
          triangles: forced.measurements.triangles,
          geometryPayloadBytes: forced.selectedGeometryBytes,
          gpuOwnedBytes: forced.measurements.gpuBytes,
          gpuBufferBytes: forced.measurements.bufferBytes,
          timing: forced.timing,
          selection: forced.completeness.realizations,
        },
        scope:
          "Same authored relief and far camera; forced near removes coarse candidates only. GPU allocations include renderer cache residency. Short observations do not establish a Pareto optimum; normal and radiance error remain unknown.",
      };
      await Bun.write(
        resolve(root, "capture.json"),
        JSON.stringify(
          { revision, reports, errors, visualAcceptance: "requires visual inspection" },
          null,
          2,
        ),
      );
      const comparisons = SURFACE_RELIEF_SHOTS.map((shot) => {
        const smooth = reports.find((entry) => entry.shot === shot && entry.variant === "smooth");
        const relief = reports.find((entry) => entry.shot === shot && entry.variant === "physical-relief");
        return {
          shot,
          smoothTriangles: smooth?.measurements.triangles,
          reliefTriangles: relief?.measurements.triangles,
          smoothGpuP50Ms: smooth?.timing.gpuP50Ms,
          reliefGpuP50Ms: relief?.timing.gpuP50Ms,
          changedSilhouettePixels: relief?.silhouette?.changedPixels,
          selections: relief?.completeness.realizations?.map((entry) => ({
            id: entry.id,
            kind: entry.kind,
            reason: entry.reason,
          })),
        };
      });
      await Bun.write(
        resolve(root, "comparisons.json"),
        JSON.stringify(
          {
            comparisons,
            matchedViewChoice,
            interpretation:
              "Near silhouette differences establish actual geometry change. Far captures report selected coarse/near candidates and observed timing; geometry bounds do not certify identical shading.",
          },
          null,
          2,
        ),
      );
      if (errors.length) throw new Error(errors.join("\n"));
      console.log(
        JSON.stringify({ root, captures: reports.length, adapter: reports[0]?.measurements.adapter }),
      );
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  960,
  640,
);
