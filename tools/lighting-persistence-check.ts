import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--frozen")) {
  const path = resolve("output/lighting-persistence", `${Date.now()}-${process.pid}`, "source");
  const manifest = await snapshotSource(path, "lighting-persistence");
  console.log(JSON.stringify({ source: path, fingerprint: manifest.sourceFingerprint }));
  const child = Bun.spawn(
    [
      process.execPath,
      "tools/lighting-persistence-check.ts",
      "--frozen",
      ...(process.argv.includes("--traversal") ? ["--traversal"] : []),
      ...(process.argv.includes("--route") ? ["--route"] : []),
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
      `import {lightingPersistenceFixture} from ${JSON.stringify(resolve("tools/fixtures/lighting-persistence.ts"))};lightingPersistenceFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const reports: Record<string, unknown>[] = [];
    try {
      for (const phase of process.argv.includes("--traversal")
        ? ["travel-cold", "travel-restored"]
        : ["cold", "restored"]) {
        await view.navigate(String(server.url));
        await waitFor(view, "window.ready||window.failure", 90000);
        const startup = await view.evaluate("window.failure");
        if (startup) throw Error(String(startup));
        for (const kind of process.argv.includes("--route")
          ? ["route"]
          : process.argv.includes("--traversal")
            ? ["winter"]
            : ["furnished", "winter"]) {
          await view.evaluate(
            `(()=>{window.result=undefined;window.failure=undefined;fixture.capture(${JSON.stringify(kind)},${process.argv.includes("--traversal")}).then(r=>window.result=r).catch(e=>window.failure=String(e));return true;})()`,
          );
          await waitFor(view, "window.result||window.failure", 300000);
          const failure = await view.evaluate("window.failure");
          if (failure) throw Error(String(failure));
          const result = await view.evaluate<{ png: string; report: Record<string, unknown> }>(
            "window.result",
          );
          await Bun.write(join(output, `${kind}-${phase}.png`), Buffer.from(result.png, "base64"));
          reports.push({ phase, ...result.report });
          console.log(JSON.stringify(reports.at(-1)));
          await Bun.write(
            join(output, "lighting-persistence.json"),
            JSON.stringify({ reports, errors }, null, 2),
          );
          if (result.report.storageError) throw Error(String(result.report.storageError));
          if (phase.endsWith("restored")) {
            const original = reports.find((r) => String(r.phase).endsWith("cold") && r.kind === kind);
            if (Number(result.report.initialStorageHits) < 1 || result.report.initialBuilds !== 0)
              throw Error("Reload recompiled lighting");
            if (
              original?.transferHash !== result.report.transferHash ||
              original?.imageHash !== result.report.imageHash
            )
              throw Error("Reload changed rendered lighting");
          }
        }
        await view.evaluate("window.fixture.dispose()");
      }
      if (errors.length) throw Error(errors.join(";"));
      console.log(JSON.stringify({ output, errors }));
    } finally {
      server.stop(true);
    }
  },
  960,
  720,
);
