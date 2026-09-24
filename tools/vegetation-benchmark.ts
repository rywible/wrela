import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createVegetationBenchmark } from "./fixtures/vegetation-benchmark";
import { vegetationStudies } from "./fixtures/vegetation-study";
import { quantiles } from "./rendering-compiler/timing";
import { snapshotSource } from "./source-snapshot";

const value = (name: string, fallback: string) =>
  process.argv.find((arg) => arg.startsWith(`--${name}=`))?.slice(name.length + 3) ?? fallback;
const number = (name: string, fallback: number, maximum: number) => {
  const n = Number(value(name, String(fallback)));
  if (!Number.isFinite(n) || n <= 0 || n > maximum) throw Error(`Invalid --${name}`);
  return n;
};
if (!process.argv.includes("--snapshot-run")) {
  const destination = resolve("output/vegetation-benchmarks", `${Date.now()}-${process.pid}`, "source");
  await snapshotSource(destination, "vegetation-benchmark");
  const child = Bun.spawn(
    [process.execPath, "tools/vegetation-benchmark.ts", ...process.argv.slice(2), "--snapshot-run"],
    { cwd: destination, stdout: "inherit", stderr: "inherit" },
  );
  console.log(JSON.stringify({ snapshot: destination }));
  process.exit(await child.exited);
}
const thermal = process.argv.includes("--thermal");
const duration = number("duration", thermal ? 1800 : 60, 3600);
const warmup = number("warmup", thermal ? 300 : 5, 600);
const trials = number("trials", thermal ? 1 : 3, 10);
if (!Number.isInteger(trials)) throw Error("Trials must be an integer");
const width = number("width", 1920, 3840),
  height = number("height", 1080, 2160);
if (!Number.isInteger(width) || !Number.isInteger(height))
  throw Error("Benchmark dimensions must be integers");
const paced = process.argv.includes("--paced"),
  moving = process.argv.includes("--moving");
const diagnostic = process.argv.includes("--albedo");
const profileCpu = process.argv.includes("--cpu-profile");
const vegetation = !process.argv.includes("--no-vegetation");
const antialiasing = value("aa", "msaa");
if (!["msaa", "temporal", "spatial"].includes(antialiasing)) throw Error("Invalid anti-aliasing");
const ablation = value("ablation", "none");
if (
  ![
    "none",
    "indirect",
    "sun",
    "reflection",
    "shadow",
    "aerial",
    "direct",
    "sky",
    "foliage-lighting",
  ].includes(ablation)
)
  throw Error("Unknown lighting ablation");
