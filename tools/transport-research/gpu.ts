import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";

await mkdir("output/transport-research", { recursive: true });
const prior = process.argv.includes("--visibility")
  ? await Bun.file("output/transport-research/gpu.json").json()
  : null;
const source = `import { createExperiment } from ${JSON.stringify(resolve("tools/transport-research/gpu-fixture.ts"))};
  createExperiment().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`;
await withBrowser(async (view, output, errors) => {
  const server = await fixtureServer(source, output);
  try {
    await view.navigate(server.url.toString());
    await waitFor(view, "window.ready || window.failure", 60000);
    const failure = await view.evaluate("window.failure");
    if (failure) throw new Error(String(failure));
    const adapter = await view.evaluate("fixture.adapter"),
      spectral = prior?.spectral ?? [],
      atmosphere = prior?.atmosphere ?? [],
      visibility = [];
    for (const name of prior ? [] : ["distant", "oblique", "long-correlated"]) {
      const result = await view.evaluate<Record<string, unknown>>(`fixture.spectral('${name}')`);
      await Bun.write(`output/transport-research/spectral-${name}.json`, JSON.stringify(result));
      const { images: _, ...summary } = result;
      spectral.push(summary);
      console.log(JSON.stringify({ spectral: summary }));
    }
    for (const scale of prior ? [] : [8, 1.2]) {
      const result = await view.evaluate(`fixture.atmosphere(${scale})`);
      atmosphere.push(result);
      console.log(JSON.stringify({ atmosphere: result }));
    }
    for (const fraction of [0, 0.5, 1]) {
      const result = await view.evaluate(`fixture.visibility(${fraction})`);
      visibility.push(result);
      console.log(JSON.stringify({ visibility: result }));
    }
    const gpuErrors = await view.evaluate<string[]>("fixture.errors");
    const result = {
      created: new Date().toISOString(),
      adapter,
      methodology:
        "Hardware WebGPU compute kernels. Each timing: 9 alternating trials, each of 64 dispatch sequences, after 3 warmups. Long batches reduce Chrome's 65.536 microsecond timestamp quantization to about 1.024 microseconds per sequence. Includes GPU work between timestamped passes; excludes CPU preprocessing, shader compilation, submission and readback. Compilation costs are separately recorded. Synthetic fixtures, not whole-game FPS.",
      spectral,
      atmosphere,
      visibility,
      errors: [...errors, ...gpuErrors],
    };
    await Bun.write("output/transport-research/gpu.json", JSON.stringify(result, null, 2));
    if (result.errors.length) throw new Error(result.errors.join("\n"));
  } finally {
    server.stop(true);
  }
});
