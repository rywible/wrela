import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { vegetationStudies } from "./fixtures/vegetation-study";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--snapshot-run")) {
  const destination = resolve("output/vegetation-material-review", `${Date.now()}-${process.pid}`, "source");
  await snapshotSource(destination, "vegetation-material-review");
  const child = Bun.spawn([process.execPath, "tools/vegetation-material-review.ts", "--snapshot-run"], {
    cwd: destination,
    stdout: "inherit",
    stderr: "inherit",
  });
  console.log(JSON.stringify({ snapshot: destination }));
  process.exit(await child.exited);
}
const study = vegetationStudies().find((study) =>
  study.frames.some((frame) => frame.id === "stand-gameplay"),
);
if (!study) throw Error("Fixture");
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createVegetationBenchmark} from ${JSON.stringify(resolve("tools/fixtures/vegetation-benchmark.ts"))};
window.results=[];window.pixels=[];
window.capture=async function(specialized){const fixture=await createVegetationBenchmark(${JSON.stringify(study)},${study.frames.findIndex((f) => f.id === "stand-gameplay")},{antialiasing:"msaa",materialSpecialization:specialized});try {await fixture.batch(12);const image=await fixture.capture();const bitmap=await createImageBitmap(await (await fetch(image)).blob());const canvas=new OffscreenCanvas(bitmap.width,bitmap.height);const context=canvas.getContext("2d");context.drawImage(bitmap,0,0);window.pixels.push(context.getImageData(0,0,bitmap.width,bitmap.height).data);return image;}finally{fixture.dispose();}};
window.compare=()=>{let sum=0,maximum=0,changed=0;const [a,b]=window.pixels;for(let i=0;i<a.length;i++){if(i%4===3)continue;const d=Math.abs(a[i]-b[i]);sum+=d*d;maximum=Math.max(maximum,d);if(d)changed++;}return {channels:a.length*3/4,rms:Math.sqrt(sum/(a.length*3/4)),maximum,changed};};window.ready=true;`,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready", 60000);
      for (const specialized of [false, true]) {
        const image = await view.evaluate<string>(`window.capture(${specialized})`);
        await Bun.write(
          join(output, specialized ? "specialized.png" : "generic.png"),
          Buffer.from(image.split(",")[1], "base64"),
        );
      }
      const metrics = await view.evaluate<{
        channels: number;
        rms: number;
        maximum: number;
        changed: number;
      }>("window.compare()");
      await Bun.write(
        join(output, "comparison.json"),
        JSON.stringify(
          {
            metrics,
            errors,
            scope:
              "Fixed source, camera and wind; 1080p MSAA display-encoded comparison. Lighting equations unchanged.",
          },
          null,
          2,
        ),
      );
      console.log(JSON.stringify({ output, metrics, errors }));
      if (metrics.rms > 0.5 || metrics.maximum > 8 || errors.length)
        throw Error("Material specialization exceeded image tolerance");
    } finally {
      server.stop(true);
    }
  },
  1920,
  1080,
);
