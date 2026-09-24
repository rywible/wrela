import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createSkyFieldFixture } from "./fixtures/sky-field";

type Result = Awaited<ReturnType<Awaited<ReturnType<typeof createSkyFieldFixture>>["check"]>>;
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
        `(fixture.check(${JSON.stringify(selected)}).then(result=>window.fieldResult=result).catch(error=>window.fieldFailure=String(error)),true)`,
      );
      await waitFor(view, "window.fieldResult||window.fieldFailure", 240000);
      const fieldFailure = await view.evaluate("window.fieldFailure");
      if (fieldFailure) throw Error(String(fieldFailure));
      const { images, ...result } = await view.evaluate<Result>("window.fieldResult");
      for (const [name, data] of Object.entries(images))
        await Bun.write(join(output, `${name}.png`), Buffer.from(data.split(",")[1], "base64"));
      await Bun.write(
        join(output, "sky-field.json"),
        JSON.stringify({ ...result, browserErrors: errors }, null, 2),
      );
      console.log(JSON.stringify({ output, ...result }));
      // Empirical gates for these fixed cases, not analytic transport bounds.
      if (
        result.cases.some(
          (c) =>
            c.referenceConvergence.rms > 0.001 ||
            c.cacheOnlyError.rms > 0.002 ||
            c.errors.adaptive384.rms > 0.004 ||
            (c.name !== "clear" &&
              !c.name.startsWith("layers") &&
              c.errors.adaptive384.rms >= c.errors.scrambled32.rms),
        )
      )
        throw Error("Sky field exceeded measured image-error gates");
      if (errors.length || result.errors.length || result.cases.some((c) => !c.finite))
        throw Error("Sky field GPU validation failed");
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  512,
  384,
);
