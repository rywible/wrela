import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {gpuFrontiers} from ${JSON.stringify(resolve("tools/compiler-frontiers/gpu-fixture.ts"))};gpuFrontiers().then(r=>window.result=r).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.result || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const result = await view.evaluate("window.result");
      await Bun.write(join(output, "frontiers-gpu.json"), JSON.stringify(result, null, 2));
      await Bun.write(
        resolve("output/compiler-frontiers/gpu.json"),
        JSON.stringify({ output, result, errors }, null, 2),
      );
      console.log(JSON.stringify({ output, result, errors }, null, 2));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      server.stop(true);
    }
  },
  320,
  180,
);
