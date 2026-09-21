import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";

const source = `import { createExperiment } from ${JSON.stringify(resolve("tools/field-research/gpu-fixture.ts"))};
createExperiment(document.querySelector('canvas')).then(f => {window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`;
await mkdir("output/field-research", { recursive: true });
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(source, output);
    try {
      await view.navigate(server.url.toString());
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw new Error(String(failure));
      const adapter = await view.evaluate("fixture.adapter"),
        resolution = await view.evaluate("fixture.resolution");
      const runs = [];
      for (const count of [1, 64, 1024, 4096]) {
        const run = await view.evaluate(`fixture.benchmark(${count})`);
        runs.push(run);
        console.log(JSON.stringify(run));
      }
      const accuracy: Record<string, unknown> = {};
      for (const mode of ["meshLod", "mesh12", "mesh24", "analytic"]) {
        accuracy[mode] = await view.evaluate(`fixture.accuracy('${mode}')`);
        await view.evaluate(`fixture.show('${mode}')`);
        await Bun.write(`output/field-research/${mode}.png`, await view.screenshot());
        console.log(JSON.stringify({ mode, accuracy: accuracy[mode] }));
      }
      await view.evaluate("fixture.show('analytic',1024)");
      await Bun.write("output/field-research/analytic-1024.png", await view.screenshot());
      const gpuErrors = await view.evaluate<string[]>("fixture.errors");
      const report = {
        created: new Date().toISOString(),
        adapter,
        resolution,
        methodology:
          "Production marching-tetrahedra stone meshes at resolutions 24 and 12, plus a strong conventional 120-triangle parametric ellipsoid LOD, vs exact ellipsoid over a 12-triangle conservative box. Identical one-draw instancing, lighting, camera, viewport, backface culling and depth. 9 alternating GPU timestamp samples of 4 passes each, after 3 warmups per path. Geometry bytes exclude shared uniforms (176 bytes) and instances (32 bytes each) used by all paths; analytic proxy vertices come from vertex IDs. No terrain, shadows, skinning, procedural material, streaming or CPU submission cost; not full-world frame time. Accuracy independently compared against double-precision ray/ellipsoid ground truth at every pixel of the single-stone view.",
        runs,
        accuracy,
        errors: [...errors, ...gpuErrors],
      };
      await Bun.write("output/field-research/gpu.json", JSON.stringify(report, null, 2));
      if (report.errors.length) throw new Error(report.errors.join("\n"));
    } finally {
      server.stop(true);
    }
  },
  1920,
  1080,
);
