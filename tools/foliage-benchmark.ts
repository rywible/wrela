import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createVegetationBenchmark } from "./fixtures/vegetation-benchmark";
import { vegetationStudies } from "./fixtures/vegetation-study";
import { quantiles } from "./rendering-compiler/timing";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--snapshot-run")) {
  const destination = resolve("output/foliage-benchmark", `${Date.now()}-${process.pid}`, "source");
  await snapshotSource(destination, "foliage-benchmark");
  console.log(JSON.stringify({ snapshot: destination }));
  process.exit(
    await Bun.spawn(
      [process.execPath, "tools/foliage-benchmark.ts", ...process.argv.slice(2), "--snapshot-run"],
      { cwd: destination, stdout: "inherit", stderr: "inherit" },
    ).exited,
  );
}
const frames = Number(process.argv.find((a) => a.startsWith("--frames="))?.slice(9) ?? 1200);
if (!Number.isInteger(frames) || frames < 120 || frames > 12000 || frames % 24)
  throw Error("Frames must be divisible by 24, within 120..12000");
type Batch = Awaited<ReturnType<Awaited<ReturnType<typeof createVegetationBenchmark>>["batch"]>>;
await withBrowser(
  async (view, output, errors) => {
    const runs = [];
    for (const shared of [false, true, true, false]) {
      const study = vegetationStudies({ architecture: true })[1];
      for (const doc of study.project.documents)
        if (doc.kind === "vegetation" && doc.botanical?.conifer?.architecture) {
          doc.botanical.conifer.architecture.sharedShoots = shared;
          doc.botanical.conifer.architecture.canopyVisibility = false;
        }
      const index = study.frames.findIndex((f) => f.id === "stand-gameplay");
      const server = await fixtureServer(
        `import {createVegetationBenchmark} from ${JSON.stringify(resolve("tools/fixtures/vegetation-benchmark.ts"))};createVegetationBenchmark(${JSON.stringify(study)},${index},{antialiasing:"temporal",resolutionScale:1,finiteSun:false}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
        output,
      );
      const batches: Batch[] = [];
      try {
        await view.navigate(String(server.url));
        await waitFor(view, "window.ready||window.failure", 60000);
        const failure = await view.evaluate("window.failure");
        if (failure) throw Error(String(failure));
        for (let n = 0; n < 240; n += 24) await view.evaluate("fixture.batch(24,true,false,false,4)");
        for (let n = 0; n < frames; n += 24) {
          const batch = await view.evaluate<Batch>("fixture.batch(24,true,false,false,4)");
          if (
            /software|swiftshader|llvmpipe/i.test(batch.measurements.adapter) ||
            batch.errors.length ||
            batch.dropped ||
            batch.frames.some((f) => !f.complete) ||
            batch.gpu.length !== 24 ||
            batch.frames.some((f) => !batch.gpu.some((g) => g.frame === f.frame))
          )
            throw Error("Incomplete or unmatched hardware trial");
          batches.push(batch);
        }
        const cpu = batches.flatMap((b) => b.frames),
          gpu = batches.flatMap((b) => b.gpu);
        const summary = {
          shared,
          gpu: quantiles(gpu.map((g) => g.gpuMs)),
          cpu: quantiles(cpu.map((f) => f.renderMs + f.prepareMs)),
          gpuBytes: Math.max(...cpu.map((f) => f.gpuBytes)),
          triangles: quantiles(cpu.map((f) => f.triangles)),
          uploadedBytes: quantiles(cpu.map((f) => f.uploadedBytes)),
        };
        runs.push({ summary, batches });
        console.log(JSON.stringify({ output, ...summary }));
        await Bun.write(
          join(output, "benchmark.json"),
          JSON.stringify(
            {
              frames,
              warmupFrames: 240,
              sequence: "expanded/shared/shared/expanded",
              width: 1920,
              height: 1080,
              aa: "temporal",
              canopyVisibility: false,
              scope:
                "Identical source and simulation ticks; shared path adds per-shoot camera/light selection and explicit resolved needles. Both paths use approximate proxies at distance. Diagnostic ABBA, not sustained acceptance.",
              runs,
              errors,
            },
            null,
            2,
          ),
        );
      } finally {
        await view.evaluate("window.fixture?.dispose()").catch(() => {});
        server.stop(true);
      }
    }
    if (errors.length) throw Error(errors.join("\n"));
  },
  1920,
  1080,
  "chrome",
  300000,
);
