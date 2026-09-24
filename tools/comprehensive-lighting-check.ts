import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { snapshotSource } from "./source-snapshot";

const baseline =
  process.argv.find((a) => a.startsWith("--baseline="))?.slice(11) ??
  resolve("output/comprehensive-lighting/baseline");
const automatic = process.argv.includes("--automatic-baseline");
const ablation = process.argv.find((a) => a.startsWith("--ablation="))?.slice(11);
if (ablation && !["indirect", "reflection", "shadow", "direct"].includes(ablation))
  throw Error("Invalid lighting ablation");
const kinds = process.argv
  .find((a) => a.startsWith("--kinds="))
  ?.slice(8)
  .split(",") ?? [
  "outdoor",
  "dusk",
  "windowed",
  "room",
  "entrance",
  "sealed",
  "cave",
  "interior",
  "emissive",
  "night",
  "eight-lights",
  "moving",
  "winter",
];
if (!process.argv.includes("--frozen")) {
  const path = resolve("output/comprehensive-lighting", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(path, "comprehensive-lighting");
  console.log(JSON.stringify({ source: path, fingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn(
    [
      process.execPath,
      "tools/comprehensive-lighting-check.ts",
      "--frozen",
      `--baseline=${resolve(baseline)}`,
      `--kinds=${kinds.join(",")}`,
      ...(automatic ? ["--automatic-baseline"] : []),
      ...(ablation ? [`--ablation=${ablation}`] : []),
    ],
    { cwd: path, stdout: "inherit", stderr: "inherit" },
  );
  process.exit(await child.exited);
}
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `${automatic ? `import * as PreviousRuntime from ${JSON.stringify(resolve(baseline, "packages/runtime/src/index.ts"))};` : ""}import {WebGPURenderer as Previous} from ${JSON.stringify(resolve(baseline, "packages/render-webgpu/src/index.ts"))};import {comprehensiveLightingFixture} from ${JSON.stringify(resolve("tools/fixtures/comprehensive-lighting.ts"))};comprehensiveLightingFixture(Previous,${automatic ? "PreviousRuntime" : "undefined"},${JSON.stringify(ablation)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 90000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const reports = [];
      for (const kind of kinds) {
        await view.evaluate(
          `(()=>{window.result=undefined;window.failure=undefined;fixture.capture(${JSON.stringify(kind)}).then(r=>window.result=r).catch(e=>window.failure=String(e));return true;})()`,
        );
        await waitFor(view, "window.result||window.failure", 300000);
        const error = await view.evaluate("window.failure");
        if (error) throw Error(String(error));
        const result = await view.evaluate<{ images: Record<string, string>; report: unknown }>(
          "window.result",
        );
        for (const [name, data] of Object.entries(result.images))
          await Bun.write(join(output, `${kind}-${name}.png`), Buffer.from(data, "base64"));
        reports.push(result.report);
        console.log(JSON.stringify({ kind, report: result.report }));
        await Bun.write(
          join(output, "comprehensive-lighting.json"),
          JSON.stringify({ reports, errors }, null, 2),
        );
      }
      if (errors.length) throw Error(errors.join(";"));
      console.log(JSON.stringify({ output, errors }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  960,
  720,
);
