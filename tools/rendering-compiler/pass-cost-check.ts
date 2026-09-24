import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";
import type { AcceptanceFixture } from "./fixture";
import { quantiles, trialOrder } from "./timing";

// Repeating an otherwise identical pass measures its marginal cost without
// attributing overlapping timestamp spans as exclusive work. Diagnostic only.
const hook = `
window.probe={label:'',repeat:1};
const pipeline=GPUDevice.prototype.createRenderPipelineAsync;
GPUDevice.prototype.createRenderPipelineAsync=function(d){if(d.label?.startsWith('Water '))d={...d,depthStencil:{...d.depthStencil,depthCompare:'less-equal'}};return pipeline.call(this,d);};
for(const method of ['beginRenderPass','beginComputePass']){
 const begin=GPUCommandEncoder.prototype[method];
 GPUCommandEncoder.prototype[method]=function(d){
  const count=d.label===window.probe.label?window.probe.repeat:1;
  if(count===1)return begin.call(this,d);
  const encoder=this,stamps=d.timestampWrites;
  const first=begin.call(encoder,{...d,timestampWrites:stamps?{querySet:stamps.querySet,beginningOfPassWriteIndex:stamps.beginningOfPassWriteIndex}:undefined});
  const calls=[];
  return new Proxy(first,{get(target,key){
   if(key==='end')return()=>{target.end();for(let i=1;i<count;i++){
    const pass=begin.call(encoder,{...d,timestampWrites:stamps&&i===count-1?{querySet:stamps.querySet,endOfPassWriteIndex:stamps.endOfPassWriteIndex}:undefined});
    for(const [name,args] of calls)pass[name](...args);pass.end();
   }};
   const member=target[key];if(typeof member!=='function')return member;
   return(...args)=>{calls.push([key,args]);return member.apply(target,args);};
  }});
 };
}
`;
const labels = [
  "Shadow pass",
  "Scene linear color and depth",
  "Water reflection, refraction and absorption",
  "Temporal reconstruction",
];
const frames = 41;
const paced = !process.argv.includes("--saturated");
const burst = Number(process.argv.find((arg) => arg.startsWith("--burst="))?.slice(8) ?? 1);
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `${hook}\nimport {createAcceptanceFixture} from ${JSON.stringify(resolve("tools/rendering-compiler/fixture.ts"))};createAcceptanceFixture().then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
      output,
    );
    const results = [];
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready || window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      await view.evaluate(`fixture.prepare('winter-valley',${frames})`);
      // One renderer keeps compiled pipelines, allocations and shader code identical.
      await view.evaluate("fixture.beginVariant('production')");
      for (let trial = 0; trial < 3; trial++)
        for (const label of trialOrder(labels, trial))
          for (const repeat of trial % 2 ? [4, 1] : [1, 4]) {
            await view.evaluate(`window.probe=${JSON.stringify({ label, repeat })}`);
            await view.evaluate(`fixture.run(true,${frames},false,${paced},${burst})`);
            const run = await view.evaluate<Awaited<ReturnType<AcceptanceFixture["run"]>>>(
              `fixture.run(false,${frames},false,${paced},${burst})`,
            );
            results.push({ trial, label, repeat, ...run });
            await Bun.write(
              join(output, "pass-cost.json"),
              JSON.stringify(
                {
                  resolution: [1920, 1080],
                  paced,
                  burst,
                  results,
                  errors,
                  limitations: [
                    "Pass repetition is a diagnostic workload, not normal game performance.",
                    "Water uses less-equal depth in both conditions so repeated water still executes fragment shading.",
                    "Marginal costs include GPU scheduling and frequency changes; they are estimates, not additive exclusive times.",
                  ],
                },
                null,
                2,
              ),
            );
            if (run.gpu.length !== frames || run.diagnostics.some((d) => d.severity === "error"))
              throw Error("Invalid pass probe");
            console.log(
              JSON.stringify({ trial, label, repeat, gpu: quantiles(run.gpu.map((s) => s.gpuMs)) }),
            );
          }
      console.log(JSON.stringify({ output, errors }));
      if (errors.length) throw Error(errors.join("\n"));
    } finally {
      await view.evaluate("fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  1920,
  1080,
);
