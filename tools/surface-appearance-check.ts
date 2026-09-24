import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {surfaceAppearanceFixture} from ${JSON.stringify(resolve("tools/fixtures/surface-appearance.ts"))};surfaceAppearanceFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 30000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const report = await view.evaluate<{
        results: Record<string, number>;
        invariants: Record<string, number>;
        errors: string[];
        complete: boolean;
        adapter: string;
      }>("fixture.check()");
      await Bun.write(join(output, "surface-appearance.json"), JSON.stringify(report, null, 2));
      if (errors.length || report.errors.length) throw Error([...errors, ...report.errors].join("\n"));
      if (!report.complete) throw Error("Surface review omitted requested content");
      for (const [name, delta] of Object.entries(report.results))
        if (!Number.isFinite(delta) || delta < 0.00001)
          throw Error(`${name} did not visibly affect rendered pixels: ${delta}`);
      for (const [name, delta] of Object.entries(report.invariants))
        if (!Number.isFinite(delta) || delta > 0.00001)
          throw Error(`${name} did not preserve the opaque coating response: ${delta}`);
      console.log(JSON.stringify({ output, ...report }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  320,
  240,
);
