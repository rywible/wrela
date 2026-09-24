import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";
import type { AcceptanceFixture } from "./fixture";
import { quantiles, trialOrder } from "./timing";

const full = process.argv.includes("--full");
const resolution = full ? [1920, 1080] : [640, 360];
const paced = !process.argv.includes("--saturated");
const serial = process.argv.includes("--serial");
const presentation = process.argv.includes("--presentation");
const material = process.argv.includes("--material");
const reconstruction = process.argv.includes("--reconstruction");
const controls = process.argv.includes("--controls");
const frames = 121;
const burst = Number(process.argv.find((arg) => arg.startsWith("--burst="))?.slice(8) ?? 1);
const scale = Number(process.argv.find((arg) => arg.startsWith("--scale="))?.split("=")[1] ?? 1);
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `
window.experiment="temporal";
const original=GPUDevice.prototype.createShaderModule;
const configuration=new WeakMap();const targets=new WeakMap();
const configure=GPUCanvasContext.prototype.configure,getTexture=GPUCanvasContext.prototype.getCurrentTexture;
GPUCanvasContext.prototype.configure=function(d){configuration.set(this,d);targets.get(this)?.destroy();targets.delete(this);return configure.call(this,d);};
GPUCanvasContext.prototype.getCurrentTexture=function(){if(window.experiment!=="offscreen")return getTexture.call(this);const d=configuration.get(this);let t=targets.get(this);if(!t||t.width!==this.canvas.width||t.height!==this.canvas.height){t?.destroy();t=d.device.createTexture({size:[this.canvas.width,this.canvas.height],format:d.format,usage:d.usage});targets.set(this,t);}return t;};
function replaceBody(code,name,body){const begin=code.indexOf("fn "+name+"(");if(begin<0)return code;const open=code.indexOf("{",begin);let end=open+1,depth=1;while(depth&&end<code.length){if(code[end]=="{")depth++;if(code[end]=="}")depth--;end++;}return code.slice(0,open+1)+body+code.slice(end-1);}
GPUDevice.prototype.createShaderModule=function(desc){let code=desc.code;
if(window.experiment==="compiled-off")code=replaceBody(code,"compiledWaterResponse","var r:CompiledWaterResult;r.valid=false;return r;");
if(window.experiment==="transport-off")code=replaceBody(code,"waterTransportWithSky","return integratedSky+base*0.1;");
return original.call(this,{...desc,code});};
import {createAcceptanceFixture} from ${JSON.stringify(resolve("tools/rendering-compiler/fixture.ts"))};createAcceptanceFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const results = [];
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      await view.evaluate(`fixture.prepare('winter-valley',${frames})`);
      for (let trial = 0; trial < 3; trial++) {
        for (const name of trialOrder(
          material
            ? ["temporal", "uncached"]
            : reconstruction
              ? ["temporal", "compute"]
              : presentation
                ? ["temporal", "offscreen"]
                : controls
                  ? ["temporal", "compiled-off", "transport-off"]
                  : ["temporal", "spatial"],
          trial,
        )) {
          await view.evaluate(
            `(window.experiment=${JSON.stringify(name)},fixture.beginVariant('production',{},undefined,${JSON.stringify(name === "spatial" ? "spatial" : "temporal")},${scale},{materialCache:${material && name !== "uncached"},temporalResolve:${JSON.stringify(name === "compute" ? "compute" : "storage")}}))`,
          );
          await view.evaluate(`fixture.run(true,121,false,${paced},${burst})`);
          const run = await view.evaluate<Awaited<ReturnType<AcceptanceFixture["run"]>>>(
            `fixture.run(false,121,${serial},${paced},${burst})`,
          );
          if (
            run.diagnostics.some((d) => d.severity === "error") ||
            run.gpu.length !== frames ||
            run.gpu.some((s) => s.gpuMs <= 0)
          )
            throw Error("Invalid GPU run");
          const summary = {
            trial,
            name,
            gpu: quantiles(run.gpu.map((s) => s.gpuMs)),
            cpu: quantiles(run.cpu),
            pacing: quantiles(run.pacing),
            completedFramesPerSecond: (frames * 1000) / run.elapsedMs,
          };
          results.push({ ...summary, ...run });
          console.log(JSON.stringify(summary));
          // Preserve the actual temporal presentation, without capture() resetting its history.
          await Bun.write(join(output, `${name}-${trial}.png`), await view.screenshot());
          await Bun.write(
            join(output, "performance.json"),
            JSON.stringify(
              { resolution, resolutionScale: scale, serialGpu: serial, results, errors },
              null,
              2,
            ),
          );
        }
      }
      console.log(JSON.stringify({ output, errors }));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  resolution[0],
  resolution[1],
);
