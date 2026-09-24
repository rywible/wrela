import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";

/** Real IndexedDB reload/CAS/portable-evidence checks and the human session selector. */
export async function checkAuthoringWorkBrowser() {
  return withBrowser(async (view, output, errors) => {
    const stylesheet = await Bun.file("apps/studio/src/style.css").text();
    const server = await fixtureServer(
      `
      import React from 'react'; import {createRoot} from 'react-dom/client';
      import {studio} from ${JSON.stringify(resolve("apps/studio/src/controller.ts"))};
      import {AuthoringWorkPanel} from ${JSON.stringify(resolve("apps/studio/src/authoring-work-panel.tsx"))};
      import { contentKey } from "@wrela/model";

      await studio.store.open();
      const style=document.createElement('style');style.textContent=${JSON.stringify(stylesheet)};document.head.append(style);
      document.querySelector('canvas').remove();const root=document.createElement('main');root.className='agent-dialog';document.body.append(root);
      createRoot(root).render(React.createElement(AuthoringWorkPanel));
      window.work=studio.work;window.key=contentKey;window.readSaved=()=>studio.store.load(studio.authoring.getSnapshot().project.id);window.ready=true;
    `,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready");
      const result = await view.evaluate(`(async()=>{
        await work.create({id:'first',brief:'Retain coat evidence',constraints:[{id:'shape',kind:'source',target:'polar-bunny',path:['field']}]});
        const first=await work.open('first');
        await work.propose('first',first.key,'matte',[{kind:'document.set',target:'snow-fur',path:['roughness'],value:0.71}]);
        let rejected=false;try{await work.propose('first',first.key,'stale',[{kind:'document.set',target:'snow-fur',path:['roughness'],value:0.72}])}catch{rejected=true}
        if(!rejected)throw Error('Stale work write was accepted');
        await work.review('first',(await work.open('first')).key,'matte');
        const canvas=document.createElement('canvas');canvas.width=2;canvas.height=2;
        canvas.getContext('2d').fillRect(0,0,2,2);
        const image=await work.store.saveArtifact('capture.png',await new Promise(r=>canvas.toBlob(r)));
        await work.feedback('first',(await work.open('first')).key,{id:'evidence',at:new Date().toISOString(),kind:'review',actor:'test',summary:'Storage fixture image',evidence:[image]});
        await work.decide('first',(await work.open('first')).key,'matte',{reviewer:'integration-test',verdict:'accepted',reason:'Recipe persistence fixture only; no artistic judgement'});
        await work.recipe('first','matte',{id:'matte-fixture',description:'Recipe persistence fixture',parameters:[]});
        const bundle=structuredClone(await work.backup('first'));bundle.work.id='second';bundle.work.brief='Second retained session';
        await work.restore(bundle);
        const restored=await work.export('second');const reference=restored.events.find(e=>e.id==='evidence').evidence[0];
        const blob=await work.store.artifact(reference);
        if(!(blob instanceof Blob)||blob.size===0||reference===image)throw Error('Portable evidence did not relocate');
        return {staleRejected:rejected,evidenceBytes:blob.size,portableArtifacts:bundle.artifacts.length};
      })()`);
      await view.reload();
      await waitFor(view, "window.ready && document.querySelectorAll('select option').length===3");
      const reloaded = await view.evaluate(`(async()=>{
        const sessions=await work.list(),recipes=await work.recipes();
        if(sessions.length!==2||recipes.length!==1)throw Error('IndexedDB state did not survive reload');
        const select=document.querySelector('select');select.value='first';select.dispatchEvent(new Event('change',{bubbles:true}));
        return {sessions:sessions.length,recipes:recipes.length};
      })()`);
      await waitFor(view, "document.querySelector('section').textContent.includes('matte')");
      await view.evaluate(
        "(()=>{document.querySelector('select').value='second';document.querySelector('select').dispatchEvent(new Event('change',{bubbles:true}))})()",
      );
      await waitFor(
        view,
        "document.querySelector('select').value==='second' && document.querySelector('section').textContent.includes('Second retained session')",
      );
      const fast = await view.evaluate(`(async()=>{
        const context=await work.fast.context('first','polar-bunny');
        const evaluated=await work.fast.evaluate('first',{expectedKey:context.key,proposal:'matte',target:'polar-bunny',views:context.views});
        const result=await work.fast.finish('first',{expectedKey:evaluated.key,proposal:'matte',requireArtReview:true});
        if(!result.published||result.packagingPending||!result.bundle.artifacts.length)throw Error('Fast browser handoff failed: '+JSON.stringify(result));
        const sheet=await work.store.artifact(evaluated.packet.contactSheet);
        if(!(sheet instanceof Blob)||!sheet.size)throw Error('Matched review images were not retained');
        return {fastPublished:result.published,fastArtifacts:result.bundle.artifacts.length,contactSheetBytes:sheet.size};
      })()`);
      await view.reload();
      await waitFor(view, "window.ready && document.querySelectorAll('select option').length===3");
      await view.evaluate(`(async()=>{
        const retained=await work.export('first');
        if(retained.proposals.find(p=>p.id==='matte')?.status!=='adopted')throw Error('Fast publication did not survive reload');
        const saved=await readSaved();if(saved.project.documents.find(d=>d.id==='snow-fur').roughness!==0.71)throw Error('Published source did not survive reload');
        const select=document.querySelector('select');select.value='first';select.dispatchEvent(new Event('change',{bubbles:true}));
      })()`);
      await waitFor(view, "document.querySelector('section').textContent.includes('adopted')");
      await Bun.write(join(output, "work-panel.png"), await view.screenshot());
      if (errors.length) throw Error(errors.join("\n"));
      const report = {
        ...(result as object),
        ...(reloaded as object),
        ...(fast as object),
        selectionChanged: true,
        output,
      };
      await Bun.write(join(output, "work-browser-review.json"), JSON.stringify(report, null, 2));
      return report;
    } finally {
      server.stop(true);
    }
  });
}
if (import.meta.main) console.log(JSON.stringify(await checkAuthoringWorkBrowser(), null, 2));
