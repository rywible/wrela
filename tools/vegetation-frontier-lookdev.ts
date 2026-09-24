import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createVegetationFrontier } from "./fixtures/vegetation-frontier";
import { vegetationStudies } from "./fixtures/vegetation-study";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--snapshot-run")) {
  const destination = resolve("output/vegetation-frontier", `${Date.now()}-${process.pid}`, "source");
  await snapshotSource(destination, "vegetation-frontier");
  const child = Bun.spawn(
    [process.execPath, "tools/vegetation-frontier-lookdev.ts", ...process.argv.slice(2), "--snapshot-run"],
    { cwd: destination, stdout: "inherit", stderr: "inherit" },
  );
  console.log(JSON.stringify({ snapshot: destination }));
  process.exit(await child.exited);
}
const study = vegetationStudies({
  development: process.argv.includes("--birch") ? "paper-birch" : undefined,
})[0];
type Result = Awaited<ReturnType<Awaited<ReturnType<typeof createVegetationFrontier>>["compare"]>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createVegetationFrontier} from ${JSON.stringify(resolve("tools/fixtures/vegetation-frontier.ts"))};createVegetationFrontier(${JSON.stringify(study)},${JSON.stringify({ depthGrid: process.argv.includes("--depth") ? 16 : 0 })}).then(f=>{window.fixture=f;window.ready=true;}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 120000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const results: Result[] = [];
      for (const pixels of [8, 16, 24])
        for (const [azimuth, elevation] of [
          [0.37, 0.1],
          [1.8, 0.35],
          [3.2, -0.05],
          [4.9, 0.7],
        ]) {
          const result = await view.evaluate<Result>(`fixture.compare(${pixels},${azimuth},${elevation})`);
          if (/software|swiftshader|llvmpipe/i.test(result.adapter)) throw Error("Hardware required");
          results.push(result);
          console.log(
            JSON.stringify({
              pixels,
              azimuth,
              candidate: result.candidate,
              convergence: result.referenceConvergence,
            }),
          );
        }
      await Bun.write(
        join(output, "frontier.json"),
        JSON.stringify(
          {
            version: 1,
            scope:
              "Static expected silhouette of the compiled source mesh, using actual 32x and 64x per-axis supersampled production rendering; excludes radiance, shadow energy, motion and exact 3D needle-source qualification. Candidate status is never promoted by this test alone.",
            results,
            errors,
          },
          null,
          2,
        ),
      );
      console.log(output);
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  256,
  256,
);
