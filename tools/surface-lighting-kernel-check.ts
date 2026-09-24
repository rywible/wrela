import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";

const baseline = process.argv.find((v) => v.startsWith("--baseline="))?.slice(11);
if (!baseline) throw Error("Supply --baseline=<frozen source directory>");
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {WebGPURenderer as Previous} from ${JSON.stringify(resolve(baseline, "packages/render-webgpu/src/index.ts"))};import {surfaceLightingKernelControl} from ${JSON.stringify(resolve("tools/fixtures/surface-lighting-kernel-control.ts"))};surfaceLightingKernelControl(Previous).then(report=>window.report=report).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.report||window.failure", 120000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const report = await view.evaluate<{ errors: string[] }>("window.report");
      await Bun.write(
        join(output, "surface-kernel-control.json"),
        JSON.stringify({ baseline, report, browserErrors: errors }, null, 2),
      );
      console.log(JSON.stringify({ output, errors: [...errors, ...report.errors] }));
      if (errors.length || report.errors.length) throw Error("Renderer control failed");
    } finally {
      server.stop(true);
    }
  },
  640,
  480,
);