const frameId = value("frame", "stand-gameplay");
const architecture = process.argv.includes("--architecture");
if (architecture && value("growth", "")) throw Error("Choose architecture or experimental growth");
const study = vegetationStudies({
  architecture,
  development:
    value("growth", "") === "pine"
      ? "lodgepole-pine"
      : value("growth", "") === "birch"
        ? "paper-birch"
        : undefined,
}).find((s) => s.frames.some((f) => f.id === frameId));
if (!study) throw Error("Unknown vegetation frame");
const frameIndex = study.frames.findIndex((f) => f.id === frameId);
type Fixture = Awaited<ReturnType<typeof createVegetationBenchmark>>;
type Batch = Awaited<ReturnType<Fixture["batch"]>>;
const machine = Bun.spawnSync(["system_profiler", "SPHardwareDataType", "SPDisplaysDataType", "-json"]);
const raw = machine.exitCode === 0 ? JSON.parse(machine.stdout.toString()) : {};
const hardware = {
  hardware: (raw.SPHardwareDataType ?? []).map((h: Record<string, unknown>) =>
    Object.fromEntries(
      ["machine_name", "machine_model", "chip_type", "number_processors", "physical_memory"].map((k) => [
        k,
        h[k],
      ]),
    ),
  ),
  graphics: (raw.SPDisplaysDataType ?? []).map((h: Record<string, unknown>) => ({
    chip: h.sppci_model,
    cores: h.sppci_cores,
  })),
  power: Bun.spawnSync(["pmset", "-g", "batt"]).stdout.toString(),
  os: Bun.spawnSync(["sw_vers"]).stdout.toString(),
  memoryPressure: Bun.spawnSync(["memory_pressure", "-Q"]).stdout.toString(),
  swap: Bun.spawnSync(["sysctl", "vm.swapusage"]).stdout.toString(),
  powerSettings: Bun.spawnSync(["pmset", "-g", "custom"]).stdout.toString(),
};
await withBrowser(
  async (view, output, errors) => {
    await Bun.write(join(output, "authored-project.json"), JSON.stringify(study.project, null, 2));
    const server = await fixtureServer(
      `import {createVegetationBenchmark} from ${JSON.stringify(resolve("tools/fixtures/vegetation-benchmark.ts"))};createVegetationBenchmark(${JSON.stringify(study)},${frameIndex},${JSON.stringify({ antialiasing, resolutionScale: 1, finiteSun: false, lightingAblation: ablation, materialSpecialization: !process.argv.includes("--generic-materials"), thinCoveragePrepass: !process.argv.includes("--no-thin-prepass") })},${JSON.stringify({ vegetation })}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const runs: unknown[] = [];
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const batch = () => view.evaluate<Batch>(`fixture.batch(24,${moving},${paced},${diagnostic},4)`);
      const warmed = performance.now();
      do {
        await batch();
      } while (performance.now() - warmed < warmup * 1000);
      if (profileCpu) {
        await view.cdp("Profiler.enable");
        await view.cdp("Profiler.setSamplingInterval", { interval: 1000 });
        await view.cdp("Profiler.start");
      }
      for (let trial = 0; trial < trials; trial++) {
        const batches: (Batch & { collectedAfterMs: number })[] = [];
        const started = performance.now();
        let progressAt = started;
        do {
          const b = await batch();
          if (/software|swiftshader|llvmpipe/i.test(b.measurements.adapter))
            throw Error("Hardware adapter required");
          if (b.frames.some((f) => !f.complete)) throw Error("Incomplete vegetation benchmark frame");
          if (
            b.frames.length !== b.gpu.length ||
            new Set(b.gpu.map((s) => s.frame)).size !== b.frames.length ||
            b.frames.some((f) => !b.gpu.some((s) => s.frame === f.frame)) ||
            b.dropped
          )
            throw Error("GPU timing did not match every submitted frame");
          batches.push({ ...b, collectedAfterMs: performance.now() - started });
          if (performance.now() - progressAt > 15000) {
            console.log(
              JSON.stringify({
                trial,
                seconds: (performance.now() - started) / 1000,
                latestGpu: quantiles(b.gpu.map((s) => s.gpuMs)),
              }),
            );
            await Bun.write(join(output, "checkpoint.json"), JSON.stringify({ trial, batches }));
            progressAt = performance.now();
          }
        } while (performance.now() - started < duration * 1000);
        const gpu = batches.flatMap((b) => b.gpu),
          frames = batches.flatMap((b) => b.frames);
        const elapsedMs = performance.now() - started;
        const intervals = [...new Set(gpu.flatMap((s) => s.intervals?.map((i) => i.pass) ?? []))];
        const summary = {
          trial,
          elapsedMs,
          completedFrames: gpu.length,
          completedFramesPerSecond: (gpu.length * 1000) / elapsedMs,
          gpu: quantiles(gpu.map((s) => s.gpuMs)),
          renderCpu: quantiles(frames.map((f) => f.renderMs)),
          prepareCpu: quantiles(frames.map((f) => f.prepareMs)),
          totalCpu: quantiles(frames.map((f) => f.prepareMs + f.renderMs)),
          passes: Object.fromEntries(
            intervals.map((p) => [
              p,
              quantiles(
                gpu.flatMap(
                  (s) => s.intervals?.filter((i) => i.pass === p).map((i) => i.endMs - i.startMs) ?? [],
                ),
              ),
            ]),
          ),
          maximumGpuBytes: frames.reduce((maximum, f) => Math.max(maximum, f.gpuBytes), 0),
          finalTenMinutes: (() => {
            if (elapsedMs < 600000) return null;
            const tail = batches.filter((b) => b.collectedAfterMs >= elapsedMs - 600000);
            const samples = tail.flatMap((b) => b.gpu),
              cpu = tail.flatMap((b) => b.frames);
            return {
              durationMs: 600000,
              frames: samples.length,
              gpu: quantiles(samples.map((s) => s.gpuMs)),
              renderCpu: quantiles(cpu.map((s) => s.renderMs)),
              prepareCpu: quantiles(cpu.map((s) => s.prepareMs)),
              totalCpu: quantiles(cpu.map((s) => s.prepareMs + s.renderMs)),
              completedFramesPerSecond: samples.length / 600,
            };
          })(),
        };
        runs.push({ summary, batches });
        await Bun.write(
          join(output, "vegetation-benchmark.json"),
          JSON.stringify(
            {
              benchmarkRevision: 2,
              simulation: "one 1/120-second game update per submitted frame; no editor replay",
              hardware,
              frameId,
              settings: {
                specimen: architecture ? "shaped-pine" : value("growth", "original"),
                width,
                height,
                warmup,
                duration,
                trials,
                paced,
                moving,
                diagnostic,
                antialiasing,
                vegetation,
                finiteSun: false,
                profileCpu,
                ablation,
                materialSpecialization: !process.argv.includes("--generic-materials"),
                thinCoveragePrepass: !process.argv.includes("--no-thin-prepass"),
              },
              runs,
              powerAtEnd: Bun.spawnSync(["pmset", "-g", "batt"]).stdout.toString(),
              thermalAtEnd: Bun.spawnSync(["pmset", "-g", "therm"]).stdout.toString(),
              errors,
              scope:
                "Matched frame GPU intervals can overlap. Throughput includes bounded submissions and browser/tool overhead; presentation is separate.",
            },
            null,
            2,
          ),
        );
        console.log(JSON.stringify({ output, ...summary }));
      }
      if (profileCpu)
        await Bun.write(join(output, "cpu-profile.json"), JSON.stringify(await view.cdp("Profiler.stop")));
      const image = await view.evaluate<string>("fixture.capture()");
      await Bun.write(join(output, "final.png"), Buffer.from(image.split(",")[1], "base64"));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  width,
  height,
  "chrome",
  (warmup + duration * trials + 180) * 1000,
);
