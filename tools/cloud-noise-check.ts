import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createCloudNoiseFixture } from "./fixtures/cloud-noise";

type Fixture = Awaited<ReturnType<typeof createCloudNoiseFixture>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createCloudNoiseFixture} from ${JSON.stringify(resolve("tools/fixtures/cloud-noise.ts"))};createCloudNoiseFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const result = await view.evaluate<Awaited<ReturnType<Fixture["check"]>>>("fixture.check()");
      await Bun.write(join(output, "cloud-noise-check.json"), JSON.stringify({ ...result, errors }, null, 2));
      if (errors.length || !result.passed) throw Error([...result.failures, ...errors].join("\n"));
      console.log(JSON.stringify({ output, ...result }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  256,
  128,
);
