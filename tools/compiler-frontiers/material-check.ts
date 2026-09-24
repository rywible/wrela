import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {materialFixture} from ${JSON.stringify(resolve("tools/compiler-frontiers/material-fixture.ts"))};materialFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure");
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      if (process.argv.includes("--perspective")) {
        await view.evaluate("fixture.mode('compiled')");
        console.log(JSON.stringify(await view.evaluate("fixture.perspectiveCheck()")));
        console.log(errors);
        return;
      }
      const images: unknown[] = [],
        timings: unknown[] = [],
        perspective: unknown[] = [];
      for (const mode of ["reference-fine", "compiled", "point", "reference"]) {
        await view.evaluate(`fixture.mode('${mode}')`);
        for (let index = 0; index < 24; index++) {
          const result = await view.evaluate(
            `fixture.image(${index},'${mode === "reference-fine" ? "reference" : mode === "compiled" ? "compiled" : "compare"}')`,
          );
          images.push({ mode, index, result });
          if (index === 10) await Bun.write(join(output, `weave-${mode}.png`), await view.screenshot());
          if (index >= 8 && index < 16 && (mode === "point" || mode === "compiled"))
            await Bun.write(join(output, `motion-${mode}-${index}.png`), await view.screenshot());
        }

        if (mode === "compiled" || mode === "point")
          perspective.push({ mode, result: await view.evaluate("fixture.perspectiveCheck()") });
      }
      await view.resize(1920, 1080);
      for (let trial = 0; trial < 3; trial++) {
        const modes = ["compiled", "point", "reference"];
        for (let offset = 0; offset < 3; offset++) {
          const mode = modes[(trial + offset) % 3];
          await view.evaluate(`fixture.mode('${mode}')`);
          timings.push({ trial, mode, result: await view.evaluate("fixture.measure()") });
        }
      }
      await view.evaluate("fixture.mode('compiled')");
      const rebase = await view.evaluate("fixture.rebase()");
      const result = { output, images, timings, perspective, rebase, errors };
      await Bun.write(join(output, "material.json"), JSON.stringify(result, null, 2));
      await Bun.write("output/compiler-frontiers/material-latest.json", JSON.stringify(result, null, 2));
      console.log(JSON.stringify({ output, rebase, errors }));
      if (errors.length) throw Error(errors.join("\n"));
      for (const row of images as { mode: string; result?: { imageError: { relativeL2: number } } }[])
        if (row.mode === "compiled" && (!row.result || row.result.imageError.relativeL2 > 0.001))
          throw Error("Compiled weave image error");
      for (const row of perspective as {
        mode: string;
        result: { error: { relativeL2: number }; convergence: { relativeL2: number } }[];
      }[])
        if (
          row.mode === "compiled" &&
          row.result.some((r) => r.error.relativeL2 > 0.006 || r.convergence.relativeL2 > 0.0002)
        )
          throw Error("Perspective reference error");
      if ((rebase as { maximum: number }).maximum !== 0)
        throw Error("World phase changed after origin rebase");
    } finally {
      await view.evaluate("fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  640,
  360,
);
