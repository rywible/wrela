import { join, resolve } from "node:path";
import { AuthoringSession, authoringTaskContext, domainQuality, planDomainAuthoring } from "@wrela/authoring";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { transferTimber, transferTree } from "./fixtures/authoring-transfer";

export async function checkReviewCache() {
  const cases = [transferTimber(), transferTree()].map((baseline, i) => {
    const plan = planDomainAuthoring(
      baseline,
      {
        domain: i ? "vegetation" : "timber",
        target: baseline.entry,
        conditions: { exposure: 0.8, moisture: 0.5, maturity: 0.9, variation: 0.7 },
        quality: domainQuality(i ? "vegetation" : "timber"),
      },
      "warm-check",
    );
    const session = new AuthoringSession(baseline);
    session.apply({ expectedRevision: 0, operations: plan.operations });
    const context = authoringTaskContext(baseline, baseline.entry);
    return {
      baseline,
      candidate: session.getSnapshot().project,
      constraints: plan.constraints,
      settings: {
        target: baseline.entry,
        width: 640,
        height: 480,
        views: [
          { ...context.views[1], rig: "neutral" },
          { ...context.views.at(-1), rig: "grazing" },
        ],
      },
    };
  });
  return withBrowser(async (view, output, errors) => {
    const server = await fixtureServer(
      `
      import {reviewAuthoringPacket} from ${JSON.stringify(resolve("packages/review/src/packet.ts"))};
      import {reviewAuthoringStudy} from ${JSON.stringify(resolve("packages/review/src/study.ts"))};
      import {encodeCapturePixels} from ${JSON.stringify(resolve("packages/render-webgpu/src/capture-encoding.ts"))};
      const blobs=new Map();let n=0;
      const save=async(name,value)=>{const key=(n++)+'-'+name;if(value instanceof Blob)blobs.set(key,value);return key};
      const pixels=async(ref)=>{const b=await createImageBitmap(blobs.get(ref));const c=document.createElement('canvas');c.width=b.width;c.height=b.height;const x=c.getContext('2d');x.drawImage(b,0,0);b.close();return x.getImageData(0,0,c.width,c.height).data};
      window.check=async(cases)=>{
        const bytes=Uint8ClampedArray.from({length:137*81*4},(_,i)=>i%4===3?255:(i*73)%256);
        const encodingPaths=[];
        const NativeWorker=window.Worker,NativeOffscreenCanvas=window.OffscreenCanvas;
        try {
          for(const mode of ['worker','offscreen-fallback','dom-fallback']) {
            window.Worker=mode==='worker'?NativeWorker:undefined;
            window.OffscreenCanvas=mode==='dom-fallback'?undefined:NativeOffscreenCanvas;
            const encoded=await encodeCapturePixels(bytes,137,81);blobs.set('encoder',encoded);
            const decoded=await pixels('encoder');if(decoded.length!==bytes.length||decoded.some((v,i)=>v!==bytes[i]))throw Error('Capture encoder changed pixels: '+mode);
            encodingPaths.push(mode);
          }
        }finally{window.Worker=NativeWorker;window.OffscreenCanvas=NativeOffscreenCanvas}
        const results=[];
        for(const c of cases){
          const cache={captures:new Map()};
          try {
            const first=await reviewAuthoringPacket(c.baseline,c.baseline,c.constraints,c.settings,save,cache);
            const warm=await reviewAuthoringPacket(c.baseline,c.candidate,c.constraints,c.settings,save,cache);
            const repeated=await reviewAuthoringPacket(c.baseline,c.candidate,c.constraints,c.settings,save,cache);
            if(warm.timings.cacheHits!==2||repeated.timings.cacheHits!==4||repeated.timings.renderedImages!==0)throw Error('Capture cache misses');
            const cold=await reviewAuthoringPacket(c.baseline,c.candidate,c.constraints,c.settings,save);
            let error=0,count=0,max=0;
            for(let i=0;i<warm.captures.length;i++){
              const a=await pixels(warm.captures[i].candidate),b=await pixels(cold.captures[i].candidate);
              for(let j=0;j<a.length;j++){if(j%4===3)continue;const d=Math.abs(a[j]-b[j]);error+=d;count++;max=Math.max(max,d)}
            }
            const mae=error/count;
            if(mae>1)throw Error('Warm renderer changed image; MAE '+mae);
            results.push({target:c.settings.target,warm:warm.timings,cold:cold.timings,repeated:repeated.timings,pixelMeanAbsoluteError:mae,maxChannelError:max});
          } finally {cache.render?.dispose();cache.captures.clear()}
        }
        const shared={captures:new Map(),maxEntries:3},cross=[];
        try{
          for(const [index,c] of [...cases,{...cases[0],settings:{...cases[0].settings,width:320,height:240}},{...cases[1],settings:{...cases[1].settings,views:cases[1].settings.views.map(v=>({...v,mode:'clay'}))}}].entries()){
            const warm=await reviewAuthoringPacket(c.baseline,c.candidate,c.constraints,c.settings,save,shared),cold=await reviewAuthoringPacket(c.baseline,c.candidate,c.constraints,c.settings,save);
            let error=0,count=0;
            for(let i=0;i<warm.captures.length;i++){const a=await pixels(warm.captures[i].candidate),b=await pixels(cold.captures[i].candidate);if(a.length!==b.length)throw Error('Resolution leaked');for(let j=0;j<a.length;j++){if(j%4!==3){error+=Math.abs(a[j]-b[j]);count++}}}
            if(error/count>1||shared.captures.size>3)throw Error('Cross-job state or cache bound failed');
            cross.push({index,pixelMeanAbsoluteError:error/count,retained:shared.captures.size});
          }
        }finally{shared.render?.dispose();shared.captures.clear()}
        const c=cases[0];
        const budget=await reviewAuthoringStudy(c.baseline,[0,1].map(i=>({proposal:'p'+i,project:c.candidate,conditions:{exposure:.8,moisture:.5,maturity:.9,variation:.7}})),c.constraints,{...c.settings,budgetMs:0},save);
        if(!budget.results[0].packet||!budget.results[1].error?.includes('budget'))throw Error('Bounded search failed to retain first candidate and stop');
        return {results,captureEncodingPixelsPreserved:true,encodingPaths,crossJobSourceResolutionModeChecks:cross,budgetStopVerified:true};
      };window.ready=true;
    `,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready");
      const report = await view.evaluate(`check(${JSON.stringify(cases)})`);
      if (errors.length) throw Error(errors.join("\n"));
      await Bun.write(join(output, "cache-check.json"), JSON.stringify(report, null, 2));
      return { output, report };
    } finally {
      server.stop(true);
    }
  });
}
if (import.meta.main) console.log(JSON.stringify(await checkReviewCache(), null, 2));
