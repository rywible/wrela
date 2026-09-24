import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createSurfaceLightingFixture} from ${JSON.stringify(resolve("tools/fixtures/surface-lighting.ts"))};createSurfaceLightingFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const reports: unknown[] = [];
      for (const kind of ["matte", "metal", "sealed"]) {
        const result = await view.evaluate<{ images: Record<string, string>; report: { errors: string[] } }>(
          `fixture.capture(${JSON.stringify(kind)},${process.argv.includes("--large") ? "1280,960" : "640,480"})`,
        );
        for (const [name, url] of Object.entries(result.images))
          await Bun.write(join(output, `${kind}-${name}.png`), Buffer.from(url.split(",")[1], "base64"));
        reports.push(result.report);
        console.log(JSON.stringify({ kind, report: result.report }));
      }
      await Bun.write(
        join(output, "surface-lighting.json"),
        JSON.stringify({ reports, browserErrors: errors }, null, 2),
      );
      console.log(JSON.stringify({ output, browserErrors: errors }));
      if (errors.length || reports.some((r) => (r as { errors: string[] }).errors.length))
        throw Error("Surface lighting prototype failed acceptance; see saved report");
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  640,
  480,
);
