import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";

await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {checkMaterialLatticeGpu} from ${JSON.stringify(resolve("tools/rendering-compiler/material-lattice-fixture.ts"))};import {createAcceptanceFixture} from ${JSON.stringify(resolve("tools/rendering-compiler/fixture.ts"))};Promise.all([checkMaterialLatticeGpu(),createAcceptanceFixture()]).then(([r,f])=>{window.result=r;window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const kernel = await view.evaluate("window.result");
      await view.evaluate("fixture.prepare('winter-valley',3)");
      const images = [];
      for (const frame of [0, 1, 2]) {
        await view.evaluate(
          "fixture.beginVariant('production',{},undefined,'spatial',1,{materialCache:false})",
        );
        await view.evaluate(`fixture.compareMaterial(${frame})`);
        await view.evaluate(
          "fixture.beginVariant('production',{},undefined,'spatial',1,{materialCache:true})",
        );
        const comparison = await view.evaluate<{ maximum: number; relativeRms: number }>(
          `fixture.compareMaterial(${frame})`,
        );
        images.push({ frame, comparison });
        if (comparison.maximum > 0.005)
          throw Error(`Cached material image changed: ${JSON.stringify(comparison)}`);
      }
      await Bun.write(join(output, "materials.json"), JSON.stringify({ kernel, images, errors }, null, 2));
      console.log(JSON.stringify({ output, kernel, images, errors }));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  640,
  360,
);
