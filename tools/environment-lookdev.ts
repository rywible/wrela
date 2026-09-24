import { join, resolve } from "node:path";
import { createDayNightLookdev, createEnvironmentLookdev } from "@wrela/examples/environment-lookdev";
import { createLookdevMaterials } from "@wrela/examples/material-lookdev";
import { createTerrainLookdev } from "@wrela/examples/terrain-lookdev";
import { parseProject, type WorldDefinition } from "@wrela/model";
import { sampleDayCycle } from "@wrela/runtime";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createLookdevFixture, LookdevStudy } from "./fixtures/lookdev";

// Full-size artistic review, separate from the small environment-authoring smoke test.
const cycle = process.argv.includes("--cycle");
const motion = process.argv.includes("--motion");
const benchmark =
  process.argv.includes("--bench") ||
  process.argv.includes("--bench-move") ||
  process.argv.includes("--bench-turn");
const benchmarkMove = process.argv.includes("--bench-move");
const benchmarkTurn = process.argv.includes("--bench-turn");
const turnReview = process.argv.includes("--turn-review");
const environment = cycle ? createDayNightLookdev() : createEnvironmentLookdev();
const sky = environment.documents.find((document) => document.kind === "environment");
if (sky?.kind !== "environment" || !sky.sequence) throw new Error("Missing sky review sequence");
const daylight = sky.sequence.keyframes[0].state;
// Extra fixed-sun states preserve the earlier regression views.
if (!cycle)
  sky.sequence = {
    ...sky.sequence,
    duration: 180,
    loop: false,
    keyframes: [
      ...sky.sequence.keyframes,
      {
        time: 96,
        state: {
          ...daylight,
          sunElevation: 0.1,
          turbidity: 1.5,
          fogDensity: 0.0001,
          cloudCover: 0.38,
          grade: { exposureCompensation: 0.7, tint: [1, 1, 1] },
        },
      },
      {
        time: 128,
        state: {
          ...daylight,
          sunElevation: -0.06,
          turbidity: 1.5,
          fogDensity: 0.00005,
          cloudCover: 0.38,
          // A blue-hour review needs photographic exposure adaptation; the
          // physical source remains unboosted and can still be tested at fixed exposure.
          grade: { exposureCompensation: 4.5, tint: [1, 1, 1] },
        },
      },
      { time: 160, state: { ...daylight, sunElevation: 1.2, cloudCover: 0.38 } },
    ],
  };
const terrain = createTerrainLookdev();
const world: WorldDefinition = {
  id: "environment-lookdev-world",
  name: "Alpine creek and changing weather",
  schemaVersion: 1,
  kind: "world",
  dependencies: [],
  generatorVersion: "wrela-world-1",
  terrain: terrain.terrain,
  environment: environment.environment,
  lighting: environment.lighting,
  water: environment.water,
  populations: [],
  instances: terrain.placements,
};
const project = parseProject({
  schemaVersion: 1,
  id: "environment-lookdev",
  name: world.name,
  documents: [...createLookdevMaterials(), ...terrain.documents, ...environment.documents, world],
  entry: world.id,
});
const landscape = { position: [14, 7, -24], target: [-6, 2, 4], fov: 48 } as const;
const reviewTurnCamera = (degrees: number) => {
  const position: [number, number, number] = [...landscape.position];
  const dx = -6 - position[0],
    dz = 4 - position[2];
  const angle = (degrees * Math.PI) / 180;
  return {
    position,
    target: [
      position[0] + dx * Math.cos(angle) + dz * Math.sin(angle),
      28,
      position[2] + dz * Math.cos(angle) - dx * Math.sin(angle),
    ] as [number, number, number],
    fov: 60,
  };
};
const nightSun = sky.dayCycle ? sampleDayCycle(sky.dayCycle, 60) : null;
const eveningSun = sky.dayCycle ? sampleDayCycle(sky.dayCycle, 40) : null;
const sunTarget: [number, number, number] = eveningSun
  ? [
      landscape.position[0] + 100 * Math.cos(eveningSun.elevation) * Math.sin(eveningSun.azimuth),
      landscape.position[1] + 100 * Math.sin(eveningSun.elevation),
      landscape.position[2] + 100 * Math.cos(eveningSun.elevation) * Math.cos(eveningSun.azimuth),
    ]
  : [0, 10, 100];
const moonTarget = nightSun
  ? ([
      landscape.position[0] - 100 * Math.cos(nightSun.elevation) * Math.sin(nightSun.azimuth),
      landscape.position[1] - 100 * Math.sin(nightSun.elevation),
      landscape.position[2] - 100 * Math.cos(nightSun.elevation) * Math.cos(nightSun.azimuth),
    ] as [number, number, number])
  : ([0, 30, 0] as [number, number, number]);
