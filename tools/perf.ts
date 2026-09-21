import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { distribution } from "./evidence";

const profile = process.argv.includes("--low") ? "low" : "balanced";
const width = profile === "low" ? 1280 : 1920,
  height = profile === "low" ? 720 : 1080;
const source = `import ${JSON.stringify(resolve("tools/fixtures/performance.ts"))};`;
type Run = {
  cpu: number[];
  gpu: number[];
  pacing: number[];
  gpuFrames: { frame: number; gpuMs: number }[];
  residency: {
    resident: number;
    cacheBytes: number;
    liveBytes: number;
    cpuArtifactsBytes: number;
    gpuBytes: number;
    pending: boolean;
  }[];
  measurements: { adapter: string };
  incomplete: { frame: number; rejected: unknown[]; uploading: string[] }[];
  completeness: { complete: boolean };
  runtime: { tick: number; blockedReason: string | null };
  diagnostics: { severity: string; code: string; message: string }[];
};
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(source, output);
    try {
      await view.navigate(`${server.url}?profile=${profile}`);
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw new Error(String(failure));
      await view.evaluate("fixture.run(90,false)");
      const runs = [];
      for (const workload of ["stationary-1", "stationary-2", "travel-288m", "thirteen-characters"]) {
        if (workload === "thirteen-characters") {
          await view.evaluate("fixture.prepareDensity()");
          await view.evaluate("fixture.run(60,false)");
        }
        const sample = await view.evaluate<Run>(
          `fixture.run(${workload === "travel-288m" ? 480 : 240},${workload === "travel-288m"})`,
        );
        runs.push({
          workload,
          ...sample,
          cpuMs: distribution(sample.cpu),
          gpuMs: distribution(sample.gpu),
          callbackIntervalMs: distribution(sample.pacing),
          hitchesOver50ms: sample.pacing.filter((value) => value > 50).length,
        });
      }
      await Bun.write(join(output, "reference-world.png"), await view.screenshot());
      const budgets = {
        residentPatches: 192,
        cpuArtifactsBytes: 96 * 1024 * 1024,
        gpuBytes: 256 * 1024 * 1024,
        cpuP95Ms: 12,
        gpuP95Ms: 16.7,
        callbackP95Ms: 22,
        callbackP99Ms: 34,
        minimumGpuSamples: 60,
        maxHitchFraction: 0.02,
      };
      const failures = [...errors];
      for (const run of runs) {
        const fail = (message: string) => failures.push(`${run.workload}: ${message}`);
        if (run.incomplete.length || !run.completeness.complete) fail("requested scene was incomplete");
        if (run.runtime.blockedReason) fail(`runtime blocked: ${run.runtime.blockedReason}`);
        if (run.cpuMs && run.cpuMs.p95 > budgets.cpuP95Ms) fail("CPU p95 budget exceeded");
        if (run.gpuMs && run.gpuMs.p95 > budgets.gpuP95Ms) fail("GPU p95 budget exceeded");
        if (!run.gpuMs || run.gpuMs.samples < budgets.minimumGpuSamples)
          fail("too few attributed GPU samples");
        if (
          run.callbackIntervalMs &&
          (run.callbackIntervalMs.p95 > budgets.callbackP95Ms ||
            run.callbackIntervalMs.p99 > budgets.callbackP99Ms)
        )
          fail("frame-callback pacing budget exceeded");
        if (run.hitchesOver50ms > Math.max(3, run.pacing.length * budgets.maxHitchFraction))
          fail("hitch budget exceeded");
        if (
          run.residency.some(
            (item) =>
              item.resident > budgets.residentPatches ||
              item.cpuArtifactsBytes > budgets.cpuArtifactsBytes ||
              item.gpuBytes > budgets.gpuBytes,
          )
        )
          fail("residency budget exceeded");
        for (const diagnostic of run.diagnostics)
          if (diagnostic.severity === "error" || diagnostic.code === "gpu-budget") fail(diagnostic.message);
      }
      const hardwareGpu = !/swiftshader|llvmpipe|software/i.test(runs[0].measurements.adapter);
      if (!hardwareGpu) failures.push("Hardware GPU required");
      const report = {
        schemaVersion: 2,
        sourceManifest: "source-manifest.json",
        environmentManifest: "environment.json",
        scenario: "winter-valley-v2",
        resolution: [width, height],
        quality: "interactive",
        profile,
        startupMs: await view.evaluate<number>("fixture.startupMs"),
        runs,
        budgets,
        hardwareGpu,
        timing:
          "CPU is scene evaluation plus submission. GPU samples are unique attributed frames. Frame intervals measure requestAnimationFrame callbacks, not display presentation.",
        coverage:
          "Local hardware engineering gate; release support requires results from each target machine/browser.",
        timestampQueries: runs.every((run) => run.gpu.length > 0)
          ? "available"
          : "unavailable; GPU timing coverage incomplete",
        errors: failures,
      };
      await Bun.write(join(output, "performance.json"), JSON.stringify(report, null, 2));
      console.log(
        JSON.stringify(
          {
            output,
            profile,
            startupMs: report.startupMs,
            runs: runs.map((run) => ({
              workload: run.workload,
              cpuMs: run.cpuMs,
              gpuMs: run.gpuMs,
              callbackIntervalMs: run.callbackIntervalMs,
            })),
            errors: failures,
          },
          null,
          2,
        ),
      );
      if (failures.length) throw new Error("Performance completeness/timing/resource gate failed");
    } finally {
      server.stop(true);
    }
  },
  width,
  height,
);
