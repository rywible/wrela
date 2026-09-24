import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createGILookdevFixture } from "./fixtures/gi-lookdev";

type Fixture = Awaited<ReturnType<typeof createGILookdevFixture>>;

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createGILookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/gi-lookdev.ts"))};createGILookdevFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const captures = [];
      for (const name of ["box", "alpine"] as const) {
        const result = await view.evaluate<Awaited<ReturnType<Fixture["capture"]>>>(
          `fixture.capture(${JSON.stringify(name)})`,
        );
        for (const [suffix, image] of [
          ["off", result.off],
          ["on", result.on],
          ["indirect-cache", result.cacheImage],
          ["indirect-reference", result.referenceImage],
        ])
          await Bun.write(
            join(output, `gi-${name}-${suffix}.png`),
            Buffer.from(image.split(",")[1], "base64"),
          );
        await Bun.write(join(output, `gi-${name}-reference.pfm`), Buffer.from(result.referencePFM, "base64"));
        await Bun.write(join(output, `gi-${name}-cache.pfm`), Buffer.from(result.cachePFM, "base64"));
        captures.push(result.metadata);
      }
      const checks = await view.evaluate<Awaited<ReturnType<Fixture["check"]>>>("fixture.check()");
      await Bun.write(
        join(output, "gi-lookdev.json"),
        JSON.stringify(
          {
            captures,
            checks,
            errors,
            visualApproval:
              "Requires image review; this is a bounded one-bounce prototype, not an AAA or full-GI claim.",
          },
          null,
          2,
        ),
      );
      if (errors.length || checks.errors.length) throw Error([...errors, ...checks.errors].join("\n"));
      console.log(
        JSON.stringify({
          output,
          captures: captures.map((c) => ({ comparison: c.comparison, buildMs: c.buildMs })),
          checks,
        }),
      );
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  640,
  480,
);
