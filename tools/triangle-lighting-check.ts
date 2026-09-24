import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--frozen")) {
  const path = resolve("output/triangle-lighting", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(path, "compiled-triangle-lighting");
  console.log(JSON.stringify({ source: path, fingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn(
    [process.execPath, "tools/triangle-lighting-check.ts", "--frozen", ...process.argv.slice(2)],
    { cwd: path, stdout: "inherit", stderr: "inherit" },
  );
  process.exit(await child.exited);
}
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createTriangleLightingFixture} from ${JSON.stringify(resolve("tools/fixtures/triangle-lighting.ts"))};createTriangleLightingFixture(${process.argv.includes("--large") ? "1280,960" : "640,480"}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const reports = [];
      for (const kind of process.argv.includes("--quick")
        ? ["alpine"]
        : process.argv.includes("--winter")
          ? ["winter"]
          : ["alpine", "material", "sealed", "winter"]) {
        await view.evaluate(
          `(()=>{window.result=undefined;window.captureError=undefined;fixture.capture(${JSON.stringify(kind)}).then(r=>window.result=r).catch(e=>window.captureError=String(e));return true;})()`,
        );
        await waitFor(view, "window.result||window.captureError", 240000);
        const captureError = await view.evaluate("window.captureError");
        if (captureError) throw Error(String(captureError));
        const result = await view.evaluate<{ images: Record<string, string>; report: { errors: string[] } }>(
          "window.result",
        );
        for (const [name, bytes] of Object.entries(result.images))
          await Bun.write(join(output, `${kind}-${name}.png`), Buffer.from(bytes, "base64"));
        reports.push(result.report);
        console.log(JSON.stringify({ kind, report: result.report }));
        await Bun.write(
          join(output, "triangle-lighting.json"),
          JSON.stringify({ reports, browserErrors: errors }, null, 2),
        );
      }
      console.log(JSON.stringify({ output, errors }));
      if (errors.length || reports.some((r) => r.errors.length))
        throw Error("Triangle lighting failed acceptance; see report");
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  640,
  480,
);
