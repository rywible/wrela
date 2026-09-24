import { cp } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { snapshotSource } from "./source-snapshot";

const baseline = process.argv.find((v) => v.startsWith("--baseline="))?.slice(11);
if (!baseline) throw Error("Supply a frozen baseline renderer");
if (!process.argv.includes("--frozen")) {
  const path = resolve("output/default-lighting", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(path, "default-lighting");
  console.log(JSON.stringify({ source: path, fingerprint: manifest.sourceFingerprint }));
  let comparison = resolve(baseline);
  if (process.argv.includes("--matched-control")) {
    // Preserve concurrent renderer work in both arms. Only the two lighting
    // shader sources come from the supplied pre-upgrade snapshot.
    comparison = resolve(path, "../control");
    await cp(path, comparison, { recursive: true, verbatimSymlinks: true });
    const files = manifest.files.map((file) => ({ ...file }));
    for (const [name, declaration] of [
      ["scene.wgsl", "override LOCAL_SKY_VISIBILITY:bool=false;"],
      ["indirect.wgsl", "override LOCAL_INDIRECT_LIGHTING:bool=true;"],
    ]) {
      const relative = `packages/render-webgpu/src/${name}`;
      const source = await Bun.file(join(baseline, relative)).text();
      const code = `${declaration}\n${source}`;
      await Bun.write(join(comparison, relative), code);
      const entry = files.find((file) => file.path === relative);
      if (!entry) throw Error(`Missing control source ${relative}`);
      entry.bytes = new TextEncoder().encode(code).byteLength;
      entry.sha256 = new Bun.CryptoHasher("sha256").update(code).digest("hex");
    }
    const fingerprint = new Bun.CryptoHasher("sha256").update(JSON.stringify(files)).digest("hex");
    await Bun.write(
      join(comparison, "source-manifest.json"),
      JSON.stringify(
        { ...manifest, files, sourceFingerprint: fingerprint, controlShaders: resolve(baseline) },
        null,
        2,
      ),
    );
    console.log(JSON.stringify({ control: comparison, fingerprint }));
  }
  const child = Bun.spawn(
    [
      process.execPath,
      "tools/default-lighting-check.ts",
      "--frozen",
      `--baseline=${comparison}`,
      ...process.argv.slice(2).filter((v) => !v.startsWith("--baseline=")),
    ],
    { cwd: path, stdout: "inherit", stderr: "inherit" },
  );
  process.exit(await child.exited);
}
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {WebGPURenderer as Previous} from ${JSON.stringify(resolve(baseline, "packages/render-webgpu/src/index.ts"))};import {defaultLightingFixture} from ${JSON.stringify(resolve("tools/fixtures/default-lighting.ts"))};defaultLightingFixture(Previous).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const reports = [];
      for (const kind of process.argv.includes("--winter")
        ? ["winter"]
        : process.argv.includes("--quick")
          ? ["alpine"]
          : ["alpine", "room", "closed", "winter"]) {
        await view.evaluate(
          `(()=>{window.result=undefined;window.failure=undefined;fixture.capture(${JSON.stringify(kind)}).then(r=>window.result=r).catch(e=>window.failure=String(e));return true;})()`,
        );
        await waitFor(view, "window.result||window.failure", 240000);
        const error = await view.evaluate("window.failure");
        if (error) throw Error(String(error));
        const result = await view.evaluate<{ images: Record<string, string>; report: unknown }>(
          "window.result",
        );
        for (const [name, data] of Object.entries(result.images))
          await Bun.write(join(output, `${kind}-${name}.png`), Buffer.from(data, "base64"));
        reports.push(result.report);
        console.log(JSON.stringify({ kind, report: result.report }));
        await Bun.write(join(output, "default-lighting.json"), JSON.stringify({ reports, errors }, null, 2));
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
