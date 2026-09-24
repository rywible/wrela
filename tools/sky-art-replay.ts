import { dirname, join, resolve } from "node:path";
import { waitFor, withBrowser } from "./browser";
import type { createLookdevFixture, LookdevStudy } from "./fixtures/lookdev";

// Replay a frozen lookdev scene while unrelated renderer work continues.
// Visual-only access deliberately makes no comparable performance measurements.
const argument = process.argv.find((arg) => arg.startsWith("--bundle="))?.slice(9);
if (!argument) throw Error("Expected --bundle=<fixture.js>");
const path = resolve(argument);
let bundle = await Bun.file(path).text();
const originalBundleSha256 = new Bun.CryptoHasher("sha256").update(bundle).digest("hex");
// Substituting shader/study source invalidates the frozen bundle's inline map.
bundle = bundle.replace(/^\/\/# (?:sourceMappingURL|debugId)=.*$/gm, "");
const cloudPath = process.argv.find((arg) => arg.startsWith("--cloud-shader="))?.slice(15);
const cloudShader = cloudPath ? await Bun.file(resolve(cloudPath)).text() : undefined;
if (cloudShader !== undefined) {
  const declaration = /var atmosphere_clouds_default = `(?:\\[\s\S]|[^`])*`;/g;
  if ([...bundle.matchAll(declaration)].length !== 1)
    throw Error("Expected one frozen cloud shader declaration");
  bundle = bundle.replace(
    declaration,
    () => `var atmosphere_clouds_default = ${JSON.stringify(cloudShader)};`,
  );
}
const studyPath = process.argv.find((arg) => arg.startsWith("--study="))?.slice(8);
const study: LookdevStudy = await Bun.file(
  studyPath ? resolve(studyPath) : join(dirname(path), "authored-study.json"),
).json();
if (studyPath) {
  const start = bundle.lastIndexOf("\ncreateLookdevFixture(");
  const end = bundle.indexOf(").then(", start);
  if (start < 0 || end < 0) throw Error("Frozen lookdev entry point was not found");
  bundle = `${bundle.slice(0, start)}\ncreateLookdevFixture(${JSON.stringify(study)}${bundle.slice(end)}`;
}
const selected = process.argv
  .find((arg) => arg.startsWith("--frames="))
  ?.slice(9)
  .split(",");
if (selected?.some((id) => !study.frames.some((frame) => frame.id === id)))
  throw Error("Unknown review frame");
type Frame = Awaited<ReturnType<Awaited<ReturnType<typeof createLookdevFixture>>["frame"]>>;

await withBrowser(
  async (view, output, errors) => {
    await Bun.write(join(output, "fixture.js"), bundle);
    if (cloudShader !== undefined) await Bun.write(join(output, "cloud-shader.wgsl"), cloudShader);
    await Bun.write(join(output, "authored-study.json"), JSON.stringify(study, null, 2));
    const server = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      fetch(req) {
        return new URL(req.url).pathname === "/fixture.js"
          ? new Response(bundle, { headers: { "Content-Type": "text/javascript" } })
          : new Response(
              '<!doctype html><style>html,body{margin:0;width:100%;height:100%}canvas{width:100%;height:100%;display:block}</style><canvas id="viewport"></canvas><script type="module" src="/fixture.js"></script>',
              { headers: { "Content-Type": "text/html; charset=utf-8" } },
            );
      },
    });
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 180000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      const frames = [];
      for (let i = 0; i < study.frames.length; i++) {
        if (selected && !selected.includes(study.frames[i].id)) continue;
        const frame = await view.evaluate<Frame>(`fixture.frame(${i})`);
        if (!frame.complete) throw Error(`Incomplete frame: ${frame.id}`);
        await Bun.write(join(output, `${frame.id}.png`), Buffer.from(frame.image.split(",")[1], "base64"));
        frames.push({ id: frame.id, complete: frame.complete, adapter: frame.measurements.adapter });
      }
      if (errors.length) throw Error(errors.join("\n"));
      const result = {
        output,
        bundle: path,
        originalBundleSha256,
        bundleSha256: new Bun.CryptoHasher("sha256").update(bundle).digest("hex"),
        cloudShaderSha256:
          cloudShader === undefined
            ? undefined
            : new Bun.CryptoHasher("sha256").update(cloudShader).digest("hex"),
        performanceComparable: false,
        frames,
        errors,
      };
      await Bun.write(join(output, "sky-art-review.json"), JSON.stringify(result, null, 2));
      console.log(JSON.stringify(result));
    } finally {
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  1440,
  900,
  "chrome",
  600_000,
  "visual-review",
);
