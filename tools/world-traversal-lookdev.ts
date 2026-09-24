import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import type { createWorldTraversalFixture } from "./fixtures/world-traversal-lookdev";
import { createWorldTraversalStudy, traversalSummary } from "./world-traversal-study";

const argument = (name: string, fallback: string) =>
  process.argv.find((arg) => arg.startsWith(`--${name}=`))?.slice(name.length + 3) ?? fallback;
const count = Number(argument("frames", "48"));
const input = createWorldTraversalStudy(count);
const small = process.argv.includes("--small");
const size = small ? [640, 480] : [1024, 768];
if (process.argv.includes("--include-stills")) {
  const child = Bun.spawn(
    [process.execPath, "tools/lookdev.ts", "--study=scene", ...(small ? ["--small"] : [])],
    { stdout: "inherit", stderr: "inherit" },
  );
  if ((await child.exited) !== 0) throw new Error("World overview capture failed");
}
type Fixture = Awaited<ReturnType<typeof createWorldTraversalFixture>>;
type Frame = Awaited<ReturnType<Fixture["frame"]>>;
type Settled = Awaited<ReturnType<Fixture["settle"]>>;
await withBrowser(
  async (view, output, errors) => {
    await Bun.write(join(output, "authored-project.json"), JSON.stringify(input.project, null, 2));
    const server = await fixtureServer(
      `import {createWorldTraversalFixture} from ${JSON.stringify(resolve("tools/fixtures/world-traversal-lookdev.ts"))};createWorldTraversalFixture(${JSON.stringify(input)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw new Error(String(failure));
      const frames: (Frame & Settled & { image: string; initialImage?: string })[] = [];
      const sheet: { id: string; image: string }[] = [];
      const sheetFrames = new Set(
        Array.from({ length: 8 }, (_, index) => Math.round((index * (count - 1)) / 7)),
      );
      for (let index = 0; index < count; index++) {
        const first = await view.evaluate<Frame>(`fixture.frame(${index})`);
        let initialImage: string | undefined;
        if (!first.initialComplete || !first.initialStreamingReady) {
          initialImage = `${first.id}-initial.png`;
          await Bun.write(join(output, initialImage), await view.screenshot());
        }
        const settled = await view.evaluate<Settled>("fixture.settle()");
        if (/swiftshader|llvmpipe|software/i.test(settled.measurements.adapter))
          throw new Error("Hardware GPU required");
        const blob = await view.screenshot();
        const image = `${first.id}.png`;
        await Bun.write(join(output, image), blob);
        frames.push({ ...first, ...settled, image, initialImage });
        if (sheetFrames.has(index))
          sheet.push({
            id: `${first.id} · ${first.time.toFixed(2)} s`,
            image: `data:image/png;base64,${Buffer.from(await blob.arrayBuffer()).toString("base64")}`,
          });
      }
      const summary = traversalSummary(frames);
      const diagnostics = frames.flatMap((frame) =>
        frame.diagnostics.filter((issue) => issue.severity === "error"),
      );
      const passed =
        summary.collisionReady &&
        summary.blockedFrames === 0 &&
        summary.completeFrames === count &&
        summary.streamingActivationObserved &&
        !diagnostics.length &&
        !errors.length;
      const contactSheet = await view.evaluate<string>(`fixture.contactSheet(${JSON.stringify(sheet)})`);
      await Bun.write(
        join(output, "traversal-contact-sheet.png"),
        Buffer.from(contactSheet.split(",")[1], "base64"),
      );
      await Bun.write(
        join(output, "world-traversal.json"),
        JSON.stringify(
          {
            passed,
            scope:
              "One persistent runtime and renderer follow an authored corridor at 3 m/s with 60 Hz simulation. Frames are spaced samples, not real-time FPS. Initial readiness and images preserve observed streaming/upload gaps; bounded settling is measured separately before final captures. Capsule clearance is approximated by three physics overlap spheres above allowed step height, plus installed-ground rays; this does not replace a controllable player test.",
            source: "authored-project.json",
            duration: input.duration,
            actor: input.actor,
            summary,
            frames,
            errors,
            visualAcceptance: "requires-image-review",
          },
          null,
          2,
        ),
      );
      console.log(JSON.stringify({ output, passed, ...summary }));
      if (!passed) process.exitCode = 1;
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  size[0],
  size[1],
);
