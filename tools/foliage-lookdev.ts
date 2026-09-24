import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createFoliageLookdevFixture } from "./fixtures/foliage-lookdev";

type Fixture = Awaited<ReturnType<typeof createFoliageLookdevFixture>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createFoliageLookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/foliage-lookdev.ts"))};createFoliageLookdevFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const captures = [];
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      for (const light of ["front", "back", "grazing"])
        for (const distance of [1.65, 5]) {
          const capture = await view.evaluate<Awaited<ReturnType<Fixture["capture"]>>>(
            `fixture.capture(${JSON.stringify(light)},${distance})`,
          );
          const { image, ...metadata } = capture;
          captures.push(metadata);
          await Bun.write(
            join(output, `foliage-${light}-${distance === 1.65 ? "near" : "far"}.png`),
            Buffer.from(image.split(",")[1], "base64"),
          );
        }
      const reverse = await view.evaluate<Awaited<ReturnType<Fixture["capture"]>>>(
        'fixture.capture("front",1.65,true)',
      );
      const { image, ...metadata } = reverse;
      captures.push(metadata);
      await Bun.write(join(output, "foliage-reverse-face.png"), Buffer.from(image.split(",")[1], "base64"));
      const probes = await view.evaluate<Awaited<ReturnType<Fixture["check"]>>>("fixture.check()");
      await Bun.write(
        join(output, "foliage-lookdev.json"),
        JSON.stringify(
          {
            captures,
            probes,
            errors,
            visualApproval: "Requires visual inspection; probe acceptance is not AAA art approval.",
          },
          null,
          2,
        ),
      );
      if (errors.length) throw Error(errors.join("\n"));
      console.log(JSON.stringify({ output, captures: captures.length, probes }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  960,
  640,
);
