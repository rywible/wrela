import { join, resolve } from "node:path";
import { waitFor, withBrowser } from "./browser";
import { distribution } from "./evidence";

// Replay an exact saved lookdev bundle, so comparisons never require rewriting
// a dirty checkout. The bundle, rather than today's source hash, owns the result.
const path = process.argv.find((arg) => arg.startsWith("--bundle="))?.slice(9);
if (!path) throw Error("Expected --bundle=<saved lookdev fixture.js>");
const bundle = await Bun.file(resolve(path)).text();
await withBrowser(
  async (view, output, errors) => {
    const server = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      fetch(req) {
        return new URL(req.url).pathname === "/fixture.js"
          ? new Response(bundle, { headers: { "Content-Type": "text/javascript" } })
          : new Response(
              '<!doctype html><style>html,body{margin:0;width:100%;height:100%}canvas{width:100%;height:100%;display:block}</style><canvas></canvas><script type="module" src="/fixture.js"></script>',
              { headers: { "Content-Type": "text/html" } },
            );
      },
    });
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const camera = { position: [14, 7, -24], target: [-6, 28, 4], fov: 60 };
      const cases = [];
      for (const [name, move, turn, wind] of [
        ["static", 0, 0, false],
        ["walking", 0.12, 0, false],
        ["turning", 0, 0.004363323, false],
        ["weather", 0, 0, true],
      ] as const) {
        const result = await view.evaluate<{
          samples: {
            measurements: { gpuMs: number; gpuBytes: number; cpuMs: number; gpuTimingDroppedFrames?: number };
            gpuTimings: { atmosphereMs: number }[];
          }[];
          diagnostics: string[];
          adapter: string;
        }>(
          `fixture.benchmark(Array.from({length:63},(_,i)=>32+${wind ? "i/30" : "0"}),${JSON.stringify(camera)},${move},${turn})`,
        );
        if (result.diagnostics.length) throw Error(result.diagnostics.join("\n"));
        if (result.samples.some((s) => s.gpuTimings.length !== 1))
          throw Error(`Incomplete GPU timings in ${name}; refusing biased frame statistics`);
        const atmosphere = result.samples.flatMap((s) => s.gpuTimings.map((t) => t.atmosphereMs));
        cases.push({
          name,
          adapter: result.adapter,
          gpuMs: distribution(result.samples.map((s) => s.measurements.gpuMs)),
          cpuMs: distribution(result.samples.map((s) => s.measurements.cpuMs)),
          atmosphereMs: distribution(atmosphere),
          updatedAtmosphereMs: distribution(atmosphere.filter((v) => v > 0)),
          gpuBytes: result.samples.at(-1)?.measurements.gpuBytes,
          timingFrames: result.samples.flatMap((s) => s.gpuTimings).length,
          droppedTimings: result.samples.at(-1)?.measurements.gpuTimingDroppedFrames ?? 0,
          samples: result.samples,
        });
      }
      const result = {
        bundle: resolve(path),
        bundleSha256: new Bun.CryptoHasher("sha256").update(bundle).digest("hex"),
        width: 1024,
        height: 768,
        cases,
        errors,
      };
      await Bun.write(join(output, "sky-replay.json"), JSON.stringify(result, null, 2));
      console.log(JSON.stringify({ output, cases: cases.map(({ samples, ...c }) => c) }));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  1024,
  768,
);
