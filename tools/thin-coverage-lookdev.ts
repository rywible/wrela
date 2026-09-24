import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createThinCoverageFixture } from "./fixtures/thin-coverage";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--snapshot-run")) {
  const destination = resolve("output/thin-coverage", `${Date.now()}-${process.pid}`, "source");
  await snapshotSource(destination, "thin-coverage");
  console.log(JSON.stringify({ snapshot: destination }));
  process.exit(
    await Bun.spawn([process.execPath, "tools/thin-coverage-lookdev.ts", "--snapshot-run"], {
      cwd: destination,
      stdout: "inherit",
      stderr: "inherit",
    }).exited,
  );
}

type Fixture = Awaited<ReturnType<typeof createThinCoverageFixture>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createThinCoverageFixture} from ${JSON.stringify(resolve("tools/fixtures/thin-coverage.ts"))};createThinCoverageFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const captures = [];
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      for (const state of ["still", "wind", "shadow"])
        for (const width of state === "still" ? [0.1, 0.25, 0.5, 1, 2, 4] : [0.5, 2]) {
          const { image, ...capture } = await view.evaluate<Awaited<ReturnType<Fixture["capture"]>>>(
            `fixture.capture(${width},${JSON.stringify(state)})`,
          );
          if (/swiftshader|llvmpipe|software/i.test(capture.adapter))
            throw Error("Hardware adapter required");
          await Bun.write(
            join(output, `coverage-${state}-${width}.png`),
            Buffer.from(image.split(",")[1], "base64"),
          );
          captures.push(capture);
        }
      for (const layers of [8, 32])
        for (const width of [0.1, 0.5, 2]) {
          const { image, ...capture } = await view.evaluate<Awaited<ReturnType<Fixture["capture"]>>>(
            `fixture.capture(${width},"still",${layers})`,
          );
          await Bun.write(
            join(output, `coverage-layers${layers}-${width}.png`),
            Buffer.from(image.split(",")[1], "base64"),
          );
          captures.push(capture);
        }
      const coarse = await view.evaluate<Awaited<ReturnType<Fixture["coarse"]>>>("fixture.coarse()");
      await Bun.write(
        join(output, "thin-coverage-lookdev.json"),
        JSON.stringify(
          {
            captures,
            coarse,
            errors,
            acceptance:
              "Coverage bias and single-frame stochastic error require review alongside actual plant/stand production captures; these are not AAA art approval or a performance benchmark.",
          },
          null,
          2,
        ),
      );
      if (errors.length) throw Error(errors.join("\n"));
      console.log(JSON.stringify({ output, captures: captures.length }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  768,
  384,
);
