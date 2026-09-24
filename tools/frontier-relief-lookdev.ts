import { join, resolve } from "node:path";
import type { FrontierPolicy } from "@wrela/examples/render-frontier";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createFrontierReliefFixture } from "./fixtures/frontier-relief";
import { writeFrontierLookdevReport } from "./frontier-lookdev-report";

type Fixture = Awaited<ReturnType<typeof createFrontierReliefFixture>>;
const sampleArgument = process.argv.find((value) => value.startsWith("--samples="))?.slice(10);
const samples = Number(sampleArgument ?? 30);
if (!Number.isInteger(samples) || samples < 30 || samples > 120 || samples % 5)
  throw Error("Use 30–120 samples, divisible by five views");
const policy: FrontierPolicy = {
  budgets: {
    "linear-radiance-rms": 0.025,
    "linear-radiance-max": 0.25,
    "silhouette-max-pixels": 1,
    "coverage-rms": 0.02,
    "coverage-max": 1,
    "temporal-rms": 0.03,
    "temporal-max": 0.35,
  },
  costAxes: ["gpuMs", "cpuMs", "ownedBytes"],
};
const size = process.argv.includes("--small") ? [320, 240] : [640, 480];
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createFrontierReliefFixture} from ${JSON.stringify(resolve("tools/fixtures/frontier-relief.ts"))};createFrontierReliefFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const result = await view.evaluate<Awaited<ReturnType<Fixture["run"]>>>(`fixture.run(${samples})`);
      if (/swiftshader|software|llvmpipe/i.test(result.rendererMeasurements.adapter))
        throw Error("Hardware rendering required");
      const imageEvidence = [];
      for (let index = 0; index < result.imageCount; index++) {
        const evidence = await view.evaluate<ReturnType<Fixture["evidence"]>>(`fixture.evidence(${index})`);
        const { image, hdrBase64, ...metadata } = evidence;
        const stem = `${evidence.choice}-view-${evidence.view}-${evidence.mode}`;
        const bytes = Buffer.from(hdrBase64, "base64");
        await Bun.write(join(output, `${stem}.png`), Buffer.from(image.split(",")[1], "base64"));
        await Bun.write(join(output, `${stem}.rgba32f`), bytes);
        imageEvidence.push({
          ...metadata,
          image: `${stem}.png`,
          linearPixels: `${stem}.rgba32f`,
          bytes: bytes.byteLength,
          sha256: new Bun.CryptoHasher("sha256").update(bytes).digest("hex"),
        });
      }
      await Bun.write(join(output, "source.json"), JSON.stringify(result.project, null, 2));
      await Bun.write(
        join(output, "measurements.json"),
        JSON.stringify({ ...result, imageEvidence, policy, errors }, null, 2),
      );
      const report = await writeFrontierLookdevReport(
        {
          schemaVersion: 1,
          policy,
          candidates: result.candidates,
          notes: [
            ...result.notes,
            "Quality limits were fixed in the fixture before capture. They are bounded research comparison gates, not artistic AAA thresholds.",
            "Raw scene-linear RGBA32F files, native silhouette masks, image hashes, frame-tagged timing observations and exact source are retained beside this report.",
          ],
        },
        output,
        output,
      );
      if (errors.length || result.failures.length) throw Error([...errors, ...result.failures].join("\n"));
      console.log(
        JSON.stringify({
          ...report,
          images: imageEvidence.length,
          gpuSamplesPerCandidate: samples,
          artisticAcceptance: "unreviewed",
        }),
      );
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  size[0],
  size[1],
);
