import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createLookdevFixture } from "./fixtures/lookdev";
import { vegetationStudies } from "./fixtures/vegetation-study";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--snapshot-run")) {
  const destination = resolve("output/vegetation-lookdev", `${Date.now()}-${process.pid}`, "source");
  await snapshotSource(destination, "vegetation-lookdev");
  const child = Bun.spawn(
    [process.execPath, "tools/vegetation-lookdev.ts", ...process.argv.slice(2), "--snapshot-run"],
    { cwd: destination, stdout: "inherit", stderr: "inherit" },
  );
  console.log(JSON.stringify({ snapshot: destination }));
  process.exit(await child.exited);
}
const filter = process.argv
  .find((argument) => argument.startsWith("--frame="))
  ?.slice(8)
  .split(",");
const steps = Number(process.argv.find((argument) => argument.startsWith("--steps="))?.slice(8) ?? 16);
if (!Number.isInteger(steps) || steps < 0 || steps > 64) throw Error("Growth steps must be 0..64");
const options = {
  steps,
  forest: process.argv.includes("--forest"),
  architecture: process.argv.includes("--architecture"),
  seed: Number(process.argv.find((a) => a.startsWith("--seed="))?.slice(7) ?? 73),
  age: Number(process.argv.find((a) => a.startsWith("--age="))?.slice(6) ?? 0.85),
  heldout: process.argv.includes("--heldout"),
  development: process.argv.includes("--growth=birch")
    ? ("paper-birch" as const)
    : process.argv.includes("--growth=pine")
      ? ("lodgepole-pine" as const)
      : undefined,
};
if (!Number.isInteger(options.seed) || !Number.isFinite(options.age) || options.age < 0 || options.age > 1)
  throw Error("Invalid specimen seed or maturity");
if (options.architecture && options.development) throw Error("Choose architecture or experimental growth");
const matrix = process.argv.includes("--matrix");
const variants = matrix
  ? [0.25, 0.55, 0.85].flatMap((age) => [73, 1009, 16847].map((seed) => ({ ...options, age, seed })))
  : [options];
const inputs = variants.flatMap((variant) =>
  vegetationStudies(variant)
    .map((study) => ({
      ...study,
      frames: study.frames
        .filter((frame) => (matrix ? frame.id === "plant-beauty" : !filter || filter.includes(frame.id)))
        .map((frame) => (matrix ? { ...frame, id: `pine-${variant.seed}-age-${variant.age}` } : frame)),
    }))
    .filter((study) => study.frames.length),
);
if (!inputs.length) throw new Error("No matching vegetation review frame");
type FrameResult = Awaited<ReturnType<Awaited<ReturnType<typeof createLookdevFixture>>["frame"]>>;
await withBrowser(
  async (view, output, errors) => {
    const frames = [];
    await Bun.write(join(output, "authored-project.json"), JSON.stringify(inputs[0].project, null, 2));
    for (const input of inputs) {
      await Bun.write(
        join(output, `${input.frames[0].id}-project.json`),
        JSON.stringify(input.project, null, 2),
      );
      const server = await fixtureServer(
        `import {createLookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/lookdev.ts"))};createLookdevFixture(${JSON.stringify(input)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
        output,
      );
      try {
        await view.navigate(String(server.url));
        await waitFor(view, "window.ready || window.failure", 60000);
        const failure = await view.evaluate("window.failure");
        if (failure) throw new Error(String(failure));
        for (let index = 0; index < input.frames.length; index++) {
          const { image, ...frame } = await view.evaluate<FrameResult>(`fixture.frame(${index})`);
          if (/swiftshader|llvmpipe|software/i.test(frame.measurements.adapter))
            throw new Error("Hardware adapter required");
          if (frame.id === "stand-far" && frame.measurements.detailSurfaces === 0)
            throw new Error("Far vegetation review did not exercise the distant representation");
          await Bun.write(join(output, `${frame.id}.png`), Buffer.from(image.split(",")[1], "base64"));
          frames.push(frame);
          console.log(JSON.stringify({ output, frame: frame.id, triangles: frame.triangles }));
        }
      } finally {
        await view.evaluate("window.fixture?.dispose()").catch(() => {});
        server.stop(true);
      }
    }
    if (errors.length) throw new Error(errors.join("\n"));
    await Bun.write(
      join(output, "vegetation-lookdev.json"),
      JSON.stringify(
        {
          frames,
          visualAcceptance: "requires-image-review",
          measurementScope: "Single captured frames; not a performance benchmark.",
        },
        null,
        2,
      ),
    );
  },
  process.argv.includes("--small") ? 640 : 1024,
  process.argv.includes("--small") ? 480 : 768,
);
