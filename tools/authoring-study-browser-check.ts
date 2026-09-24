import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { transferTimber, transferTree } from "./fixtures/authoring-transfer";

export async function checkAuthoringStudyBrowser() {
  const project = transferTimber(),
    tree = transferTree();
  for (const d of tree.documents)
    if (!project.documents.some((p) => p.id === d.id)) project.documents.push(d);
  return withBrowser(async (view, output, errors) => {
    const stylesheet = await Bun.file("apps/studio/src/style.css").text();
    const server = await fixtureServer(
      `
      import React from 'react'; import {createRoot} from 'react-dom/client';
      import {studio} from ${JSON.stringify(resolve("apps/studio/src/controller.ts"))};
      import {AuthoringWorkPanel} from ${JSON.stringify(resolve("apps/studio/src/authoring-work-panel.tsx"))};
      import {domainQuality, instantiateEditRecipe, AuthoringSession} from '@wrela/authoring';
      import {reviewAuthoring} from '@wrela/review';
      import {verifyReviewImage} from ${JSON.stringify(resolve("packages/review/src/packet.ts"))};
      await studio.store.open(); studio.authoring.replace(${JSON.stringify(project)});
      const style=document.createElement('style');style.textContent=${JSON.stringify(stylesheet)};document.head.append(style);
      document.querySelector('canvas').remove();const root=document.createElement('main');root.className='agent-dialog';document.body.append(root);
      createRoot(root).render(React.createElement(AuthoringWorkPanel));
      window.work=studio.work;window.quality=domainQuality;window.ready=true;
      window.checkRecipe=async(recipe,project,target)=>{
        const plan=instantiateEditRecipe(project,recipe,{}, {[recipe.semantic.target]:target});
        if(plan.acceptance!=='unreviewed')throw Error('Transferred recipe inherited art acceptance');
        const s=new AuthoringSession(project);s.apply({expectedRevision:0,operations:plan.operations});
        const r=await reviewAuthoring(project,s.getSnapshot().project,plan.constraints);
        if(r.results.some(r=>r.status!=='passed'))throw Error('Transferred recipe violated source protection');
      };
      window.checkBlank=async()=>{const c=document.createElement('canvas');c.width=64;c.height=48;c.getContext('2d').fillRect(0,0,64,48);const b=await new Promise(r=>c.toBlob(r));try{await verifyReviewImage(b)}catch{return true}return false};
    `,
      output,
    );
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready");
      if (!(await view.evaluate("checkBlank()"))) throw Error("Blank review was accepted");
      const results = [];
      for (const [domain, target, heldout] of [
        ["timber", project.entry, transferTimber(true)],
        ["vegetation", tree.entry, transferTree(true)],
      ] as const) {
        const result = await view.evaluate(`(async()=>{
          const id=${JSON.stringify(domain)}, target=${JSON.stringify(target)};
          let stage='create';try {
          await work.create({id,brief:'Browser transfer fixture: '+id});
          stage='context';const c=await work.fast.context(id,target);
          stage='study';const result=await work.fast.study(id,{id:'browser-study',expectedKey:c.key,brief:{domain:id,target,conditions:{exposure:.75,moisture:.4,maturity:.85,variation:.55},quality:quality(id)},candidates:2});
          if(!result.candidates.every(c=>c.passed))throw Error('Study constraints failed');
          const p=result.candidates[0].proposal;
          stage='decide';await work.decide(id,result.key,p,{reviewer:'integration-test',verdict:'accepted',reason:'Storage and recipe transfer fixture, not an artistic judgement'});
          stage='remember';const recipe=await work.fast.remember(id,p,id+'-recipe');
          await checkRecipe(recipe,${JSON.stringify(heldout)},${JSON.stringify(heldout.entry)});
          stage='backup';const bundle=structuredClone(await work.backup(id));bundle.work.id=id+'-restored';stage='restore';await work.restore(bundle);
          const restored=await work.export(id+'-restored');
          const gallery=restored.events.find(e=>e.actor==='study-workflow').evidence[1];
          const blob=await work.store.artifact(gallery);
          if(!(blob instanceof Blob)||blob.size<1000)throw Error('Restored gallery missing');
          return {domain:id,candidates:result.candidates.length,bytes:blob.size,portableArtifacts:bundle.artifacts.length,recipe:recipe.id};
          }catch(error){throw Error(id+" / "+stage+": "+error)}
        })()`);
        results.push(result);
      }
      await view.reload();
      await waitFor(view, "window.ready");
      const retained = await view.evaluate(
        `(async()=>{const ws=await work.list(),recipes=await work.recipes();if(ws.length!==4||recipes.length!==2)throw Error('Study state failed reload');return {sessions:ws.length,recipes:recipes.length}})()`,
      );
      await waitFor(view, "!!document.querySelector('select option[value=vegetation]')");
      await view.evaluate(
        "(()=>{const s=document.querySelector('select');s.value='vegetation';s.dispatchEvent(new Event('change',{bubbles:true}))})()",
      );
      await waitFor(
        view,
        "document.body.textContent.includes('Explore reviewed alternatives') && document.body.textContent.includes('Create from my brief')",
      );
      await view.evaluate(
        "(()=>{const d=[...document.querySelectorAll('details')].find(d=>d.querySelector('summary')?.textContent==='Explore reviewed alternatives');d.open=true;const subject=d.querySelector('option[value=study-tree]')?.closest('select');if(subject?.value!=='study-tree')throw Error('Study selected an unrelated subject');[...d.querySelectorAll('button')].find(b=>b.textContent==='Create from my brief').click()})()",
      );
      await waitFor(view, "document.body.textContent.includes('5 alternatives')", 60000);
      await Bun.write(join(output, "study-panel.png"), await view.screenshot());
      if (errors.length) throw Error(errors.join("\n"));
      const report = {
        output,
        results,
        retained,
        blankReviewRejected: true,
        uiStudyGenerated: true,
        uiJobGenerated: true,
        scope: "Scripted real browser storage, review, UI and recipe-transfer checks; no art acceptance",
      };
      await Bun.write(join(output, "study-browser-report.json"), JSON.stringify(report, null, 2));
      return report;
    } finally {
      server.stop(true);
    }
  });
}
if (import.meta.main) console.log(JSON.stringify(await checkAuthoringStudyBrowser(), null, 2));
