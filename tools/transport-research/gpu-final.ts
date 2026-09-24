import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";

await mkdir("output/transport-research", { recursive: true });
const source = `import { createExperiment } from ${JSON.stringify(resolve("tools/transport-research/gpu-final-fixture.ts"))};
  createExperiment().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`;
await withBrowser(async (view, output, errors) => {
  const server = await fixtureServer(source, output);
  try {
    await view.navigate(server.url.toString());
    await waitFor(view, "window.ready || window.failure", 60000);
    const failure = await view.evaluate("window.failure");
    if (failure) throw new Error(String(failure));
    const prior = process.argv.includes("--sun")
      ? await Bun.file("output/transport-research/final-gpu.json").json()
      : null;
    const adapter = await view.evaluate("fixture.adapter"),
      coherent = prior?.coherent ?? [];
    for (const roughness of prior ? [] : [0.04, 0.06, 0.18]) {
      const result = await view.evaluate<Record<string, unknown>>(`fixture.coherent(${roughness})`);
      await Bun.write(`output/transport-research/coherent-gpu-${roughness}.json`, JSON.stringify(result));
      const { images: _, ...summary } = result;
      coherent.push(summary);
      console.log(JSON.stringify({ coherent: summary }));
    }
    const sun = await view.evaluate<Record<string, unknown>>("fixture.sun()");
    await Bun.write("output/transport-research/sun-gpu.json", JSON.stringify(sun));
    const { images: _, ...sunSummary } = sun;
    console.log(JSON.stringify({ sun: sunSummary }));
    const gpuErrors = await view.evaluate<string[]>("fixture.errors");
    const result = {
      created: new Date().toISOString(),
      adapter,
      methodology:
        "Hardware WebGPU; 9 alternating timing trials, each 64 dispatches, after 3 warmups. Includes per-query moving-light/view preparation and five Newton updates in pole modes. Excludes driver compilation, CPU submission and readback. Synthetic complete coherent phase orbits, not whole-game frames. Accuracy checked over 32 additional RNG seeds.",
      coherent,
      sun: sunSummary,
      errors: [...errors, ...gpuErrors],
    };
    await Bun.write("output/transport-research/final-gpu.json", JSON.stringify(result, null, 2));
    if (result.errors.length) throw new Error(result.errors.join("\n"));
  } finally {
    server.stop(true);
  }
});
