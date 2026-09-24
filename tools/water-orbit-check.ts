import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { recordWaterOrbit } from "./fixtures/water-orbit";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {recordWaterOrbit} from ${JSON.stringify(resolve("tools/fixtures/water-orbit.ts"))};window.run=recordWaterOrbit;window.ready=true;`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready", 60000);
      const { video, captures, ...result } =
        await view.evaluate<Awaited<ReturnType<typeof recordWaterOrbit>>>("run()");
      await Bun.write(join(output, "orbit.webm"), Buffer.from(video, "base64"));
      for (const [i, capture] of captures.entries())
        await Bun.write(join(output, `orbit-${i}.png`), Buffer.from(capture.image.split(",")[1], "base64"));
      await Bun.write(
        join(output, "orbit.json"),
        JSON.stringify(
          { ...result, captureTimes: captures.map((c) => c.time), browserErrors: errors },
          null,
          2,
        ),
      );
      if (errors.length) throw Error(errors.join("\n"));
      console.log(
        JSON.stringify({ output, frames: result.frames, impulseVolumeError: result.impulseVolumeError }),
      );
    } finally {
      server.stop(true);
    }
  },
  960,
  540,
  "chrome",
  600000,
  "visual-review",
);
