import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";
import type { AcceptanceFixture } from "./fixture";
import { quantiles } from "./timing";

type Run = Awaited<ReturnType<AcceptanceFixture["run"]>>;
const full = process.argv.includes("--full");
const smoke = process.argv.includes("--smoke");
const resolution: [number, number] = smoke ? [320, 180] : full ? [1920, 1080] : [640, 360];
const frames = smoke ? 2 : full ? 24 : 8;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createAcceptanceFixture} from ${JSON.stringify(resolve("tools/rendering-compiler/fixture.ts"))};createAcceptanceFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      await view.evaluate(`fixture.prepare('winter-valley',${frames})`);
      const results = [];
      for (const [name, overrides] of [
        ["production", {}],
        ["water-direct", { water: "direct" }],
        ["water-regular", { water: "regular" }],
        ["visibility-off", { visibility: false }],
      ] as const) {
        if (smoke && name !== "production") continue;
        console.log(JSON.stringify({ stage: "initializing", name, resolution, frames }));
        await view.evaluate(`fixture.beginVariant("production",${JSON.stringify(overrides)})`);
        await view.evaluate("fixture.run(true)");
        const run = await view.evaluate<Run>("fixture.run(false)");
        await Bun.write(join(output, `${name}-raw.json`), JSON.stringify(run, null, 2));
        if (run.diagnostics.some((d) => d.severity === "error"))
          throw new Error(JSON.stringify(run.diagnostics));
        if (!run.gpu.length || run.gpu.some((sample) => sample.gpuMs <= 0))
          throw new Error("Missing or invalid GPU timestamps");
        const passes = Object.fromEntries(
          (
            ["gpuMs", "atmosphereMs", "shadowMs", "sceneMs", "waterMs", "temporalMs", "displayMs"] as const
          ).flatMap((key) => {
            const values = run.gpu
              .map((sample) => sample[key])
              .filter((value): value is number => value !== undefined && value >= 0);
            return values.length ? [[key, quantiles(values)]] : [];
          }),
        );
        const result = { name, overrides, cpuSummary: quantiles(run.cpu), passes, ...run };
        results.push(result);
        await Bun.write(join(output, `${name}.json`), JSON.stringify(result, null, 2));
        console.log(JSON.stringify({ name, cpu: quantiles(run.cpu), passes }));
        if (smoke) {
          await view.evaluate("fixture.capture(0)");
          await Bun.write(join(output, "smoke.png"), await view.screenshot());
        }
      }
      await Bun.write(
        join(output, "profile.json"),
        JSON.stringify(
          {
            resolution,
            purpose: "Diagnostic pass attribution; controls are not quality references",
            results,
            errors,
          },
          null,
          2,
        ),
      );
      console.log(JSON.stringify({ output, errors }));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  resolution[0],
  resolution[1],
);
