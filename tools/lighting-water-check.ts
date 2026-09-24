import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createLightingWaterFixture} from ${JSON.stringify(resolve("tools/fixtures/lighting-water.ts"))};createLightingWaterFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const report = await view.evaluate<{ errors: string[] }>("fixture.check()");
      await Bun.write(
        join(output, "lighting-water.json"),
        JSON.stringify({ ...report, browserErrors: errors }, null, 2),
      );
      if (errors.length || report.errors.length) throw Error([...errors, ...report.errors].join(";"));
      console.log(JSON.stringify({ output, report }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  320,
  240,
);
