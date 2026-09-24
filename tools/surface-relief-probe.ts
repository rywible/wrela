import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {surfaceReliefProbeFixture} from ${JSON.stringify(resolve("tools/fixtures/surface-relief-probe.ts"))};surfaceReliefProbeFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 30000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw new Error(String(failure));
      const report = await view.evaluate<{ samples: number; failures: number; errors: string[] }>(
        "fixture.check()",
      );
      await Bun.write(join(output, "surface-relief-parity.json"), JSON.stringify(report, null, 2));
      console.log(JSON.stringify({ output, ...report }));
      if (errors.length || report.errors.length || report.failures || report.samples < 100)
        throw new Error(`Relief GPU parity failed: ${JSON.stringify({ errors, report })}`);
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  64,
  64,
);
