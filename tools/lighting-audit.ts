import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createLightingAuditFixture } from "./fixtures/lighting-audit";
import { snapshotSource } from "./source-snapshot";

// Freeze the audited renderer so concurrent authoring cannot change a comparison.
if (!process.argv.includes("--frozen")) {
  const destination = resolve("output/lighting-audit", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(destination, "lighting-audit-2026-09-23");
  console.log(JSON.stringify({ sourceSnapshot: destination, sourceFingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn([process.execPath, "tools/lighting-audit.ts", "--frozen"], {
    cwd: destination,
    stdout: "inherit",
    stderr: "inherit",
  });
  process.exit(await child.exited);
}

type Result = Awaited<ReturnType<Awaited<ReturnType<typeof createLightingAuditFixture>>["capture"]>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createLightingAuditFixture} from ${JSON.stringify(resolve("tools/fixtures/lighting-audit.ts"))};createLightingAuditFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const result = await view.evaluate<Result>("fixture.capture()");
      const captures = [];
      for (const { name, image, ...metadata } of result.captures) {
        await Bun.write(join(output, `${name}.png`), Buffer.from(image, "base64"));
        captures.push({ name, ...metadata });
      }
      const report = { ...result, captures, browserErrors: errors, performanceComparable: false };
      await Bun.write(join(output, "lighting-audit.json"), JSON.stringify(report, null, 2));
      if (errors.length || result.errors.length) throw Error([...errors, ...result.errors].join(";"));
      console.log(JSON.stringify({ output, report }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  640,
  480,
);
