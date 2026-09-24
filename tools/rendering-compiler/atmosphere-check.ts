import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";

const source = `import {verifyAtmosphereEdges} from ${JSON.stringify(resolve("tools/rendering-compiler/atmosphere-fixture.ts"))};
verifyAtmosphereEdges().then(result=>window.result=result).catch(error=>window.failure=String(error));`;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(source, output);
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.result || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const result = await view.evaluate("window.result");
      await Bun.write(join(output, "atmosphere-edges.json"), JSON.stringify(result, null, 2));
      if (errors.length) throw Error(errors.join("\n"));
      console.log(JSON.stringify({ output, result }, null, 2));
    } finally {
      server.stop(true);
    }
  },
  320,
  180,
);
