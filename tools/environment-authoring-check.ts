import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { environmentAuthoringFixture } from "./fixtures/environment-authoring";

type Report = Awaited<ReturnType<Awaited<ReturnType<typeof environmentAuthoringFixture>>["check"]>>;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {environmentAuthoringFixture} from ${JSON.stringify(resolve("tools/fixtures/environment-authoring.ts"))};environmentAuthoringFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 30000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const { clear, storm, ...report } = await view.evaluate<Report>("fixture.check()");
      await Bun.write(join(output, "environment-clear.png"), Buffer.from(clear.split(",")[1], "base64"));
      await Bun.write(join(output, "environment-storm.png"), Buffer.from(storm.split(",")[1], "base64"));
      await Bun.write(join(output, "environment-authoring.json"), JSON.stringify(report, null, 2));
      if (errors.length || report.errors.length) throw Error([...errors, ...report.errors].join("\n"));
      if (
        !report.complete ||
        report.delta < 0.001 ||
        report.rebaseDelta > 0.005 ||
        report.wetness !== 1 ||
        report.resolvedFlow !== null ||
        report.riverVertices < 20
      )
        throw Error("Authored environment acceptance failed");
      if (/swiftshader|llvmpipe|software/i.test(report.adapter))
        throw Error("Software adapter cannot establish hardware evidence");
      console.log(JSON.stringify({ output, ...report }));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  480,
  320,
);
