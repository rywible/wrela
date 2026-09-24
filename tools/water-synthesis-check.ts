import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {benchmarkWaterSynthesis} from ${JSON.stringify(resolve("tools/fixtures/water-synthesis-benchmark.ts"))};window.run=benchmarkWaterSynthesis;window.ready=true;`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready", 60000);
      const result = await view.evaluate("run()");
      await Bun.write(join(output, "synthesis.json"), JSON.stringify(result, null, 2));
      if (errors.length) throw Error(errors.join("\n"));
      console.log(JSON.stringify({ output, result }));
    } finally {
      server.stop(true);
    }
  },
  640,
  384,
  "chrome",
  600000,
  "measurement",
);
