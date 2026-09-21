import { join } from "node:path";
import type { BrowserView } from "./browser";
import { setViewport, waitFor } from "./browser";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}
/** Runs against the exported local-file player already loaded by the production UI suite. */
export async function verifyWinterGame(view: BrowserView, output: string) {
  const checks: string[] = [];
  const capture = async (name: string) => {
    await view.evaluate("new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))");
    await Bun.write(join(output, name), await view.screenshot());
  };
  const delivery = await view.evaluate<{
    launchMs: number;
    mode: string;
    cookedDocuments: number;
    sourceCompiledDocuments: number;
    compilerSource?: string;
  }>("wrelaPlayer.measure().delivery");
  assert(
    delivery.mode === "cooked" &&
      delivery.cookedDocuments > 0 &&
      delivery.sourceCompiledDocuments === 0 &&
      !!delivery.compilerSource,
    `Exported player did not consume its matching cooked products: ${JSON.stringify(delivery)}`,
  );
  assert(
    delivery.launchMs > 0 && delivery.launchMs <= 5000,
    `Reference cold launch exceeded the 5000ms local budget: ${delivery.launchMs}ms`,
  );
  checks.push(
    "Matching cooked export opens a complete first frame within the 5000ms local reference budget without finite source compilation",
  );
  await view.evaluate(
    '(()=>{if(wrelaPlayer.inspect().playing)[...document.querySelectorAll(".controls button")].find(button=>button.textContent==="Pause").click()})()',
  );
  const viewerGraphics = await view.evaluate<
    { profile: string; time: number; complete: boolean }[]
  >(`(async()=>{
    const before=wrelaPlayer.inspect().time,results=[];
    for(const profile of ['low','high','balanced']){
      await wrelaPlayer.graphics.set(profile);
      const state=wrelaPlayer.inspect();
      if(state.time!==before)throw Error('Graphics recreation advanced paused viewer time');
      results.push({profile:state.graphics.profile,time:state.time,complete:wrelaPlayer.measure().completeness.complete});
    }return results;
  })()`);
  assert(
    viewerGraphics.every((result) => result.complete),
    "A viewer graphics profile returned an incomplete frame",
  );
  await view.evaluate("wrelaPlayer.game.start()");
  await waitFor(view, "wrelaPlayer.game.inspect().active && !wrelaPlayer.game.inspect().preparing");
  await view.cdp("Input.dispatchKeyEvent", {
    type: "keyDown",
    code: "KeyW",
    key: "w",
    windowsVirtualKeyCode: 87,
  });
  try {
    await waitFor(view, "wrelaPlayer.game.inspect().position[2] < -0.6", 10000);
  } finally {
    await view.cdp("Input.dispatchKeyEvent", {
      type: "keyUp",
      code: "KeyW",
      key: "w",
      windowsVirtualKeyCode: 87,
    });
  }
  checks.push("Native keyboard input moves the physical bunny during live animation frames");
  await view.evaluate("wrelaPlayer.game.pause()");
  const paused = await view.evaluate<{ before: number; after: number }>(
    `(async()=>{const before=wrelaPlayer.game.inspect().tick;await wrelaPlayer.game.simulate({ticks:60});return{before,after:wrelaPlayer.game.inspect().tick}})()`,
  );
  assert(paused.before === paused.after, "Paused trail advanced game time");
  await view.evaluate(
    "(()=>{window.beforeGraphicsTrail=JSON.stringify(wrelaPlayer.game.save());const select=document.querySelector('.winter-panel select[aria-label=\"Graphics quality\"]');select.value='low';select.dispatchEvent(new Event('change',{bubbles:true}));})()",
  );
  await waitFor(view, "!wrelaPlayer.inspect().preparing && wrelaPlayer.inspect().graphics.profile==='low'");
  assert(
    await view.evaluate(
      "JSON.stringify(wrelaPlayer.game.save())===window.beforeGraphicsTrail && wrelaPlayer.measure().completeness.complete",
    ),
    "Paused game graphics control changed saved progress or omitted rendered geometry",
  );
  await view.evaluate("wrelaPlayer.graphics.set('balanced')");
  checks.push(
    "Performance, Balanced and High quality preserve paused viewer time; the paused game control preserves its complete save and renders fully",
  );
  await view.click('.winter-panel button[data-action="save"]');
  assert(
    await view.evaluate("!!localStorage.getItem('wrela-winter-trail:'+wrelaPlayer.inspect().project)"),
    "Native save control did not persist a trail save",
  );
  await view.evaluate("window.trailPortableSave=wrelaPlayer.game.save()");
  await view.evaluate("wrelaPlayer.game.restart()");
  await view.evaluate("wrelaPlayer.game.pause()");
  await view.click('.winter-panel button[data-action="restore"]');
  await waitFor(
    view,
    "!wrelaPlayer.game.inspect().preparing && wrelaPlayer.game.inspect().tick===window.trailPortableSave.game.tick",
  );
  const localRestore = await view.evaluate<boolean>(
    "JSON.stringify(wrelaPlayer.game.save().game)===JSON.stringify(window.trailPortableSave.game)",
  );
  assert(localRestore, "Local trail save changed marker progress, warmth or actor position");
  await view.evaluate("wrelaPlayer.game.restore(JSON.parse(JSON.stringify(window.trailPortableSave)))");
  assert(
    await view.evaluate("wrelaPlayer.game.inspect().paused"),
    "Portable restore should resume safely paused",
  );
  checks.push(
    "Pause stops authoritative time; native local save and portable JSON restore preserve the trail",
  );
  await capture("winter-trail-paused.png");
  await view.evaluate("wrelaPlayer.game.resume()");
  const route = await view.evaluate<{
    state: { phase: string; restored: string[]; seconds: number };
    events: { kind: string }[];
    batches: number;
  }>(`(async()=>{
    const destinations=[[-16,-16],[-34,12],[-10,34],[0,0]];let batches=0;
    for(let index=0;index<destinations.length;index++){
      const [x,z]=destinations[index];
      for(let attempt=0;attempt<100;attempt++){
        const state=wrelaPlayer.game.inspect(),dx=x-state.position[0],dz=z-state.position[2],distance=Math.hypot(dx,dz);
        const near=distance<1.8;
        await wrelaPlayer.game.simulate({ticks:near?55:Math.max(1,Math.min(60,Math.floor((distance-1.5)/4.3*60))),input:{move:near?[0,0]:[dx/distance,dz/distance],interact:near,hurry:false}});batches++;
        const next=wrelaPlayer.game.inspect();
        if(index<3?next.restored.length>=index+1:next.phase==='won')break;
        if(attempt===99)throw Error('Trail route failed near '+x+','+z+': '+JSON.stringify(next));
      }
      if(index===1)wrelaPlayer.game.pause();
      if(index===1){window.trailMidpoint=wrelaPlayer.game.save();wrelaPlayer.game.resume();}
    }
    return{state:wrelaPlayer.game.inspect(),events:wrelaPlayer.game.events(),batches};
  })()`);
  assert(
    route.state.phase === "won" && route.state.restored.length === 3,
    "The physical trail route did not restore all markers and return home",
  );
  assert(
    route.events.filter((event) => event.kind === "restored").length === 3 &&
      route.events.some((event) => event.kind === "home"),
    "Game events did not record all interactions and the home outcome",
  );
  await capture("winter-trail-complete.png");
  checks.push(
    "Bounded deterministic scenarios use the live input/physics path to restore every lantern and return home",
  );
  await view.evaluate("wrelaPlayer.game.restore(window.trailMidpoint)");
  await view.evaluate("wrelaPlayer.game.resume()");
  await capture("winter-trail-playing.png");
  await view.evaluate("wrelaPlayer.game.pause()");
  await setViewport(view, 390, 844);
  const mobile = await view.evaluate<boolean>("document.documentElement.scrollWidth<=innerWidth+1");
  assert(mobile, "Winter trail overflows the 390px viewport");
  await capture("winter-trail-mobile.png");
  await setViewport(view, 1440, 960);
  const failure = await view.evaluate<{ phase: string; event: boolean; restarted: string }>(
    `(async()=>{const fixture=wrelaPlayer.game.save();fixture.game.warmth=0.02;await wrelaPlayer.game.restore(fixture);wrelaPlayer.game.resume();await wrelaPlayer.game.simulate({ticks:5,input:{move:[0,0],interact:false,hurry:false}});const phase=wrelaPlayer.game.inspect().phase,event=wrelaPlayer.game.events().some(event=>event.kind==='lost');await wrelaPlayer.game.restart();wrelaPlayer.game.pause();return{phase,event,restarted:wrelaPlayer.game.inspect().phase}})()`,
  );
  assert(
    failure.phase === "lost" && failure.event && failure.restarted === "exploring",
    "Cold failure or restart did not complete correctly",
  );
  checks.push("A low-warmth portable fixture reaches the failure state and restart creates a fresh trail");
  return {
    checks,
    delivery,
    viewerGraphics,
    route,
    paused,
    mobile,
    failure,
    simulation:
      "One native keyboard segment uses real RAF. Bounded scenario batches then run the same semantic input and RuntimeSession collision/fixed-update path; they do not measure real-time performance.",
  };
}
