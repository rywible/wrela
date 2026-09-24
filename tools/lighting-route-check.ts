import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { snapshotSource } from "./source-snapshot";

const comparison = process.argv.find((a) => a.startsWith("--compare="))?.slice(10);
const quick = process.argv.includes("--quick");
const compareRuntime = process.argv.includes("--compare-runtime");
const emission = process.argv.includes("--emission");
const gain = process.argv.includes("--gain");
const hd = process.argv.includes("--hd");
const temporal = process.argv.includes("--temporal");
const indirect = process.argv.includes("--indirect");
const noReflection = process.argv.includes("--no-reflection");
const steady = process.argv.includes("--steady");
const reverse = process.argv.includes("--reverse");
const width = hd ? 1920 : 960,
  height = hd ? 1080 : 720;

if (!process.argv.includes("--frozen")) {
  const path = resolve("output/lighting-route", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(path, "lighting-route");
  console.log(JSON.stringify({ source: path, fingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn(
    [
      process.execPath,
      "tools/lighting-route-check.ts",
      "--frozen",
      ...(quick ? ["--quick"] : []),
      ...(compareRuntime ? ["--compare-runtime"] : []),
      ...(emission ? ["--emission"] : []),
      ...(gain ? ["--gain"] : []),
      ...(hd ? ["--hd"] : []),
      ...(temporal ? ["--temporal"] : []),
      ...(indirect ? ["--indirect"] : []),
      ...(noReflection ? ["--no-reflection"] : []),
      ...(steady ? ["--steady"] : []),
      ...(reverse ? ["--reverse"] : []),
      ...(comparison ? [`--compare=${resolve(comparison)}`] : []),
    ],
    {
      cwd: path,
      stdout: "inherit",
      stderr: "inherit",
    },
  );
  process.exit(await child.exited);
}
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `${comparison ? `import {WebGPURenderer as Previous} from ${JSON.stringify(resolve(comparison, "packages/render-webgpu/src/index.ts"))};` : ""}${comparison && compareRuntime ? `import {RadianceLightingCache as PreviousRuntime} from ${JSON.stringify(resolve(comparison, "packages/runtime/src/radiance-lighting.ts"))};` : ""}import {lightingRouteFixture} from ${JSON.stringify(resolve("tools/fixtures/lighting-route.ts"))};window.start=(previous)=>lightingRouteFixture(${comparison ? "previous?Previous:undefined" : "undefined"},${comparison && compareRuntime ? "previous?PreviousRuntime:undefined" : "undefined"},${JSON.stringify({ width, height, temporal, indirect, noReflection, steady })}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const reports: Record<string, unknown>[] = [];
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.start", 30000);
      const cases = gain
        ? [
            ["room", false, "open", 1],
            ["room", false, "open", 4],
            ["room", false, "open", 0.25],
            ["room", false, "open", 0],
          ]
        : emission
          ? [
              ["room", false, "open"],
              ["room", true, "open"],
              ["cave", true, "closed"],
            ]
          : quick
            ? [
                ["exterior", false, "open"],
                ["room", false, "open"],
                ["cave", false, "closed"],
              ]
            : [
                ["exterior", false, "open"],
                ["entrance", false, "open"],
                ["room", false, "open"],
                ["cave", false, "open"],
                ["cave", false, "closed"],
                ["cave", false, "fixed"],
                ["deep", false, "closed"],
                ["deep", false, "fixed"],
                ["exterior", true, "open"],
                ["room", true, "open"],
                ["cave", true, "closed"],
                ["cave", true, "fixed"],
              ];
      for (const variant of comparison
        ? reverse
          ? ["after", "before"]
          : ["before", "after"]
        : ["current"]) {
        await view.evaluate(
          `(()=>{window.ready=false;window.failure=undefined;start(${variant === "before"});return true;})()`,
        );
        await waitFor(view, "window.ready||window.failure", 180000);
        const startup = await view.evaluate("window.failure");
        if (startup) throw Error(String(startup));
        for (const args of cases) {
          await view.evaluate(
            `(()=>{window.result=undefined;window.failure=undefined;fixture.capture(...${JSON.stringify(args)}).then(r=>window.result=r).catch(e=>window.failure=String(e));return true;})()`,
          );
          await waitFor(view, "window.result||window.failure", 300000);
          const failure = await view.evaluate("window.failure");
          if (failure) throw Error(String(failure));
          const result = await view.evaluate<{
            png: string;
            identityPng: string;
            report: Record<string, unknown>;
          }>("window.result");
          await Bun.write(
            join(output, `${variant}-${result.report.shot}.png`),
            Buffer.from(result.png, "base64"),
          );
          await Bun.write(
            join(output, `${variant}-${result.report.shot}-identity.png`),
            Buffer.from(result.identityPng, "base64"),
          );
          reports.push({ variant, ...result.report });
          console.log(
            JSON.stringify({
              shot: result.report.shot,
              variant,
              gpuMs: result.report.gpuMs,
              shadowMs: result.report.shadowMs,
              sceneMs: result.report.sceneMs,
              imageDifference: result.report.imageDifference,
              mobilityDifference: result.report.mobilityDifference,
              readinessMs: result.report.readinessMs,
              radianceBuilds: result.report.radianceBuilds,
            }),
          );
          await Bun.write(join(output, "lighting-route.json"), JSON.stringify({ reports, errors }, null, 2));
          const counterpart = reports.find((r) => r.variant !== variant && r.shot === result.report.shot);
          if (counterpart && counterpart.identityHash !== result.report.identityHash)
            throw Error("Renderer change altered camera identity/coverage");
        }
        await view.evaluate("window.fixture.dispose()");
      }
      if (errors.length) throw Error(errors.join(";"));
      console.log(JSON.stringify({ output, errors }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  width,
  height,
);
