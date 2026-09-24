import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createShootQualification } from "./fixtures/shoot-qualification";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--snapshot-run")) {
  const destination = resolve("output/shoot-qualification", `${Date.now()}-${process.pid}`, "source");
  await snapshotSource(destination, "shoot-qualification");
  console.log(JSON.stringify({ snapshot: destination }));
  process.exit(
    await Bun.spawn(
      [process.execPath, "tools/shoot-qualification.ts", ...process.argv.slice(2), "--snapshot-run"],
      { cwd: destination, stdout: "inherit", stderr: "inherit" },
    ).exited,
  );
}
type Result = Awaited<ReturnType<Awaited<ReturnType<typeof createShootQualification>>["compare"]>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createShootQualification} from ${JSON.stringify(resolve("tools/fixtures/shoot-qualification.ts"))};createShootQualification().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const results: Result[] = [];
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      for (const variant of process.argv.includes("--matrix") ? [0, 3, 4, 7] : [0])
        for (const pixels of [16, 64, 128])
          for (const angle of [0.31, 1.62]) {
            const result = await view.evaluate<Result>(`fixture.compare(${pixels},${angle},0.27,${variant})`);
            if (/software|swiftshader|llvmpipe/i.test(result.adapter)) throw Error("Hardware required");
            results.push(result);
            console.log(JSON.stringify({ output, ...result }));
            await Bun.write(
              join(output, "qualification.json"),
              JSON.stringify(
                {
                  status: results.every((r) => r.passes) ? "static-cases-pass" : "unqualified",
                  scope:
                    "Static, isolated source shoot. Coverage uses 4x/8x supersampling; radiance uses 4x, fixed daylight and foreground energy relative to empty background. These cases cannot qualify motion or light-angle changes.",
                  results,
                  errors,
                },
                null,
                2,
              ),
            );
          }
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  1280,
  1280,
  "chrome",
  300000,
);
