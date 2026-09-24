import { join, resolve } from "node:path";
import { createWaterLookdev, waterLakeCamera, waterStudyCameras } from "@wrela/examples/water-lookdev";
import { createWaterValley, waterValleyCameras } from "@wrela/examples/water-valley";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createLookdevFixture, LookdevStudy } from "./fixtures/lookdev";

const ocean = process.argv.includes("--ocean"),
  bench = process.argv.includes("--bench");
const ablation = process.argv.find((arg) => arg.startsWith("--ablate="))?.slice(9);
const replacements: Record<string, [string, string]> = {
  waves: [
    "let sampled=waterBodyFilteredWaves(parameter,footprint);",
    "let sampled=WaterBodyWave(vec4f(0.0),vec2f(0.0),vec3f(1.0,0.0,1.0),vec3f(0.0));",
  ],
  transport: ["if(waterBody[4].w>0.5){color=", "if(false){color="],
  foam: ["if(coverage<0.001){return color;}", "if(true){return color;}"],
  caustics: ["let strength=waterBody[3].w;", "let strength=0.0;"],
};
if (ablation && !replacements[ablation]) throw new Error("Unknown water ablation");
const hook = ablation
  ? `const originalShader=GPUDevice.prototype.createShaderModule;GPUDevice.prototype.createShaderModule=function(descriptor){if(descriptor.code.includes('fn waterBodyResponse')){const pair=${JSON.stringify(replacements[ablation])};if(!descriptor.code.includes(pair[0]))throw Error('Stale water ablation');descriptor={...descriptor,code:descriptor.code.replace(pair[0],pair[1])};}return originalShader.call(this,descriptor);};`
  : "";
const integrated = process.argv.includes("--integrated"),
  surf = process.argv.includes("--surf");
const project = integrated
  ? createWaterValley()
  : createWaterLookdev(ocean ? "ocean" : surf ? "surf" : "creek");
const lighting = process.argv.find((arg) => arg.startsWith("--lighting="))?.slice(11);
const sky = project.documents.find((document) => document.kind === "environment");
if (lighting && sky?.kind === "environment") {
  if (lighting === "sunset") {
    sky.sunElevation = 0.12;
    sky.cloudCover = 0.2;
  } else if (lighting === "overcast") {
    sky.sunElevation = 0.8;
    sky.cloudCover = 0.95;
  } else if (lighting === "noon") {
    sky.sunElevation = 1.15;
    sky.cloudCover = 0.15;
  } else throw Error("Unknown water lighting study");
}
const camera = integrated
  ? waterValleyCameras[process.argv.find((a) => a.startsWith("--view="))?.slice(7) ?? "bank"]
  : process.argv.includes("--lake")
    ? waterLakeCamera
    : waterStudyCameras[project.entry];
const resolution = process.argv.find((arg) => arg.startsWith("--resolution="))?.slice(13);
if (resolution) {
  if (!["32", "64", "128"].includes(resolution)) throw Error("Unknown water resolution");
  const water = project.documents.find((document) => document.id === project.entry);
  if (water?.kind === "water" && water.domain) water.domain.resolution = Number(resolution) as 32 | 64 | 128;
}
const input: LookdevStudy = {
  project,
  waterWarmup: 2,
  subject: project.entry,
  stage: "neutral-stage",
  frames: [
    { id: ocean ? "ocean" : "creek", camera, time: 5 },
    { id: ocean ? "ocean-motion" : "creek-ripple", camera, time: 5.35 },
  ],
  quality: process.argv.includes("--low") ? "low" : process.argv.includes("--high") ? "high" : "balanced",
  antialiasing: process.argv.includes("--msaa") ? "msaa" : "spatial",
};
type Frame = Awaited<ReturnType<Awaited<ReturnType<typeof createLookdevFixture>>["frame"]>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `${hook}import {verifyWaterGpu} from ${JSON.stringify(resolve("tools/fixtures/water-conformance.ts"))};window.verifyWaterGpu=verifyWaterGpu;import {createLookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/lookdev.ts"))};createLookdevFixture(${JSON.stringify(input)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw new Error(String(failure));
      const conformance = await view.evaluate("verifyWaterGpu()");
      const { image, ...frame } = await view.evaluate<Frame>("fixture.frame(0)");
      if (process.argv.includes("--inspect"))
        await Bun.write(
          join(output, "inspection.json"),
          JSON.stringify(await view.evaluate("fixture.inspectWater()"), null, 2),
        );
      await Bun.write(join(output, `${frame.id}.png`), Buffer.from(image.split(",")[1], "base64"));
      await Bun.write(
        join(output, "water-lookdev.json"),
        JSON.stringify({ frame, conformance, errors }, null, 2),
      );
      if (bench) {
        const runs = [];
        for (const omitWater of [false, true, true, false])
          runs.push(await view.evaluate(`fixture.benchmarkWater(120,${omitWater})`));
        const result = {
          ablation: ablation ?? null,
          methodology:
            "ABBA whole-frame GPU timing with 45 warmup and 120 measured dynamic frames per run; opaque bed and sky retained in control",
          runs,
        };
        await Bun.write(join(output, "water-benchmark.json"), JSON.stringify(result, null, 2));
      }
      if (bench) await view.evaluate("fixture.frame(0)");
      if (!ocean && !surf && !integrated)
        await view.evaluate(`fixture.disturbWater(${JSON.stringify(project.entry)},0,10,2,3)`);
      const motion = await view.evaluate<Frame>("fixture.frame(1)");
      await Bun.write(join(output, `${motion.id}.png`), Buffer.from(motion.image.split(",")[1], "base64"));
      if (errors.length) throw new Error(errors.join("\n"));
      console.log(JSON.stringify({ output, frame: frame.id, adapter: frame.measurements.adapter }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  1920,
  1080,
  "chrome",
  600000,
  bench ? "measurement" : "visual-review",
);
