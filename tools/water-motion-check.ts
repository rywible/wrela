import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { checkWaterMotion } from "./fixtures/water-motion";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {checkWaterMotion} from ${JSON.stringify(resolve("tools/fixtures/water-motion.ts"))};window.run=checkWaterMotion;window.ready=true;`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready", 60000);
      const { images, ...result } =
        await view.evaluate<Awaited<ReturnType<typeof checkWaterMotion>>>("run()");
      for (const [name, data] of Object.entries(images))
        await Bun.write(join(output, `${name}.png`), Buffer.from(data.split(",")[1], "base64"));
      await Bun.write(join(output, "water-motion.json"), JSON.stringify({ ...result, errors }, null, 2));
      console.log(
        JSON.stringify({
          output,
          cases: result.cases.map((c) => ({
            subject: c.subject,
            aa: c.antialiasing,
            reconstruction: c.measurements.reconstruction,
            waterReconstruction: c.measurements.waterReconstruction,
            maxFrozenFrameChange: Math.max(
              ...c.changes.filter((frame) => frame.phase === "frozen").map((frame) => frame.mean),
            ),
            maxMovingFrameChange: Math.max(
              ...c.changes.filter((frame) => frame.phase === "moving").map((frame) => frame.mean),
            ),
            errors: c.errors,
          })),
        }),
      );
      if (
        errors.length ||
        result.cases.some(
          (c) => c.errors.length || c.changes.some((frame) => frame.phase === "frozen" && frame.mean > 0.01),
        )
      )
        throw new Error("Stationary water view is not stable");
      if (
        result.cases.some((c) => !c.changes.some((frame) => frame.phase === "moving" && frame.mean > 0.005))
      )
        throw new Error("Water animation stopped");
      if (
        result.cases.some(
          (c) =>
            c.measurements.waterReconstruction !==
            (c.antialiasing === "spatial" ? "spatial" : "compact-history"),
        )
      )
        throw new Error("Water temporal history did not follow the reconstruction policy");
    } finally {
      server.stop(true);
    }
  },
  640,
  384,
  "chrome",
  600000,
  "visual-review",
);
