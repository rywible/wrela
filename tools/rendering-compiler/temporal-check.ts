import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {checkTemporalGpu} from ${JSON.stringify(resolve("tools/rendering-compiler/temporal-fixture.ts"))};import {checkIrradianceGpu} from ${JSON.stringify(resolve("tools/rendering-compiler/irradiance-fixture.ts"))};import {checkShadowDomainGpu} from ${JSON.stringify(resolve("tools/rendering-compiler/shadow-domain-fixture.ts"))};Promise.all([checkTemporalGpu(),checkTemporalGpu("raster"),checkTemporalGpu("storage"),checkIrradianceGpu(),checkShadowDomainGpu()]).then(r=>window.result=r).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.result || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const result = await view.evaluate("window.result");
      await Bun.write(join(output, "temporal.json"), JSON.stringify(result, null, 2));
      console.log(JSON.stringify({ output, result, errors }));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      server.stop(true);
    }
  },
  320,
  180,
);