const input: LookdevStudy = {
  project,
  subject: world.id,
  quality: process.argv.includes("--quality=low")
    ? "low"
    : process.argv.includes("--quality=high")
      ? "high"
      : "balanced",
  frames: turnReview
    ? [0, 1, 2, 2.9, 3.1, 4, 6].map((degrees, index) => ({
        id: `turn-${index}`,
        camera: reviewTurnCamera(degrees),
        time: 20,
      }))
    : motion
      ? Array.from({ length: 12 }, (_, index) => ({
          id: `cycle-motion-${index}`,
          camera: { ...landscape, position: [...landscape.position], target: [...landscape.target] },
          time: 20 + index / 30,
        }))
      : cycle
        ? [
            {
              id: "cycle-dawn",
              camera: { ...landscape, position: [...landscape.position], target: [...landscape.target] },
              time: 0,
            },
            {
              id: "cycle-noon",
              camera: { ...landscape, position: [...landscape.position], target: [...landscape.target] },
              time: 20,
            },
            {
              id: "cycle-storm",
              camera: { ...landscape, position: [...landscape.position], target: [...landscape.target] },
              time: 30,
            },
            {
              id: "cycle-sunset",
              camera: { ...landscape, position: [...landscape.position], target: [...landscape.target] },
              time: 40,
            },
            {
              id: "cycle-golden-hour",
              camera: { position: [...landscape.position], target: sunTarget, fov: 60 },
              time: 40,
            },
            {
              id: "cycle-night",
              camera: { ...landscape, position: [...landscape.position], target: [...landscape.target] },
              time: 60,
            },
            { id: "cycle-stars", camera: { position: [14, 7, -24], target: [-6, 45, 4], fov: 70 }, time: 60 },
            { id: "cycle-moon", camera: { position: [14, 7, -24], target: moonTarget, fov: 40 }, time: 60 },
          ]
        : [
            {
              id: "environment-clear",
              camera: { ...landscape, position: [...landscape.position], target: [...landscape.target] },
              time: 0,
            },
            {
              id: "environment-clouded",
              camera: { ...landscape, position: [...landscape.position], target: [...landscape.target] },
              time: 32,
            },
            {
              id: "environment-clearing",
              camera: { ...landscape, position: [...landscape.position], target: [...landscape.target] },
              time: 64,
            },
            {
              id: "environment-shore",
              camera: { position: [10, 2.5, -20], target: [5, -0.15, -13], fov: 45 },
              time: 0,
            },
            {
              id: "environment-low-sun",
              camera: { ...landscape, position: [...landscape.position], target: [...landscape.target] },
              time: 96,
            },
            {
              id: "environment-twilight",
              camera: { position: [14, 7, -24], target: [-45, 8, -52], fov: 60 },
              time: 128,
            },
            {
              id: "environment-zenith",
              camera: { position: [14, 7, -24], target: [14, 80, -23], fov: 80 },
              time: 160,
            },
            {
              id: "environment-sky",
              camera: { position: [14, 7, -24], target: [-6, 28, 4], fov: 60 },
              time: 32,
            },
          ],
};
const filter = process.argv.find((arg) => arg.startsWith("--frame="))?.slice(8);
if (filter) input.frames = input.frames.filter((frame) => frame.id === filter);
if (!input.frames.length) throw new Error("Unknown environment review frame");
type Frame = Awaited<ReturnType<Awaited<ReturnType<typeof createLookdevFixture>>["frame"]>>;
await withBrowser(
  async (view, output, errors) => {
    await Bun.write(join(output, "authored-project.json"), JSON.stringify(project, null, 2));
    const server = await fixtureServer(
      `import {createLookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/lookdev.ts"))};createLookdevFixture(${JSON.stringify(input)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw new Error(String(failure));
      if (benchmark) {
        const result = await view.evaluate<{
          samples: { time: number; gpuTimings: unknown[]; measurements: { gpuMs: number } }[];
          diagnostics: string[];
          adapter: string;
        }>(
          benchmarkMove
            ? "fixture.benchmark(Array.from({length:63},()=>20),undefined,0.12)"
            : benchmarkTurn
              ? "fixture.benchmark(Array.from({length:63},()=>20),undefined,0,0.004363323)"
              : "fixture.benchmark(Array.from({length:63},(_,i)=>20+i/30))",
        );
        await Bun.write(join(output, "sky-benchmark.json"), JSON.stringify({ ...result, errors }, null, 2));
        if (errors.length || result.diagnostics.length)
          throw new Error([...errors, ...result.diagnostics].join("\n"));
        console.log(JSON.stringify({ output, samples: result.samples.length, adapter: result.adapter }));
        return;
      }
      const frames: Omit<Frame, "image">[] = [];
      for (let index = 0; index < input.frames.length; index++) {
        const { image, ...frame } = await view.evaluate<Frame>(`fixture.frame(${index})`);
        await Bun.write(join(output, `${frame.id}.png`), Buffer.from(image.split(",")[1], "base64"));
        frames.push(frame);
        if (!frame.complete || /swiftshader|software|llvmpipe/i.test(frame.measurements.adapter))
          throw new Error("Complete hardware rendering is required for environment review");
      }
      await Bun.write(join(output, "environment-lookdev.json"), JSON.stringify({ frames, errors }, null, 2));
      if (errors.length) throw new Error(errors.join("\n"));
      console.log(JSON.stringify({ output, frames: frames.map((frame) => frame.id) }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  1024,
  768,
);
