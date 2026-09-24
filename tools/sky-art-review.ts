import { join, resolve } from "node:path";
import { createEnvironmentLookdev } from "@wrela/examples/environment-lookdev";
import { createLookdevMaterials } from "@wrela/examples/material-lookdev";
import { createTerrainLookdev } from "@wrela/examples/terrain-lookdev";
import { type Camera, type EnvironmentState, parseProject } from "@wrela/model";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createLookdevFixture, LookdevStudy } from "./fixtures/lookdev";

// Fixed physical weather/light states. All images go through Studio's production
// extraction, atmosphere, HDR and display passes; there is no screenshot-only sky.
const environment = createEnvironmentLookdev();
const sky = environment.documents.find((d) => d.kind === "environment");
if (!sky || sky.kind !== "environment" || !sky.sequence) throw Error("Missing environment");
const base: EnvironmentState = {
  ...sky.sequence.keyframes[0].state,
  sunElevation: 0.5,
  sunAzimuth: -1.1,
  sunIntensity: 3.5,
  cloudCover: 0.48,
  fogDensity: 0.00008,
  turbidity: 1.6,
  cloudscape: {
    ...sky.sequence.keyframes[0].state.cloudscape,
    development: 0.86,
    storminess: 0.12,
    highCloudCover: process.argv.includes("--no-high") ? 0 : 0.32,
    background: process.argv.includes("--formations-only") ? 0 : 0.31,
  },
  grade: { exposureCompensation: 0.8, tint: [1, 1, 1] },
};
if (process.argv.includes("--legacy-layout")) {
  base.cloudscape = { development: 0.86, storminess: 0.12, highCloudCover: 0.15 };
}
const cases: { id: string; state: EnvironmentState; camera?: Camera }[] = [
  { id: "cumulus-side", state: base },
  { id: "cumulus-back", state: { ...base, sunAzimuth: 0.1, sunElevation: 0.36 } },
  { id: "cumulus-front", state: { ...base, sunAzimuth: 2.6 } },
  {
    id: "storm",
    state: {
      ...base,
      cloudCover: 0.9,
      cloudscape: {
        ...base.cloudscape,
        development: 0.9,
        storminess: 0.85,
        highCloudCover: 0.3,
        background: 1,
      },
    },
  },
  {
    id: "golden",
    state: {
      ...base,
      cloudCover: 0.56,
      sunAzimuth: -0.35,
      sunElevation: 0.075,
      grade: { exposureCompensation: 1.3, tint: [1, 1, 1] },
    },
  },
  {
    id: "blue-hour",
    state: {
      ...base,
      cloudCover: 0.56,
      sunAzimuth: -0.35,
      sunElevation: -0.05,
      grade: { exposureCompensation: 4.5, tint: [1, 1, 1] },
    },
  },
  { id: "zenith", state: base, camera: { position: [14, 7, -24], target: [14, 100, -23], fov: 80 } },
  {
    id: "sunset",
    state: {
      ...base,
      cloudCover: 0.56,
      sunAzimuth: -0.35,
      sunElevation: -0.01,
      grade: { exposureCompensation: 2.4, tint: [1, 1, 1] },
    },
  },
];
if (process.argv.includes("--morphology")) {
  cases.length = 0;
  for (const [id, kind, seed, maturity] of [
    ["tower-7", "tower", 7, 0.25],
    ["tower-31", "tower", 31, 0.25],
    ["tower-83", "tower", 83, 0.25],
    ["tower-211", "tower", 211, 0.25],
    ["mature-tower", "tower", 31, 0.95],
    ["bank", "bank", 12, 0.65],
    ["remnants", "wisp", 89, 0.9],
  ] as const) {
    cases.push({
      id,
      state: {
        ...base,
        cloudscape: {
          ...base.cloudscape,
          development: 0.86,
          storminess: 0.12,
          background: 0,
          highCloudCover: 0,
          midCloudCover: 0,
          formations: [
            {
              id: "growth",
              kind,
              seed,
              maturity,
              shear: 0.4,
              center: [0, 9000],
              base: 1100,
              size:
                kind === "tower"
                  ? [2600, 4400, 2400]
                  : kind === "bank"
                    ? [3500, 2400, 2400]
                    : [3500, 1200, 1900],
              yaw: 0,
              density: 1,
              erosion: kind === "wisp" ? 0.72 : 0.45,
            },
          ],
        },
      },
      camera: { position: [0, 7, 0], target: [0, 2700, 9000], fov: 48 },
    });
  }
}
if (process.argv.includes("--layers")) {
  cases.push({
    id: "night",
    state: { ...base, sunElevation: -0.6, grade: { exposureCompensation: 3.5, tint: [1, 1, 1] } },
  });
  for (const [id, midCloudCover, highCloudCover] of [
    ["middle-only", 0.5, 0],
    ["cirrus-only", 0, 0.5],
    ["layered-horizon", 0.4, 0.4],
  ] as const) {
    cases.push({
      id,
      state: {
        ...base,
        cloudCover: 0,
        cloudscape: {
          ...base.cloudscape,
          development: 0.86,
          storminess: 0.12,
          midCloudCover,
          highCloudCover,
        },
      },
      camera: { position: [14, 7, -24], target: [14, id === "layered-horizon" ? 12 : 100, 76], fov: 65 },
    });
  }
}
sky.sequence = {
  duration: Math.max(72, cases.length * 8),
  loop: false,
  interpolation: "smooth",
  keyframes: cases.map((c, i) => ({ time: i * 8, state: c.state })),
};
const terrain = createTerrainLookdev();
const world = {
  terrain: terrain.terrain,
  id: "sky-art-world",
  name: "Cloud lighting review",
  schemaVersion: 1,
  kind: "world",
  dependencies: [],
  generatorVersion: "wrela-world-1",
  environment: environment.environment,
  lighting: environment.lighting,
  populations: [],
  instances: [],
};
const project = parseProject({
  schemaVersion: 1,
  id: "sky-art-review",
  name: "Cloud lighting review",
  entry: world.id,
  documents: [...createLookdevMaterials(), ...terrain.documents, ...environment.documents, world],
});
const input: LookdevStudy = {
  project,
  cloudReconstruction: process.argv.includes("--full") ? "full" : "temporal",
  subject: world.id,
  quality: process.argv.includes("--high") ? "high" : process.argv.includes("--low") ? "low" : "balanced",
  frames: cases.map((c, i) => ({
    id: c.id,
    time: i * 8,
    camera: c.camera ?? { position: [14, 7, -24], target: [-20, 43, 76], fov: 65 },
  })),
};
if (process.argv.includes("--around")) {
  input.frames.push(
    { id: "opposite", time: 0, camera: { position: [14, 7, -24], target: [48, 43, -124], fov: 65 } },
    { id: "east", time: 0, camera: { position: [14, 7, -24], target: [114, 43, 10], fov: 65 } },
    { id: "west", time: 0, camera: { position: [14, 7, -24], target: [-86, 43, -58], fov: 65 } },
  );
}
const only = process.argv.find((a) => a.startsWith("--frame="))?.slice(8);
if (only) input.frames = input.frames.filter((f) => f.id === only);
type Frame = Awaited<ReturnType<Awaited<ReturnType<typeof createLookdevFixture>>["frame"]>>;
await withBrowser(
  async (view, output, errors) => {
    await Bun.write(join(output, "authored-study.json"), JSON.stringify(input, null, 2));
    const server = await fixtureServer(
      `import {createLookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/lookdev.ts"))};createLookdevFixture(${JSON.stringify(input)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const frames = [];
      for (let i = 0; i < input.frames.length; i++) {
        const { image, ...frame } = await view.evaluate<Frame>(`fixture.frame(${i})`);
        await Bun.write(join(output, `${frame.id}.png`), Buffer.from(image.split(",")[1], "base64"));
        if (!frame.complete || /swiftshader|software|llvmpipe/i.test(frame.measurements.adapter))
          throw Error("Hardware capture required");
        frames.push(frame);
      }
      await Bun.write(join(output, "sky-art-review.json"), JSON.stringify({ frames, errors }, null, 2));
      if (errors.length) throw Error(errors.join("\n"));
      console.log(JSON.stringify({ output, frames: frames.map((f) => f.id) }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  1440,
  900,
  "chrome",
  600_000,
  process.argv.includes("--visual-only") ? "visual-review" : "measurement",
);
