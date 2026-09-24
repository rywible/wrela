import { join, resolve } from "node:path";
import { pineArchitecture } from "@wrela/compiler/pine-architecture";
import { shapedPineLookdevDefinition } from "@wrela/examples";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createLookdevFixture, LookdevStudy } from "./fixtures/lookdev";
import { vegetationStudies } from "./fixtures/vegetation-study";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--snapshot-run")) {
  const destination = resolve("output/shoot-lookdev", `${Date.now()}-${process.pid}`, "source");
  await snapshotSource(destination, "shoot-lookdev");
  const child = Bun.spawn(
    [process.execPath, "tools/shoot-lookdev.ts", ...process.argv.slice(2), "--snapshot-run"],
    { cwd: destination, stdout: "inherit", stderr: "inherit" },
  );
  console.log(JSON.stringify({ snapshot: destination }));
  process.exit(await child.exited);
}
const all = process.argv.includes("--matrix"),
  plant = shapedPineLookdevDefinition();
const branch = pineArchitecture(plant).branches.find((b) => b.id === "b16/s2-0");
if (!branch) throw Error("Missing reference branch");
const center = branch.start.map((v, i) => (v + branch.end[i]) / 2) as [number, number, number];
const base = vegetationStudies({ architecture: true })[0];
const studies: LookdevStudy[] = (["triangles", "filtered", "legacy-filtered"] as const).map(
  (representation) => ({
    ...base,
    coniferReference: { branch: branch.id, representation },
    frames: (all ? [0, 1, 2] : [0]).flatMap((angle) =>
      (all ? [0, 1, 2] : [0]).map((light) => ({
        id: `${representation}-view${angle}-light${light}`,
        camera: {
          position: [
            center[0] + Math.sin(0.6 + angle * 2.094) * 1.05,
            center[1] + 0.35,
            center[2] + Math.cos(0.6 + angle * 2.094) * 1.05,
          ],
          target: center,
          fov: 40,
        },
        sunDirection: (
          [
            [0.5, 0.7, 0.5],
            [-0.5, 0.25, -0.84],
            [0.95, 0.1, -0.2],
          ] as [number, number, number][]
        )[light],
      })),
    ),
  }),
);
await withBrowser(
  async (view, output, errors) => {
    const frames = [];
    for (const study of studies) {
      const server = await fixtureServer(
        `import {createLookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/lookdev.ts"))};createLookdevFixture(${JSON.stringify(study)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
        output,
      );
      try {
        await view.navigate(String(server.url));
        await waitFor(view, "window.ready || window.failure", 60000);
        const failure = await view.evaluate("window.failure");
        if (failure) throw Error(String(failure));
        for (let i = 0; i < study.frames.length; i++) {
          const { image, ...frame } = await view.evaluate<
            Awaited<ReturnType<Awaited<ReturnType<typeof createLookdevFixture>>["frame"]>>
          >(`fixture.frame(${i})`);
          if (/swiftshader|llvmpipe|software/i.test(frame.measurements.adapter))
            throw Error("Hardware adapter required");
          await Bun.write(join(output, `${frame.id}.png`), Buffer.from(image.split(",")[1], "base64"));
          frames.push(frame);
          console.log(
            JSON.stringify({
              output,
              frame: frame.id,
              triangles: frame.triangles,
              reference: frame.coniferReference,
            }),
          );
        }
      } finally {
        await view.evaluate("window.fixture?.dispose()").catch(() => {});
        server.stop(true);
      }
    }
    await Bun.write(
      join(output, "shoot-lookdev.json"),
      JSON.stringify(
        {
          frames,
          errors,
          scope:
            "Unmatched real references; matched Wrela source/proxy cameras. Still images do not qualify motion.",
        },
        null,
        2,
      ),
    );
    if (errors.length) throw Error(errors.join("\n"));
  },
  1024,
  768,
);
