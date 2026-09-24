import { join, resolve } from "node:path";
import { forestEdgeCamera } from "@wrela/examples";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { vegetationStudies } from "./fixtures/vegetation-study";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--snapshot-run")) {
  const destination = resolve("output/forest-walk", `${Date.now()}-${process.pid}`, "source");
  await snapshotSource(destination, "forest-walk");
  console.log(JSON.stringify({ snapshot: destination }));
  process.exit(
    await Bun.spawn([process.execPath, "tools/forest-walk.ts", ...process.argv.slice(2), "--snapshot-run"], {
      cwd: destination,
      stdout: "inherit",
      stderr: "inherit",
    }).exited,
  );
}
await withBrowser(
  async (view, output, errors) => {
    const study = { ...vegetationStudies({ forest: true })[0], antialiasing: "temporal" };
    const server = await fixtureServer(
      `import {createLookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/lookdev.ts"))};createLookdevFixture(${JSON.stringify(study)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const frames = [];
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      for (const t of [0, 6, 12, 18, 24]) {
        const camera = forestEdgeCamera(t);
        // Warm actual reconstruction, then retain consecutive moving frames.
        await view.evaluate(
          `fixture.benchmark(${JSON.stringify(Array.from({ length: 32 }, (_, i) => t + i / 60))},${JSON.stringify(camera)},0.005)`,
        );
        for (let i = 0; i < 4; i++) {
          const result = await view.evaluate(
            `fixture.benchmark(${JSON.stringify(Array.from({ length: 4 }, (_, j) => t + (32 + i * 4 + j) / 60))},${JSON.stringify({ ...camera, position: [camera.position[0] + 0.16 + i * 0.02, camera.position[1], camera.position[2]], target: [camera.target[0] + 0.16 + i * 0.02, camera.target[1], camera.target[2]] })},0.005)`,
          );
          const image = `walk-${t}-${i}.png`;
          await Bun.write(join(output, image), await view.screenshot());
          frames.push({ time: t, index: i, image, result });
        }
        console.log(JSON.stringify({ output, time: t }));
      }
      await Bun.write(
        join(output, "walk.json"),
        JSON.stringify(
          {
            width: 1920,
            height: 1080,
            antialiasing: "temporal",
            scope:
              "Native compositor captures after 32-frame history warmup, with moving camera and wind. Short consecutive sequences at five eye-level route positions; not a temporal-error qualification or sustained performance test.",
            frames,
            errors,
          },
          null,
          2,
        ),
      );
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  1920,
  1080,
  "chrome",
  180000,
);
