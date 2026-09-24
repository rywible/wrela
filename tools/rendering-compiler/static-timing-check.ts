import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";
import type { AcceptanceFixture } from "./fixture";
import { quantiles } from "./timing";

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
      await view.evaluate("fixture.prepare('winter-valley',91)");
      await view.evaluate("fixture.freezeFrame()");
      await view.evaluate("fixture.beginVariant('production')");
      await view.evaluate("fixture.run(true)");
      const stationary = await view.evaluate<Awaited<ReturnType<AcceptanceFixture["run"]>>>("fixture.run()");
      if (
        stationary.gpu.length !== 91 ||
        stationary.gpu.some((s) => s.atmosphereMs !== 0 || s.intervals?.some((i) => i.pass === "atmosphere"))
      )
        throw Error("Cached atmosphere leaked into stationary GPU timings");
      const live =
        await view.evaluate<Awaited<ReturnType<AcceptanceFixture["runLive"]>>>("fixture.runLive()");
      if (live.gpu.length !== 91) throw Error("Missing live timings");
      const result = {
        stationary,
        live,
        summary: {
          stationaryGpu: quantiles(stationary.gpu.map((s) => s.gpuMs)),
          liveGpu: quantiles(live.gpu.map((s) => s.gpuMs)),
          liveCpu: quantiles(live.cpu),
          pacing: quantiles(live.pacing),
        },
        errors,
      };
      await Bun.write(join(output, "static-timing.json"), JSON.stringify(result, null, 2));
      console.log(JSON.stringify({ output, summary: result.summary, errors }));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  1920,
  1080,
);
