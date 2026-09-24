import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createSkyFieldFixture } from "./fixtures/sky-field";

type Result = Awaited<ReturnType<Awaited<ReturnType<typeof createSkyFieldFixture>>["temporalCheck"]>>;
const selected = process.argv
  .find((arg) => arg.startsWith("--cases="))
  ?.slice(8)
  .split(",");
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createSkyFieldFixture} from ${JSON.stringify(resolve("tools/fixtures/sky-field.ts"))};createSkyFieldFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      await view.evaluate(
        `(fixture.temporalCheck(${JSON.stringify(selected)}).then(result=>window.temporalResult=result).catch(error=>window.temporalFailure=String(error)),true)`,
      );
      await waitFor(view, "window.temporalResult||window.temporalFailure", 240000);
      const temporalFailure = await view.evaluate("window.temporalFailure");
      if (temporalFailure) throw Error(String(temporalFailure));
      const { images, ...result } = await view.evaluate<Result>("window.temporalResult");
      for (const [name, data] of Object.entries(images))
        await Bun.write(join(output, `${name}.png`), Buffer.from(data.split(",")[1], "base64"));
      await Bun.write(
        join(output, "sky-temporal.json"),
        JSON.stringify({ ...result, browserErrors: errors }, null, 2),
      );
      console.log(JSON.stringify({ output, ...result }));
      if (errors.length || result.errors.length) throw Error("Cloud reprojection GPU validation failed");
      if (result.cases.some((c) => c.steps.some((s) => !Number.isFinite(s.error.rms) || s.error.rms > 0.004)))
        throw Error("Cloud reprojection exceeded image error gate");
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  512,
  384,
);
