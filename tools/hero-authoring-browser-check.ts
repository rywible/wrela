import { join, resolve } from "node:path";
import { createHeroWorkspace } from "@wrela/authoring";
import { fixtureServer, waitFor, withBrowser } from "./browser";

export async function checkHeroBrowser() {
  return withBrowser(async (view, output, errors) => {
    const server = await fixtureServer(
      `
      import React from 'react';import {createRoot} from 'react-dom/client';
      import {studio} from ${JSON.stringify(resolve("apps/studio/src/controller.ts"))};
      import {AuthoringWorkPanel} from ${JSON.stringify(resolve("apps/studio/src/authoring-work-panel.tsx"))};
      import {contentKey} from '@wrela/model';
      await studio.store.open();studio.authoring.replace(${JSON.stringify(createHeroWorkspace())});
      const style=document.createElement('style');style.textContent=${JSON.stringify(await Bun.file("apps/studio/src/style.css").text())};document.head.append(style);
      document.querySelector('canvas').remove();const root=document.createElement('main');root.className='agent-dialog';document.body.append(root);createRoot(root).render(React.createElement(AuthoringWorkPanel));
      window.work=studio.work;window.key=contentKey;window.ready=true;
    `,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready");
      await view.evaluate(
        "work.create({id:'hero-browser',brief:'Create and refine a portable hero construction'})",
      );
      await view.reload();
      await waitFor(view, "window.ready && !!document.querySelector('option[value=hero-browser]')");
      await view.evaluate(
        "(()=>{const s=document.querySelector('option[value=hero-browser]').closest('select');s.value='hero-browser';s.dispatchEvent(new Event('change',{bubbles:true}))})()",
      );
      await waitFor(view, "document.body.textContent.includes('Create and refine a hero asset')");
      await view.evaluate(
        "(()=>{const d=[...document.querySelectorAll('details')].find(d=>d.querySelector('summary')?.textContent==='Create and refine a hero asset');d.open=true;[...d.querySelectorAll('button')].find(b=>b.textContent==='Construct and review').click()})()",
      );
      await waitFor(view, "document.body.textContent.includes('1 alternatives')", 60000);
      const report = await view.evaluate(`(async()=>{
        let w=await work.export('hero-browser');const base=w.proposals[0].id;
        const production=await work.fast.advanceHero(w.id,base,'production',.65);
        if(!production.passed||production.packet.captures.length!==6)throw Error('Production stage views failed');
        w=await work.export(w.id);const root=w.proposals[0].batch.operations.find(o=>o.kind==='document.create'&&o.document.kind==='object').document;
        const repair=await work.fast.iterate(w.id,{expectedKey:key(w),proposal:'glass-repair',target:root.id,baseProposal:production.proposal,repair:{kind:'material.response',target:root.id+'-glass',transmission:.97},pins:[{target:root.id,path:['assembly'],reason:'Keep accepted construction'}],critique:'Make the flame enclosure clearer'});
        if(!repair.passed)throw Error('Repair failed');
        const combined=await work.fast.iterate(w.id,{expectedKey:repair.key,proposal:'combined',target:root.id,baseProposal:production.proposal,combine:{proposal:'glass-repair',materials:[root.id+'-glass']},pins:[{target:root.id,path:['assembly'],reason:'Keep construction'}]});
        if(!combined.passed)throw Error('Combine failed');
        await work.decide(w.id,combined.key,'combined',{reviewer:'integration-test',verdict:'accepted',reason:'Persistence fixture; not artistic approval'});
        const recipe=await work.fast.rememberCreation(w.id,'combined',root.id);
        const bundle=structuredClone(await work.backup(w.id));bundle.work.id='restored';await work.restore(bundle);
        const restored=await work.export('restored');
        if(!restored.events.some(e=>e.summary.includes('Retained construction')))throw Error('Recipe evidence lost');
        await work.create({id:'transfer',brief:'Reuse portable construction'});
        const reuse=await work.fast.reuseCreation('transfer',recipe.id,recipe.revision);
        if(!reuse.passed)throw Error('Reuse failed');
        return {uiConstructed:true,productionViews:production.packet.captures.length,pinnedRepair:true,combined:true,recipe:recipe.id,portableArtifacts:bundle.artifacts.length,reused:true};
      })()`);
      await view.reload();
      await waitFor(view, "window.ready");
      const saved = await view.evaluate(
        "(async()=>({work:(await work.list()).length,toolkit:(await work.store.toolkit('creation')).length}))()",
      );
      await Bun.write(join(output, "hero-panel.png"), await view.screenshot());
      if (errors.length) throw Error(errors.join("\n"));
      await Bun.write(
        join(output, "hero-browser-report.json"),
        JSON.stringify(
          { report, saved, scope: "Scripted real-browser integration, no art acceptance" },
          null,
          2,
        ),
      );
      return { output, report, saved };
    } finally {
      server.stop(true);
    }
  });
}
if (import.meta.main) console.log(JSON.stringify(await checkHeroBrowser(), null, 2));
