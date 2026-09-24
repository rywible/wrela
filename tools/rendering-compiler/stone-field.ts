import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";
import type { StoneFieldFixture, StoneVariant } from "./stone-field-fixture";
import { quantiles, summarizeAlternatingTrials, summarizeMatchedTrial, trialOrder } from "./timing";

// Deliberately safe default. Larger workloads require an explicit command.
const full = process.argv.includes("--full");
const medium = process.argv.includes("--medium");
const count = full ? 4096 : medium ? 256 : 64;
const resolution = full ? [1920, 1080] : medium ? [640, 360] : [320, 180];
const frames = full ? 32 : medium ? 17 : 1;
const trials = full ? 5 : medium ? 3 : 1;
const variants: StoneVariant[] = ["analytic", "parametric", "auto"];
type Run = Awaited<ReturnType<StoneFieldFixture["run"]>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createStoneFieldFixture} from ${JSON.stringify(resolve("tools/rendering-compiler/stone-field-fixture.ts"))};createStoneFieldFixture(${count}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const preparation = await view.evaluate("fixture.preparation");
      const captures = [];
      const runs = [];
      const matched = [];
      for (let trial = 0; trial < trials; trial++) {
        const observations: Run[] = [];
        // The first trial establishes analytic image references. Later trials alternate.
        for (const variant of trialOrder(variants, trial)) {
          console.log(JSON.stringify({ stage: "stone-field", trial, variant, count, resolution, frames }));
          const initialization = await view.evaluate(`fixture.begin(${JSON.stringify(variant)})`);
          await view.evaluate(`fixture.run(${Math.min(frames, 8)})`);
          const run = await view.evaluate<Run>(`fixture.run(${frames})`);
          observations.push(run);
          runs.push({ trial, initialization, ...run, cpuSummary: quantiles(run.cpu) });
          await Bun.write(
            join(output, "stone-field-partial.json"),
            JSON.stringify({ preparation, runs }, null, 2),
          );
          if (trial === 0)
            for (const mode of ["beauty", "depth", "identity"] as const) {
              captures.push(await view.evaluate(`fixture.capture(${JSON.stringify(mode)})`));
              await Bun.write(join(output, `${variant}-${mode}.png`), await view.screenshot());
            }
        }
        if (frames >= 17) matched.push(summarizeMatchedTrial(observations));
      }
      const report = {
        version: "production-stone-field-1",
        count,
        resolution,
        frames,
        trials,
        sampleCount: 4,
        geometryErrorBudgetPixels: 0.25,
        preparation,
        runs,
        captures,
        matched,
        summary: matched.length >= 3 ? summarizeAlternatingTrials(matched) : null,
        errors,
        interpretation:
          "Whole production-frame timings, static compiled stone instances, common conservative 0.25 pixel geometric error budget. CPU timings cover prepared-scene submission. Beauty/depth differences are diagnostic; finite output and exact identity accounting are hard gates. Parametric silhouette/depth pixels need not equal the analytic reference at the allowed geometric error. No water or occlusion culling.",
      };
      await Bun.write(join(output, "stone-field.json"), JSON.stringify(report, null, 2));
      console.log(JSON.stringify({ output, summary: report.summary, errors }));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  resolution[0],
  resolution[1],
);
