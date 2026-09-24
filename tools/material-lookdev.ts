import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createMaterialLookdevFixture } from "./fixtures/material-lookdev";

const families = process.argv.includes("--families");

type Capture = Awaited<ReturnType<Awaited<ReturnType<typeof createMaterialLookdevFixture>>["capture"]>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createMaterialLookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/material-lookdev.ts"))};createMaterialLookdevFixture(${families}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const reports = [];
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      for (const light of ["neutral", "grazing", "backlit"] as const) {
        const capture = await view.evaluate<Capture>(`fixture.capture(${JSON.stringify(light)},14)`);
        const { image, ...report } = capture;
        reports.push(report);
        await Bun.write(join(output, `materials-${light}.png`), Buffer.from(image.split(",")[1], "base64"));
        if (!capture.completeness.complete || capture.failures.length) throw Error(JSON.stringify(report));
      }
      const distant = await view.evaluate<Capture>('fixture.capture("neutral",28)');
      await Bun.write(
        join(output, "materials-distance.png"),
        Buffer.from(distant.image.split(",")[1], "base64"),
      );
      if (!distant.completeness.complete || distant.failures.length)
        throw Error("Incomplete distant material capture");
      if (families) {
        for (const mode of ["beauty", "albedo", "roughness", "metallic"] as const) {
          const coated = await view.evaluate<Capture>(
            `fixture.capture("neutral",14,true,${JSON.stringify(mode)})`,
          );
          const { image, ...coatedReport } = coated;
          reports.push(coatedReport);
          await Bun.write(
            join(output, `materials-coated-${mode}.png`),
            Buffer.from(image.split(",")[1], "base64"),
          );
          if (!coated.completeness.complete || coated.failures.length)
            throw Error(JSON.stringify(coatedReport));
        }
      }
      const { image: _image, ...report } = distant;
      reports.push(report);
      await Bun.write(
        join(output, "material-lookdev.json"),
        JSON.stringify(
          {
            reports,
            errors,
            order: "Left to right, top to bottom",
            artisticApproval: "requires visual review",
          },
          null,
          2,
        ),
      );
      if (errors.length) throw Error(errors.join("\n"));
      console.log(JSON.stringify({ output, captures: reports.length, adapter: distant.adapter }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  960,
  800,
);
