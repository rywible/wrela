import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createVegetationBenchmark } from "./fixtures/vegetation-benchmark";
import { vegetationStudies } from "./fixtures/vegetation-study";
import { quantiles } from "./rendering-compiler/timing";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--snapshot-run")) {
  const destination = resolve("output/vegetation-marginal", `${Date.now()}-${process.pid}`, "source");
  await snapshotSource(destination, "vegetation-marginal");
  console.log(JSON.stringify({ snapshot: destination }));
  process.exit(
    await Bun.spawn(
      [process.execPath, "tools/vegetation-marginal.ts", ...process.argv.slice(2), "--snapshot-run"],
      { cwd: destination, stdout: "inherit", stderr: "inherit" },
    ).exited,
  );
}
function signedQuantiles(values: number[]) {
  if (!values.length || values.some((value) => !Number.isFinite(value)))
    throw Error("Invalid marginal samples");
  const sorted = [...values].sort((a, b) => a - b);
  const q = (fraction: number) => sorted[Math.ceil(fraction * sorted.length) - 1];
  return {
    samples: sorted.length,
    p50: q(0.5),
    p95: q(0.95),
    p99: q(0.99),
    minimum: sorted[0],
    maximum: sorted[sorted.length - 1],
  };
}
const frames = Number(process.argv.find((arg) => arg.startsWith("--frames="))?.slice(9) ?? 1200);
if (!Number.isInteger(frames) || frames < 120 || frames > 12000 || frames % 24)
  throw Error("Frames must be 120..12000, divisible by 24");
const study = vegetationStudies()[1];
const frameIndex = study.frames.findIndex((frame) => frame.id === "stand-gameplay");
type Batch = Awaited<ReturnType<Awaited<ReturnType<typeof createVegetationBenchmark>>["batch"]>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createVegetationBenchmark} from ${JSON.stringify(resolve("tools/fixtures/vegetation-benchmark.ts"))};createVegetationBenchmark(${JSON.stringify(study)},${frameIndex},{antialiasing:"temporal",resolutionScale:1,finiteSun:false}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const results = [];
      for (let trial = 0; trial < 3; trial++) {
        const pair = new Map<boolean, Batch[]>();
        for (const vegetation of trial % 2 ? [false, true] : [true, false]) {
          await view.evaluate(`fixture.reset(${vegetation})`);
          // Equal simulation warmup produces the same camera, light, wind and
          // terrain sequence in both halves; no finite-sun eligibility changes.
          for (let i = 0; i < 5; i++) await view.evaluate("fixture.batch(24,true,false,false,4)");
          const batches: Batch[] = [];
          for (let submitted = 0; submitted < frames; submitted += 24) {
            const batch = await view.evaluate<Batch>("fixture.batch(24,true,false,false,4)");
            if (
              batch.errors.length ||
              batch.dropped ||
              batch.frames.some((f) => !f.complete) ||
              batch.gpu.length !== batch.frames.length ||
              batch.frames.some((f) => !batch.gpu.some((g) => f.frame === g.frame))
            )
              throw Error("Incomplete or unmatched trial");
            if (/software|swiftshader|llvmpipe/i.test(batch.measurements.adapter))
              throw Error("Hardware required");
            batches.push(batch);
          }
          pair.set(vegetation, batches);
        }
        const present = pair.get(true)?.flatMap((b) => b.gpu.map((g) => g.gpuMs)) ?? [];
        const absent = pair.get(false)?.flatMap((b) => b.gpu.map((g) => g.gpuMs)) ?? [];
        if (present.length !== frames || absent.length !== frames) throw Error("Missing trial samples");
        const result = {
          trial,
          withVegetation: quantiles(present),
          withoutVegetation: quantiles(absent),
          pairedDifference: signedQuantiles(present.map((v, i) => v - absent[i])),
          batches: [...pair].map(([vegetation, batches]) => ({ vegetation, batches })),
        };
        results.push(result);
        console.log(
          JSON.stringify({
            trial,
            present: result.withVegetation,
            absent: result.withoutVegetation,
            marginal: result.pairedDifference,
          }),
        );
        await Bun.write(
          join(output, "marginal.json"),
          JSON.stringify(
            {
              version: 1,
              settings: {
                width: 1920,
                height: 1080,
                antialiasing: "temporal",
                finiteSun: false,
                frames,
                queue: 4,
              },
              scope:
                "Alternating controlled marginal GPU estimate at matching simulation ticks; interaction-sensitive, not an additive pass decomposition. CPU host still extracts the same scene in both variants. Short diagnostic, not sustained acceptance.",
              results,
              errors,
            },
            null,
            2,
          ),
        );
      }
      console.log(output);
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  1920,
  1080,
  "chrome",
  Math.max(180000, frames * 200),
);
