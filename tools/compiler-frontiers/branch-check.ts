import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";

const input = await Bun.file("output/compiler-frontiers/branch-spatial/gpu-input.json").json();
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {gpuFrontiers} from ${JSON.stringify(resolve("tools/compiler-frontiers/gpu-fixture.ts"))};gpuFrontiers(${JSON.stringify(input)}).then(r=>window.result=r).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.result || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const result = { output, result: await view.evaluate("window.result"), errors };
      await Bun.write(join(output, "branch.json"), JSON.stringify(result, null, 2));
      await Bun.write("output/compiler-frontiers/branch-spatial/gpu.json", JSON.stringify(result, null, 2));
      console.log(JSON.stringify(result, null, 2));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      server.stop(true);
    }
  },
  320,
  180,
);
