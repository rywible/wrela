import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";
import type { AcceptanceFixture } from "./fixture";
import { SCALABILITY_WORKLOADS } from "./scalability";
import { quantiles, trialOrder } from "./timing";

const full = process.argv.includes("--full"),
  resolution = full ? [1920, 1080] : [640, 360];
const saturated = process.argv.includes("--saturated");
const frames = 91;
const burst = Number(process.argv.find((arg) => arg.startsWith("--burst="))?.slice(8) ?? 1);
const selected = process.argv
  .find((a) => a.startsWith("--workloads="))
  ?.slice(12)
  .split(",");
const workloads = SCALABILITY_WORKLOADS.filter((w) => !selected || selected.includes(w.name));
if (!workloads.length || selected?.some((name) => !workloads.some((w) => w.name === name)))
  throw Error("Unknown workload");
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createAcceptanceFixture} from ${JSON.stringify(resolve("tools/rendering-compiler/fixture.ts"))};createAcceptanceFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const results = [];
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const preparation = await view.evaluate(`fixture.prepare('winter-valley',${frames})`);
      for (let trial = 0; trial < 3; trial++)
        for (const name of trialOrder(
          workloads.map((w) => w.name),
          trial,
        )) {
          const workload = workloads.find((w) => w.name === name);
          if (!workload) throw Error("Missing workload");
          const inventory = await view.evaluate(`fixture.configureWorkload(${JSON.stringify(workload)})`);
          await view.evaluate("fixture.beginVariant('production')");
          await view.evaluate(`fixture.run(true,${frames},false,${!saturated},${burst})`);
          const run = await view.evaluate<Awaited<ReturnType<AcceptanceFixture["run"]>>>(
            `fixture.run(false,${frames},${saturated},${!saturated},${burst})`,
          );
          if (run.diagnostics.some((d) => d.severity === "error") || run.gpu.length !== frames) {
            await Bun.write(
              join(output, "failed-workload.json"),
              JSON.stringify({ name, trial, ...run }, null, 2),
            );
            throw Error(
              `Invalid or missing GPU samples: ${name}, ${run.gpu.length}/${frames}; ${JSON.stringify(run.diagnostics)}`,
            );
          }
          const summary = {
            trial,
            name,
            inventory,
            gpu: quantiles(run.gpu.map((s) => s.gpuMs)),
            cpu: quantiles(run.cpu),
            pacing: quantiles(run.pacing),
            measurements: run.measurements,
            completedFramesPerSecond: (frames * 1000) / run.elapsedMs,
          };
          console.log(JSON.stringify(summary));
          results.push({ ...summary, ...run });
          if (trial === 0) await Bun.write(join(output, `${name}.png`), await view.screenshot());
          await Bun.write(
            join(output, "scalability.json"),
            JSON.stringify(
              {
                resolution,
                saturated,
                burst,
                preparation,
                results,
                errors,
                limitations: [
                  "Pre-evaluated animation packets: excludes simulation, AI, streaming and compilation.",
                  "Debris is opaque instanced geometry; transparent particle overdraw is not implemented by this renderer.",
                  "Point-light range is bounded by the current renderer's eight-light contract.",
                  "Overlapping pass timestamps are not exclusive costs.",
                ],
              },
              null,
              2,
            ),
          );
        }
      await view.evaluate("fixture.configureWorkload({name:'baseline'})");
      await view.evaluate("fixture.beginVariant('production')");
      const live = await view.evaluate<Awaited<ReturnType<AcceptanceFixture["runLive"]>>>(
        `fixture.runLive(${frames})`,
      );
      if (live.gpu.length !== frames || live.diagnostics.some((d) => d.severity === "error"))
        throw Error("Invalid live-loop measurement");
      const liveSummary = {
        cpu: quantiles(live.cpu),
        simulation: quantiles(live.simulation),
        extraction: quantiles(live.extraction),
        submission: quantiles(live.submission),
        gpu: quantiles(live.gpu.map((s) => s.gpuMs)),
        pacing: quantiles(live.pacing),
      };
      await Bun.write(
        join(output, "live-loop.json"),
        JSON.stringify(
          {
            ...live,
            summary: liveSummary,
            limitation:
              "Current authored simulation plus extraction and rendering; excludes application UI, network traffic and unimplemented gameplay.",
          },
          null,
          2,
        ),
      );
      console.log(JSON.stringify({ live: liveSummary }));
      console.log(JSON.stringify({ output, errors }));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  resolution[0],
  resolution[1],
);
