import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";
import type { AcceptanceFixture } from "./fixture";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createAcceptanceFixture} from ${JSON.stringify(resolve("tools/rendering-compiler/fixture.ts"))};createAcceptanceFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      await view.evaluate("fixture.prepare('water',2)");
      await view.evaluate("fixture.beginVariant('production')");
      const reference =
        await view.evaluate<Awaited<ReturnType<AcceptanceFixture["waterReference"]>>>(
          "fixture.waterReference()",
        );
      let squared = 0,
        energy = 0;
      for (const sample of reference.cases)
        for (let i = 0; i < 3; i++) {
          squared += (sample.actual[i] - sample.reference[i]) ** 2;
          energy += sample.reference[i] ** 2;
        }
      const compiledCases = reference.cases.filter((sample) => sample.path === 4);
      if (compiledCases.length < 5) throw Error("Compiled glint path was not exercised");
      const relativeRms = Math.sqrt(squared / Math.max(energy, reference.cases.length * 3 * 0.01 ** 2));
      await Bun.write(
        join(output, "water-independent-reference.json"),
        JSON.stringify({ ...reference, relativeRms }, null, 2),
      );
      console.log(
        JSON.stringify({
          output,
          compiledCases: compiledCases.length,
          relativeRms,
          maximumRelativeError: reference.maximumRelativeError,
          maximumReferenceConvergence: reference.maximumReferenceConvergence,
          errors,
        }),
      );
      if (
        errors.length ||
        relativeRms > 0.01 ||
        reference.maximumRelativeError > 0.05 ||
        reference.maximumReferenceConvergence > 1e-6
      )
        throw Error("Independent production water quality gate failed");
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  640,
  360,
);
