import { join } from "node:path";
import { createCreatureFixture } from "@wrela/examples";
import { contentKey, parseProject } from "@wrela/model";
import { type BrowserView, setViewport, waitFor, withBrowser } from "./browser";
import { buildStudio } from "./build";

function ensure(condition: unknown, message: string): asserts condition {
  if (!condition) throw Error(message);
}

async function clickText(view: BrowserView, text: string) {
  const point = await view.evaluate<{ x: number; y: number }>(`(() => {
    const element = [...document.querySelectorAll('button')].find(button => button.textContent.trim() === ${JSON.stringify(text)});
    if (!element) throw Error('Missing control: ' + ${JSON.stringify(text)});
    element.scrollIntoView({block:'center'});
    const bounds = element.getBoundingClientRect(),x=bounds.x+bounds.width/2,y=bounds.y+bounds.height/2;
    const hit=document.elementFromPoint(x,y);
    if(!hit || !element.contains(hit))throw Error('Control is clipped or obscured: '+${JSON.stringify(text)}+' at '+JSON.stringify({x,y,hit:hit?.outerHTML.slice(0,240)}));
    return {x,y};
  })()`);
  await view.cdp("Input.dispatchMouseEvent", {
    type: "mousePressed",
    ...point,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await view.cdp("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    ...point,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}

/** Runs only when explicitly invoked. withBrowser supplies the machine GPU lease,
 * isolated Chrome process group, 30s browser-call deadlines and source manifests.
 * No world/water is installed at startup; verification canvas is capped at320x180. */
export async function verifyCreatureStudio(
  options: { biped?: boolean; frames?: 1 | 4; offline?: boolean } = {},
) {
  const fixture = createCreatureFixture(options.biped ? "reed-penitent" : "ash-warden");
  // Fail before acquiring the GPU when a work-in-progress source fixture is invalid.
  parseProject(fixture.project);
  const frames = options.frames ?? 1;
  if (frames !== 1 && frames !== 4) throw Error("Studio verification accepts one or four poses");
  return withBrowser(
    async (view, output, errors) => {
      const directory = await buildStudio(join(output, "studio-build"));
      const report: Record<string, unknown> = {
        creature: fixture.characterId,
        frames,
        checks: [],
        visualApproval: "not-reviewed",
      };
      const checks = report.checks as string[];
      const record = async (name: string, evidence?: unknown) => {
        checks.push(name);
        if (evidence !== undefined) report[name] = evidence;
        await Bun.write(join(output, "creature-studio-verification.json"), JSON.stringify(report, null, 2));
        console.log(`Creature Studio: ${name}`);
      };
      const server = Bun.serve({
        hostname: "127.0.0.1",
        port: 0,
        async fetch(request) {
          const path = new URL(request.url).pathname;
          if (path === "/favicon.ico") return new Response(null, { status: 204 });
          if (path === "/bridge/session") return Response.json({ available: true });
          if (path === "/bridge/project")
            return request.method === "GET"
              ? Response.json({ project: fixture.project, key: contentKey(fixture.project) })
              : new Response("Verification workspace is read-only", { status: 405 });
          const file = Bun.file(join(directory, path === "/" ? "index.html" : path));
          if (!(await file.exists())) return new Response("Not found", { status: 404 });
          if (path === "/" || path === "/index.html") {
            // Keep the installed UI and its handlers; constrain only framebuffer layout.
            const html = (await file.text()).replace(
              "</head>",
              `<style>.viewport{width:320px!important;height:180px!important;min-height:180px!important;max-height:180px!important;align-self:center;justify-self:center}.viewport>canvas{width:320px!important;height:180px!important}</style></head>`,
            );
            return new Response(html, { headers: { "Content-Type": "text/html" } });
          }
          return new Response(file);
        },
      });
      let offline = false;
      try {
        await view.navigate(server.url.toString());
        await waitFor(
          view,
          'window.wrela && ["current","failed"].includes(wrela.preview.inspect().status)',
          30000,
        );
        await view.evaluate(
          `(() => { const control=document.querySelector('#creature-study'); if(!control)throw Error('Creature study selector missing');control.value=${JSON.stringify(fixture.characterId)};control.dispatchEvent(new Event('change',{bubbles:true})); })()`,
        );
        await waitFor(
          view,
          `wrela.inspect().selection===${JSON.stringify(fixture.characterId)} && wrela.preview.inspect().status==='current'`,
          30000,
        );
        await view.evaluate(
          `wrela.preview.prepare({subject:${JSON.stringify(fixture.characterId)},stage:'studio',stageId:${JSON.stringify(fixture.stageId)},quality:'interactive',timeoutMs:20000})`,
        );
        await waitFor(view, "document.querySelector('.creature-panel') !== null");
        const viewport = await view.evaluate<{ width: number; height: number }>(
          "(()=>{const c=document.querySelector('#viewport');return{width:c.width,height:c.height}})()",
        );
        ensure(
          viewport.width <= 320 && viewport.height <= 180,
          "Verification exceeded its bounded framebuffer",
        );
        await record("fixture-opened-without-world", {
          viewport,
          project: await view.evaluate("JSON.parse(wrela.project()).id"),
        });

        await view.evaluate(
          "window.__creatureBefore={source:JSON.stringify(wrela.project()),revision:wrela.inspect().revision,camera:wrela.preview.inspect().camera,time:wrela.preview.inspect().time}",
        );
        await clickText(view, "Clay + skeleton");
        await view.evaluate("wrela.preview.prepare({timeoutMs:20000})");
        const clay = await view.evaluate(`(() => {const state=wrela.preview.inspect();return {
        mode:state.mode,hideGroom:state.hideGroom,overlays:state.overlays,
        sourceUnchanged:JSON.stringify(wrela.project())===__creatureBefore.source,
        cameraUnchanged:JSON.stringify(state.camera)===JSON.stringify(__creatureBefore.camera),
        timeUnchanged:state.time===__creatureBefore.time};})()`);
        ensure(
          (clay as { mode: string }).mode === "clay" &&
            (clay as { sourceUnchanged: boolean }).sourceUnchanged &&
            (clay as { cameraUnchanged: boolean }).cameraUnchanged &&
            (clay as { timeUnchanged: boolean }).timeUnchanged,
          "Clay inspection changed source, camera or pose",
        );
        await Bun.write(join(output, "studio-clay-skeleton.png"), await view.screenshot());
        await record("clay-and-skeleton-preserve-source-camera-pose", clay);

        const picked = await view.evaluate(`(() => {
        const canvas=document.querySelector('#viewport'),width=canvas.clientWidth,height=canvas.clientHeight;
        for(const y of [.25,.4,.5,.6,.7])for(const x of [.25,.4,.5,.6,.75]){
          const point={x:x*width,y:y*height,width,height},surface=wrela.creature.inspectPixel(point);
          if(surface?.documentId===${JSON.stringify(fixture.characterId)} && surface.regions.length){
            const grounding=wrela.creature.groundPixel(point); window.__creaturePoint=point; return {point,surface,grounding};
          }
        }throw Error('No rendered creature patch could be grounded');
      })()`);
        ensure(
          !!(picked as { grounding?: unknown }).grounding,
          "Current displayed creature could not be grounded",
        );
        await record("actual-rendered-pixel-and-displayed-lod-grounding", picked);

        const repair = await view.evaluate(`(() => {
          const before=JSON.stringify(wrela.project());
          const canvas=document.querySelector('#viewport'),width=canvas.clientWidth,height=canvas.clientHeight;
          for(const y of [.3,.4,.5,.6])for(const x of [.3,.4,.5,.6,.7]){
            const pixel={x:x*width,y:y*height,width,height},hit=wrela.creature.groundPixel(pixel);
            if(!hit||hit.regions.length!==1||hit.geometrySources.length!==1||hit.groomRoots.length)continue;
            const subject=wrela.inspect(${JSON.stringify(fixture.characterId)});
            if(subject.creature.attachments.some(a=>a.nodeIds.includes(hit.geometrySources[0].id)))continue;
            const result=wrela.creature.repairPixel({id:'pixel-repair-check',pixel,restDisplacement:[0,.02,0],support:{radii:[.18,.18,.18],rotation:[0,0,0]},intent:'Verify observed surface repair'});
            return {sourceUnchanged:JSON.stringify(wrela.project())===before,residuals:result.residuals,bound:result.maxDisplacementBound};
          }throw Error('No independently owned body surface for repair');
        })()`);
        ensure(
          (repair as { sourceUnchanged: boolean }).sourceUnchanged,
          "Pixel repair modified accepted source",
        );
        const repairPreview = await view.evaluate(
          `(async()=>{const result=await wrela.creature.previewCandidate('pixel-repair-check',{subject:${JSON.stringify(fixture.characterId)},stageId:${JSON.stringify(fixture.stageId)},camera:wrela.preview.inspect().camera,motion:'idle',ticks:[0],channel:'clay',hideGroom:true,width:320,height:180,timeoutMs:25000});wrela.creature.candidates.cancel('pixel-repair-check');wrela.creature.candidates.release('pixel-repair-check');return result.metadata;})()`,
        );
        await record("grounded-repair-and-compiled-preview", { repair, preview: repairPreview });

        const candidateId = "studio-verification-candidate";
        await view.evaluate(`(() => {const subject=wrela.inspect(${JSON.stringify(fixture.characterId)}),region=subject.creature.regions.find(r=>r.nodeIds.length)||subject.creature.regions[0];
        wrela.creature.candidates.propose({id:${JSON.stringify(candidateId)},batch:{expectedRevision:wrela.inspect().revision,label:'Verification shoulder proportion',operations:[{kind:'creature.proportion',target:subject.id,region:region.id,scale:[1.025,1,1],propagate:'region'}]}});
      })()`);
        const pair = await view.evaluate<{
          metadata: unknown;
          images: { tick: number; baseline: string; candidate: string }[];
          sourceUnchanged: boolean;
        }>(`(async () => {
        const before=JSON.stringify(wrela.project()),result=await wrela.creature.previewCandidate(${JSON.stringify(candidateId)},{subject:${JSON.stringify(fixture.characterId)},stageId:${JSON.stringify(fixture.stageId)},camera:wrela.preview.inspect().camera,motion:'idle',ticks:${JSON.stringify(frames === 4 ? [0, 30, 60, 90] : [0])},channel:'clay',overlays:['rig'],hideGroom:true,width:320,height:180,timeoutMs:25000});
        const url=blob=>new Promise((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result);reader.onerror=()=>reject(reader.error);reader.readAsDataURL(blob)});
        return {metadata:result.metadata,sourceUnchanged:JSON.stringify(wrela.project())===before,images:await Promise.all(result.frames.map(async frame=>({tick:frame.tick,baseline:await url(frame.baseline.blob),candidate:await url(frame.candidate.blob)})))};
      })()`);
        ensure(
          pair.sourceUnchanged && pair.images.length === frames,
          "Candidate sequence mutated accepted source or omitted poses",
        );
        for (const image of pair.images)
          for (const version of ["baseline", "candidate"] as const)
            await Bun.write(
              join(output, `candidate-${version}-${image.tick}.png`),
              Buffer.from(image[version].split(",")[1], "base64"),
            );
        await record("bounded-matched-candidate-sequence", pair.metadata);
        await view.evaluate(`wrela.creature.candidates.adopt(${JSON.stringify(candidateId)})`);
        await view.evaluate("wrela.preview.prepare({timeoutMs:20000})");
        ensure(
          await view.evaluate("JSON.stringify(wrela.project())!==__creatureBefore.source"),
          "Candidate adoption did not change source",
        );
        await view.click('button[aria-label="Undo"]');
        await view.evaluate("wrela.preview.prepare({timeoutMs:20000})");
        ensure(
          await view.evaluate("JSON.stringify(wrela.project())===__creatureBefore.source"),
          "UI Undo did not restore pre-candidate source",
        );
        await record("candidate-adoption-and-ui-undo");

        await view.evaluate(
          "window.__encounterBefore={source:JSON.stringify(wrela.project()),camera:wrela.preview.inspect().camera,time:wrela.preview.inspect().time,playing:wrela.preview.inspect().playing}",
        );
        await clickText(view, "Play encounter");
        await waitFor(view, "wrela.creature.encounter.inspect()?.tick>=2", 20000);
        const startTick = await view.evaluate<number>("wrela.creature.encounter.inspect().tick");
        await view.evaluate("wrela.creature.encounter.input({move:[-1,0],dodge:true,strike:true})");
        await waitFor(view, `wrela.creature.encounter.inspect()?.tick>=${startTick + 12}`, 10000);
        await view.evaluate("wrela.creature.encounter.input({move:[0,0]})");
        const encounter = await view.evaluate("wrela.creature.encounter.inspect()");
        await Bun.write(join(output, "studio-creature-encounter.png"), await view.screenshot());
        await clickText(view, "End encounter");
        await waitFor(view, "!wrela.creature.encounter.inspect()", 2000).catch(() => undefined);
        const restoration = await view.evaluate<{
          inactive: boolean;
          sourceUnchanged: boolean;
          cameraUnchanged: boolean;
          timeUnchanged: boolean;
          playingUnchanged: boolean;
        }>(`(()=>{const state=wrela.preview.inspect(),before=__encounterBefore,encounter=wrela.creature.encounter.inspect();return {
          inactive:!encounter,sourceUnchanged:JSON.stringify(wrela.project())===before.source,
          cameraUnchanged:JSON.stringify(state.camera)===JSON.stringify(before.camera),
          timeUnchanged:state.time===before.time,playingUnchanged:state.playing===before.playing,
          expected:{camera:before.camera,time:before.time,playing:before.playing},
          actual:{camera:state.camera,time:state.time,playing:state.playing,encounter},
          endControlVisible:!![...document.querySelectorAll('button')].find(button=>button.textContent.trim()==='End encounter')};})()`);
        report.encounterRestoration = restoration;
        await Bun.write(join(output, "creature-studio-verification.json"), JSON.stringify(report, null, 2));
        ensure(
          restoration.inactive &&
            restoration.sourceUnchanged &&
            restoration.cameraUnchanged &&
            restoration.timeUnchanged &&
            restoration.playingUnchanged,
          `Ending encounter did not restore authoring state: ${JSON.stringify(restoration)}`,
        );
        await record("playable-encounter-and-original-state-restoration", encounter);

        if (options.offline !== false) {
          await view.evaluate("localStorage.setItem('wrela-render-quality','low')");
          const blobUrl = await view.evaluate<string>(
            "wrela.export().then(blob=>{window.__exportedCreature=URL.createObjectURL(blob);return window.__exportedCreature})",
          );
          await setViewport(view, 320, 180);
          await view.cdp("Network.enable");
          await view.cdp("Network.emulateNetworkConditions", {
            offline: true,
            latency: 0,
            downloadThroughput: 0,
            uploadThroughput: 0,
          });
          offline = true;
          await view.navigate(blobUrl);
          await waitFor(view, "window.wrelaPlayer?.ready", 30000);
          await waitFor(view, "wrelaPlayer.measure().completeness?.complete", 20000);
          const delivery = await view.evaluate<{
            mode: string;
            sourceCompiledDocuments: number;
            cookedDocuments: number;
          }>("wrelaPlayer.measure().delivery");
          ensure(
            delivery.mode === "cooked" &&
              delivery.sourceCompiledDocuments === 0 &&
              delivery.cookedDocuments > 0,
            "Offline export did not use the embedded cooked creature",
          );
          await Bun.write(join(output, "offline-cooked-creature.png"), await view.screenshot());
          await record("offline-exported-cooked-player", {
            delivery,
            state: await view.evaluate("wrelaPlayer.inspect()"),
          });
          await view.evaluate(`wrelaPlayer.creature.start(${JSON.stringify(fixture.characterId)})`);
          await waitFor(view, "wrelaPlayer.creature.inspect().snapshot?.tick>=2", 20000);
          const exportedTick = await view.evaluate<number>("wrelaPlayer.creature.inspect().snapshot.tick");
          await view.evaluate("wrelaPlayer.creature.input({move:[1,0],dodge:true,strike:true})");
          await waitFor(view, `wrelaPlayer.creature.inspect().snapshot?.tick>=${exportedTick + 12}`, 10000);
          await view.evaluate(
            "(()=>{wrelaPlayer.creature.input({move:[0,0]});return wrelaPlayer.creature.pause()})()",
          );
          await Bun.write(join(output, "offline-cooked-encounter.png"), await view.screenshot());
          const exportedEncounter = await view.evaluate("wrelaPlayer.creature.inspect()");
          await view.evaluate("wrelaPlayer.creature.leave()");
          ensure(
            await view.evaluate(
              "!wrelaPlayer.creature.inspect().active && wrelaPlayer.measure().delivery.sourceCompiledDocuments===0",
            ),
            "Exported encounter did not restore player or required source recompilation",
          );
          await record("offline-cooked-playable-encounter", exportedEncounter);
        }
        ensure(errors.length === 0, `Browser console errors: ${errors.join("; ")}`);
        await record("completed", { consoleErrors: errors });
        return { output, checks };
      } finally {
        if (offline)
          await view
            .cdp("Network.emulateNetworkConditions", {
              offline: false,
              latency: 0,
              downloadThroughput: -1,
              uploadThroughput: -1,
            })
            .catch(() => {});
        await view
          .evaluate(
            "(()=>{window.wrela?.creature.encounter.stop();window.wrelaPlayer?.dispose();if(window.__exportedCreature)URL.revokeObjectURL(window.__exportedCreature);return true})()",
          )
          .catch(() => {});
        server.stop(true);
      }
    },
    960,
    640,
  );
}

if (import.meta.main) {
  const result = await verifyCreatureStudio({
    biped: process.argv.includes("--biped"),
    frames: process.argv.includes("--sequence") ? 4 : 1,
    offline: !process.argv.includes("--skip-offline"),
  });
  console.log(JSON.stringify(result));
}
