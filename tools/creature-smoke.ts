import { join, resolve } from "node:path";
import { type EvaluatedScene, VIEW_MODES } from "@wrela/model";

import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createCreatureRenderFixture } from "./fixtures/creature-render";

const id = process.argv.includes("--biped") ? "reed-penitent" : "ash-warden";
const motion = process.argv.find((arg) => arg.startsWith("--motion="))?.slice(9) ?? "idle";
const time = Number(process.argv.find((arg) => arg.startsWith("--time="))?.slice(7) ?? 0);
const viewName = process.argv.find((arg) => arg.startsWith("--view="))?.slice(7) ?? "three-quarter";
const mode = (process.argv.find((arg) => arg.startsWith("--mode="))?.slice(7) ??
  "beauty") as EvaluatedScene["mode"];
const hideGroom = process.argv.includes("--hide-groom");
const skeleton = process.argv.includes("--skeleton");
const width = Number(process.argv.find((arg) => arg.startsWith("--width="))?.slice(8) ?? 320);
const height = Math.round((width * 9) / 16);
if (!VIEW_MODES.includes(mode) || !Number.isInteger(width) || width < 160 || width > 640)
  throw Error("Use a known mode and160–640px width");
if (!Number.isFinite(time) || time < 0 || time > 10)
  throw new Error("Use a capture time from 0 to 10 seconds");
type Capture = Awaited<ReturnType<Awaited<ReturnType<typeof createCreatureRenderFixture>>["capture"]>>;

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createCreatureRenderFixture} from ${JSON.stringify(resolve("tools/fixtures/creature-render.ts"))};createCreatureRenderFixture(${JSON.stringify(id)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 30000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw new Error(String(failure));
      const capture = await view.evaluate<Capture>(
        `fixture.capture(${JSON.stringify(motion)},${time},${JSON.stringify(viewName)},${JSON.stringify(mode)},${hideGroom},${skeleton})`,
      );
      const { image, ...report } = capture;
      await Bun.write(join(output, `${id}-${mode}.png`), Buffer.from(image.split(",")[1], "base64"));
      await Bun.write(join(output, "creature-smoke.json"), JSON.stringify(report, null, 2));
      if (errors.length) throw new Error(errors.join("\n"));
      if (!capture.completeness.complete) throw new Error("Creature smoke omitted requested content");
      if (/swiftshader|llvmpipe|software/i.test(capture.measurements.adapter))
        throw Error("Software rendering is not hardware evidence");
      if (capture.diagnostics.some((diagnostic) => diagnostic.severity === "error"))
        throw Error("Creature capture contains error diagnostics; inspect the saved report");
      console.log(
        JSON.stringify({ output, id, source: capture.source, complete: capture.completeness.complete }),
      );
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  width,
  height,
);
