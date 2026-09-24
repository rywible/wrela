import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createLightingUpgradeFixture } from "./fixtures/lighting-upgrade";

type Fixture = Awaited<ReturnType<typeof createLightingUpgradeFixture>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createLightingUpgradeFixture} from ${JSON.stringify(resolve("tools/fixtures/lighting-upgrade.ts"))};createLightingUpgradeFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const numerical = await view.evaluate<Awaited<ReturnType<Fixture["check"]>>>("fixture.check()");
      const captures = [];
      for (const name of ["physical-gi", "lantern", "winter"] as const) {
        const result = await view.evaluate<Awaited<ReturnType<Fixture["capture"]>>>(
          `fixture.capture(${JSON.stringify(name)})`,
        );
        await Bun.write(join(output, `${name}.png`), Buffer.from(result.image, "base64"));
        if ("unshadowed" in result)
          await Bun.write(join(output, `${name}-unshadowed.png`), Buffer.from(result.unshadowed, "base64"));
        const { image: _image, ...metadata } = result;
        captures.push({ name, ...metadata, unshadowed: undefined });
      }
      const report = { numerical, captures, errors };
      await Bun.write(join(output, "lighting-upgrade.json"), JSON.stringify(report, null, 2));
      if (errors.length) throw Error(errors.join(";"));
      console.log(JSON.stringify({ output, report }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  640,
  480,
);
