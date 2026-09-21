import { join } from "node:path";
import { pathToFileURL } from "node:url";
import type { BrowserView } from "./browser";
import { setViewport, waitFor } from "./browser";
import { buildStudio } from "./build";
import { verifyWinterGame } from "./game-verification";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}
async function input(view: BrowserView, selector: string, text: string) {
  await view.scrollTo(selector);
  await view.click(selector);
  await view.evaluate(`document.querySelector(${JSON.stringify(selector)}).select()`);
  await view.type(text);
  await view.press("Tab");
}
async function clickText(view: BrowserView, text: string) {
  const point = await view.evaluate<{ x: number; y: number }>(
    `(()=>{const element=[...document.querySelectorAll('button')].find(button=>button.textContent.trim()===${JSON.stringify(text)});if(!element)throw Error('Missing button');element.scrollIntoView({block:'center'});const r=element.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2}})()`,
  );
  await view.cdp("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await view.cdp("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}
async function ready(view: BrowserView) {
  await waitFor(view, 'window.wrela && ["current","failed"].includes(wrela.preview.inspect().status)', 60000);
  const status = await view.evaluate<{ status: string; message: string }>("wrela.preview.inspect()");
  assert(status.status === "current", `Studio preview failed: ${status.message}`);
  await view.evaluate("wrela.preview.prepare({timeoutMs:30000})");
}
async function startStudio(output: string) {
  const directory = join(output, "studio-build");
  await buildStudio(directory);
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === "/bridge/session") return Response.json({ available: false });
      const file = Bun.file(join(directory, path === "/" ? "index.html" : path));
      return (await file.exists()) ? new Response(file) : new Response("Not found", { status: 404 });
    },
  });
  return {
    url: server.url.toString(),
    stop: async () => {
      server.stop(true);
    },
  };
}

export async function verifyStudio(view: BrowserView, output: string) {
  const server = await startStudio(output);
  const checks: string[] = [];
  const checkpoint = (message: string) => {
    checks.push(message);
    console.log(`UI verified: ${message}`);
  };
  try {
    await setViewport(view, 1440, 960);
    await view.navigate(server.url);
    await ready(view);
    assert(
      await view.evaluate('document.querySelector("#viewport").width > 0'),
      "Viewport has no drawable width",
    );
    await Bun.write(join(output, "studio-world.png"), await view.screenshot());
    checkpoint("Fresh isolated browser opens the reference world with real WebGPU");

    await view.evaluate("document.querySelector('.search-button').focus()");
    await view.click(".search-button");
    await waitFor(view, '!!document.querySelector("[role=dialog]")');
    await view.evaluate("[...document.querySelectorAll('[role=dialog] button')].at(-1).focus()");
    await view.press("Tab");
    assert(
      await view.evaluate("document.activeElement===document.querySelector('[role=dialog] button')"),
      "Command palette did not wrap keyboard focus forward",
    );
    await view.press("Tab", { modifiers: ["Shift"] });
    assert(
      await view.evaluate(
        "document.activeElement===[...document.querySelectorAll('[role=dialog] button')].at(-1)",
      ),
      "Command palette did not wrap keyboard focus backward",
    );
    await view.press("Escape");
    assert(
      await view.evaluate("document.activeElement===document.querySelector('.search-button')"),
      "Closing command palette did not restore focus",
    );
    checkpoint("Command palette confines keyboard focus and restores the opening control");

    await view.click('[data-document="polar-bunny"]');
    await ready(view);
    await view.evaluate(
      `(()=>{const select=document.querySelector('select[aria-label="Preview stage"]');select.value='studio';select.dispatchEvent(new Event('change',{bubbles:true}));})()`,
    );
    await ready(view);
    assert(
      await view.evaluate('wrela.preview.inspect().stage === "studio"'),
      "Stage selector did not select neutral studio",
    );
    const beforeSize = await view.evaluate<number>(
      'wrela.inspect("polar-bunny").field.nodes.find(n=>n.id==="body").size[0]',
    );
    const beforeRevision = await view.evaluate<number>("wrela.inspect().revision");
    await input(view, 'input[aria-label="Size X"]', "0.64");
    await ready(view);
    assert(
      await view.evaluate('wrela.inspect("polar-bunny").field.nodes.find(n=>n.id==="body").size[0] === 0.64'),
      `Shape editing UI did not update authored source: ${JSON.stringify(await view.evaluate('wrela.inspect("polar-bunny")'))}`,
    );
    assert(
      (await view.evaluate<number>("wrela.inspect().revision")) > beforeRevision,
      "Shape edit did not commit a source revision",
    );
    await view.click('button[aria-label="Undo"]');
    await ready(view);
    assert(
      (await view.evaluate<number>(
        'wrela.inspect("polar-bunny").field.nodes.find(n=>n.id==="body").size[0]',
      )) === beforeSize,
      "Undo did not restore the source shape",
    );
    await view.click('button[aria-label="Redo"]');
    await ready(view);
    assert(
      await view.evaluate('wrela.inspect("polar-bunny").field.nodes.find(n=>n.id==="body").size[0] === 0.64'),
      "Redo did not restore the edit",
    );
    checkpoint("Native shape edit, undo and redo update compiled preview and authored source");

    const previewRevision = await view.evaluate<number>("wrela.inspect().revision");
    await input(view, 'input[aria-label="Exposure"]', "1.25");
    assert(
      (await view.evaluate<number>("wrela.inspect().revision")) === previewRevision,
      "Preview exposure incorrectly changed authored source",
    );
    await view.click('button[aria-label="Play preview"]');
    await waitFor(view, "wrela.preview.inspect().time > 0.1");
    await view.click('button[aria-label="Pause playback"]');
    await view.click('button[aria-label="Rewind"]');
    await ready(view);
    const capture = await view.evaluate<{
      bytes: number;
      metadata: { revision: number; tick: number; missing: string[] };
    }>("wrela.preview.capture({tick:30}).then(result=>({bytes:result.blob.size,metadata:result.metadata}))");
    assert(
      capture.bytes > 1000 && capture.metadata.tick === 30 && capture.metadata.missing.length === 0,
      "Deterministic capture metadata is incomplete",
    );
    await Bun.write(join(output, "studio-bunny.png"), await view.screenshot());
    checkpoint("Preview-only exposure, playback, rewind and tick-exact PNG capture");

    await view.click('button[aria-label="Toggle authoring handles"]');
    const positionBefore = await view.evaluate<number[]>(
      'wrela.inspect("polar-bunny").field.nodes.find(n=>n.id==="body").position',
    );
    const handle = await view.evaluate<{ x: number; y: number }>(
      `(()=>{const r=document.querySelector('button[aria-label="Move Body"]').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()`,
    );
    await view.cdp("Input.dispatchMouseEvent", {
      type: "mousePressed",
      x: handle.x,
      y: handle.y,
      button: "left",
      buttons: 1,
      clickCount: 1,
    });
    for (const dx of [8, 16, 24])
      await view.cdp("Input.dispatchMouseEvent", {
        type: "mouseMoved",
        x: handle.x + dx,
        y: handle.y - 8,
        button: "left",
        buttons: 1,
      });
    await view.cdp("Input.dispatchMouseEvent", {
      type: "mouseReleased",
      x: handle.x + 24,
      y: handle.y - 8,
      button: "left",
      buttons: 0,
      clickCount: 1,
    });
    await ready(view);
    assert(
      JSON.stringify(
        await view.evaluate('wrela.inspect("polar-bunny").field.nodes.find(n=>n.id==="body").position'),
      ) !== JSON.stringify(positionBefore),
      "Dragging authoring handle did not move the field node",
    );
    await view.click('button[aria-label="Undo"]');
    await ready(view);
    assert(
      JSON.stringify(
        await view.evaluate('wrela.inspect("polar-bunny").field.nodes.find(n=>n.id==="body").position'),
      ) === JSON.stringify(positionBefore),
      "Handle gesture was not coalesced into one undo step",
    );
    await view.click('button[aria-label="Toggle authoring handles"]');
    checkpoint("Trusted pointer handle dragging commits source edits as one undo gesture");

    await view.scrollTo(".workspace-tabs");
    await view.click(".workspace-tabs button:nth-child(3)");
    const poseSource = await view.evaluate<string>("wrela.project()");
    await input(view, 'input[aria-label="Pose rotation Y"]', "0.22");
    await input(view, 'input[aria-label="Pose offset Y"]', "0.15");
    assert(
      (await view.evaluate<string>("wrela.project()")) === poseSource,
      "Preview pose changed authored rest anatomy or motion before keying",
    );
    await view.scrollTo(".inspector .button-row button.primary");
    await view.click(".inspector .button-row button.primary");
    await ready(view);
    assert(
      await view.evaluate(
        'wrela.inspect("polar-bunny").motions[0].keys.some(k=>k.joint==="root" && k.time===0.5 && k.rotation[1]===0.22 && k.translation[1]===0.15)',
      ),
      "Pose key did not record the requested rotation and offset at the playhead",
    );
    await Bun.write(join(output, "studio-pose.png"), await view.screenshot());
    checkpoint("Pose preview preserves source until Add pose key records an explicit motion key");

    for (const domain of [
      { id: "coastal-waves", label: "Water level", key: "level", value: -0.9 },
      { id: "alpine-pine", label: "Height (m)", key: "height", value: 7.2 },
      { id: "snow-fur", label: "Roughness", key: "roughness", value: 0.61 },
      { id: "winter-sky", label: "Sun elevation", key: "sunElevation", value: 0.7 },
      { id: "daylight", label: "Ambient", key: "ambient", value: 0.72 },
    ]) {
      await view.scrollTo(`[data-document="${domain.id}"]`);
      await view.click(`[data-document="${domain.id}"]`);
      await ready(view);
      const captureDigest = () =>
        view.evaluate<string>(
          `wrela.preview.capture({tick:30}).then(async result=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',await result.blob.arrayBuffer()))).join(','))`,
        );
      const materialBefore = domain.id === "snow-fur" ? await captureDigest() : undefined;
      const previous = await view.evaluate<number>(
        `wrela.inspect(${JSON.stringify(domain.id)})[${JSON.stringify(domain.key)}]`,
      );
      await input(view, `input[aria-label="${domain.label}"]`, String(domain.value));
      await ready(view);
      assert(
        (await view.evaluate<number>(
          `wrela.inspect(${JSON.stringify(domain.id)})[${JSON.stringify(domain.key)}]`,
        )) === domain.value,
        `${domain.label} inspector did not commit the source value`,
      );
      if (materialBefore)
        assert(
          (await captureDigest()) !== materialBefore,
          "Material roughness edit did not reach the actual rendered shared subject",
        );
      await view.click('button[aria-label="Undo"]');
      await ready(view);
      assert(
        (await view.evaluate<number>(
          `wrela.inspect(${JSON.stringify(domain.id)})[${JSON.stringify(domain.key)}]`,
        )) === previous,
        `${domain.label} undo did not restore the source`,
      );
    }
    checkpoint(
      "Water, vegetation, material, environment and lighting inspectors edit and undo their domain source",
    );

    await view.click('[data-document="valley-terrain"]');
    await ready(view);
    await view.click(".project-footer button");
    const operations = [
      { kind: "terrain.widenValley", target: "valley-terrain", intervention: "river-valley", width: 80 },
      {
        kind: "world.placeForest",
        target: "winter-valley",
        rule: "conifer-line",
        definition: "alpine-pine",
        spacing: 10,
        density: 0.65,
        seed: 42,
      },
    ];
    await input(view, "#agent-commands", JSON.stringify(operations));
    await view.click(".agent-dialog button.primary");
    await ready(view);
    assert(
      await view.evaluate(
        'wrela.inspect("valley-terrain").interventions.find(i=>i.id==="river-valley").radius === 40',
      ),
      "Agent valley operation failed",
    );
    assert(
      await view.evaluate(
        'wrela.inspect("winter-valley").populations.find(p=>p.id==="conifer-line").density === 0.65',
      ),
      "Agent forest operation failed",
    );
    await view.click('button[aria-label="Close agent operations"]');
    const staleRejected = await view.evaluate<boolean>(
      '(()=>{try{wrela.apply({expectedRevision:0,operations:[{kind:"document.rename",target:"valley-terrain",name:"stale"}]});return false}catch{return true}})()',
    );
    assert(staleRejected, "Stale agent revision was accepted");
    checkpoint("Agent dialog executes atomic valley and forest batch; stale revisions are rejected");

    await view.click('button[aria-label="Save project"]');
    await waitFor(view, "wrela.inspect().savedRevision === wrela.inspect().revision");
    const savedName = await view.evaluate<string>('wrela.inspect("valley-terrain").name');
    await input(view, 'input[aria-label="Definition name"]', "Unsaved verification change");
    await ready(view);
    await view.evaluate("wrela.reopen()");
    await ready(view);
    assert(
      (await view.evaluate<string>('wrela.inspect("valley-terrain").name')) === savedName,
      "Reopen failed to restore the saved source",
    );
    await view.reload();
    await ready(view);
    assert(
      await view.evaluate(
        'wrela.inspect("valley-terrain").interventions.find(i=>i.id==="river-valley").radius === 40',
      ),
      "Saved intervention did not survive fresh page initialization",
    );
    checkpoint("IndexedDB save, reopen and reload preserve semantic changes");

    await view.click('[data-document="winter-valley"]');
    await ready(view);
    const runtimeSource = await view.evaluate<string>("wrela.project()");
    await view.evaluate('wrela.world.removeInstance("stone-instance")');
    assert(
      await view.evaluate(
        'wrela.world.inspect().overrides.some(([id,value])=>id==="stone-instance"&&value.removed)',
      ),
      "Occurrence hide did not enter runtime state",
    );
    await clickText(view, "Save runtime");
    await waitFor(view, 'document.body.innerText.includes("Runtime changes saved")');
    await clickText(view, "Reset runtime");
    await ready(view);
    assert(
      await view.evaluate("wrela.world.inspect().overrides.length===0"),
      "Runtime reset retained hidden occurrences",
    );
    await clickText(view, "Restore runtime");
    await waitFor(
      view,
      'wrela.world.inspect().overrides.some(([id,value])=>id==="stone-instance"&&value.removed)',
    );
    assert(
      (await view.evaluate<string>("wrela.project()")) === runtimeSource,
      "Runtime occurrence controls changed authored source",
    );
    await view.evaluate("wrela.world.reset()");
    await ready(view);
    checkpoint("Runtime occurrence removal, save, reset and restore preserve authored source");

    await view.evaluate("wrela.preview.travel(640,128)");
    await ready(view);
    const travel = await view.evaluate<{
      world: { resident: number; failures: string[] };
      frame: { gpuBytes: number };
    }>("wrela.measure()");
    assert(
      travel.world.resident <= 192 && travel.world.failures.length === 0,
      "Travel violated terrain residency or readiness",
    );
    assert(travel.frame.gpuBytes <= 256 * 1024 * 1024, "Travel violated GPU memory budget");
    checkpoint("Teleport prepares a destination beyond the initial region with bounded residency");

    const seeking = await view.evaluate<{ statuses: string[]; time: number; seeking: boolean }>(
      `(async()=>{const tasks=await Promise.allSettled([wrela.preview.seek(25),wrela.preview.seek(0.5)]);return {statuses:tasks.map(task=>task.status),time:wrela.preview.inspect().time,seeking:wrela.preview.inspect().seeking}})()`,
    );
    assert(
      seeking.statuses[1] === "fulfilled" && seeking.time === 0.5 && !seeking.seeking,
      `Latest asynchronous seek did not win: ${JSON.stringify(seeking)}`,
    );
    await ready(view);
    checkpoint("Superseded asynchronous replay yields to the latest seek without leaving preview busy");

    const channels = await view.evaluate<{
      keys: string[];
      sizes: number[];
      metadata: { channels: string[] };
    }>(
      `wrela.preview.capture({channels:['beauty','depth','lod'],tick:45}).then(result=>({keys:Object.keys(result.images),sizes:Object.values(result.images).map(blob=>blob.size),metadata:result.metadata}))`,
    );
    assert(
      channels.keys.length === 3 && channels.sizes.every((size) => size > 1000),
      "Multi-channel capture did not return three populated images",
    );
    assert(
      await view.evaluate<boolean>("wrela.preview.capture({revision:-1}).then(()=>false,()=>true)"),
      "Capture accepted a superseded source revision",
    );
    const plainFront = await view.evaluate<string>(
      `wrela.preview.capture({subject:'polar-bunny',stage:'studio',tick:30,camera:'front',channels:['beauty'],overlays:[]}).then(async result=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',await result.blob.arrayBuffer()))).join(','))`,
    );
    const named = await view.evaluate<{
      keys: string[];
      sizes: number[];
      metadata: {
        overlays: string[];
        identities: unknown[];
        diagnostics: { segments: unknown[] };
        waterApproximations: { spacing: number; maxHeightError: number }[];
      };
    }>(
      `wrela.preview.capture({subject:'polar-bunny',stage:'studio',tick:30,camera:'front',channels:['beauty','identity','silhouette'],overlays:['rig','colliders']}).then(result=>(window.namedCapture=result,{keys:Object.keys(result.images),sizes:Object.values(result.images).map(blob=>blob.size),metadata:result.metadata}))`,
    );
    assert(
      named.keys.length === 3 &&
        named.sizes.every((size) => size > 1000) &&
        named.metadata.overlays.length === 2 &&
        named.metadata.identities.length > 0 &&
        named.metadata.diagnostics.segments.length > 0,
      "Named camera capture omitted channels, identity map or physical overlays",
    );
    const overlaidFront = await view.evaluate<string>(
      `(async()=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',await window.namedCapture.images.beauty.arrayBuffer()))).join(','))()`,
    );
    assert(
      plainFront !== overlaidFront,
      "Visible diagnostic overlays did not change front-camera bunny pixels",
    );
    for (const channel of named.keys) {
      const data = await view.evaluate<string>(
        `new Promise(resolve=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result.split(',')[1]);reader.readAsDataURL(window.namedCapture.images[${JSON.stringify(channel)}]);})`,
      );
      await Bun.write(join(output, `front-overlay-${channel}.png`), Buffer.from(data, "base64"));
    }
    const superseded = await view.evaluate<boolean>(
      `(async()=>{const capture=wrela.preview.capture({tick:60});wrela.apply({expectedRevision:wrela.inspect().revision,operations:[{kind:'document.rename',target:'valley-terrain',name:'Superseding capture'}]});const rejected=await capture.then(()=>false,()=>true);wrela.undo();return rejected})()`,
    );
    assert(superseded, "In-flight capture accepted a changed source revision");
    await ready(view);
    checkpoint(
      "Named-camera beauty/identity/silhouette and rig/collider overlay capture; stale and in-flight superseded revisions rejected",
    );

    await setViewport(view, 390, 844);
    await Bun.sleep(100);
    const mobile = await view.evaluate<{
      width: number;
      viewport: number;
      scroll: number;
      saveVisible: boolean;
    }>(
      `(()=>{const r=document.querySelector('button[aria-label="Save project"]').getBoundingClientRect();return {viewport:innerWidth,width:document.documentElement.clientWidth,scroll:document.documentElement.scrollWidth,saveVisible:r.width>0&&r.left>=0&&r.right<=innerWidth}})()`,
    );
    await Bun.write(join(output, "studio-mobile.png"), await view.screenshot());
    assert(
      mobile.scroll <= mobile.width + 1 && mobile.saveVisible,
      `390px layout overflows or hides save: ${JSON.stringify(mobile)}`,
    );
    await view.click('button[aria-label="Save project"]');
    await setViewport(view, 1440, 960);
    checkpoint("390px responsive layout has no horizontal overflow and retains usable save controls");

    const custom = JSON.parse(await view.evaluate<string>("wrela.project()"));
    custom.id = "verification-imported-project";
    custom.name = "</script><script>globalThis.__exportInjection=1</script>";
    custom.documents.find((document: { kind: string }) => document.kind === "character").name =
      "</script><script>globalThis.__sourceInjection=1</script>";
    const customFile = join(output, "custom-project.json");
    await Bun.write(customFile, JSON.stringify(custom));
    const dom = (await view.cdp("DOM.getDocument")) as { root: { nodeId: number } };
    const fileInput = (await view.cdp("DOM.querySelector", {
      nodeId: dom.root.nodeId,
      selector: 'input[aria-label="Import project file"]',
    })) as { nodeId: number };
    await view.cdp("DOM.setFileInputFiles", { nodeId: fileInput.nodeId, files: [customFile] });
    await waitFor(view, `wrela.inspect().project.id === ${JSON.stringify(custom.id)}`);
    await ready(view);
    assert(
      (await view.evaluate<string>("wrela.inspect().project.id")) === custom.id,
      "Custom project import did not install the requested identity",
    );
    await view.click('button[aria-label="Save project"]');
    await waitFor(view, "wrela.inspect().savedRevision===wrela.inspect().revision");
    await view.reload();
    await ready(view);
    assert(
      (await view.evaluate<string>("wrela.inspect().project.id")) === custom.id,
      "Fresh browser initialization did not reopen the custom saved project",
    );
    checkpoint("Imported custom project identity persists across save and fresh page initialization");

    await view.evaluate("navigator.serviceWorker.ready");
    await waitFor(view, "!!navigator.serviceWorker.controller");
    await view.cdp("Network.enable");
    await view.cdp("Network.emulateNetworkConditions", {
      offline: true,
      latency: 0,
      downloadThroughput: 0,
      uploadThroughput: 0,
    });
    await view.reload();
    await ready(view);
    assert(
      (await view.evaluate<string>("wrela.inspect().project.id")) === custom.id,
      "Offline snapshot failed to restore the saved project",
    );
    await Bun.write(join(output, "studio-offline.png"), await view.screenshot());
    await view.cdp("Network.emulateNetworkConditions", {
      offline: false,
      latency: 0,
      downloadThroughput: -1,
      uploadThroughput: -1,
    });
    checkpoint("Installed production service-worker snapshot reloads with network disabled");

    const html = await view.evaluate<string>("wrela.export().then(blob=>blob.text())");
    assert(
      html.includes("wrela-project") &&
        html.includes("river-valley") &&
        html.includes("Apache License") &&
        html.includes("MIT License"),
      "Standalone export does not contain source documents",
    );
    const exported = join(output, "standalone-world.html");
    await Bun.write(exported, html);
    await server.stop();
    await view.navigate(pathToFileURL(exported).href);
    await waitFor(
      view,
      'window.wrelaPlayer?.ready || document.querySelector("[data-player-error]") || document.querySelector(".message")?.textContent.includes("Error")',
      60000,
    );
    const player = await view.evaluate<{ ready: boolean; error?: string }>(
      "window.wrelaPlayer || {ready:false,error:document.body.innerText}",
    );
    assert(
      (await view.evaluate<string>(
        'JSON.parse(document.getElementById("wrela-project").textContent).name',
      )) === custom.name,
      "Export escaping changed the authored project name",
    );
    assert(
      await view.evaluate("!globalThis.__exportInjection && !globalThis.__sourceInjection"),
      "Exported source text executed as script",
    );
    assert(player.ready, `Exported player failed without the development server: ${player.error}`);
    for (const kind of ["material", "lighting", "stage", "world"]) {
      await view.evaluate(
        `(()=>{const project=JSON.parse(document.getElementById('wrela-project').textContent);const select=document.querySelector('select[aria-label="View subject"]');select.value=project.documents.find(d=>d.kind===${JSON.stringify(kind)}).id;select.dispatchEvent(new Event('change',{bubbles:true}));})()`,
      );
      await waitFor(
        view,
        `document.querySelector('.badge').textContent==='Ready'||document.querySelector('[data-player-error]')`,
      );
      assert(
        await view.evaluate("!document.querySelector('[data-player-error]')"),
        `Standalone ${kind} subject failed`,
      );
      await view.evaluate("new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))");
      assert(
        await view.evaluate("!document.querySelector('[data-player-error]')"),
        `Standalone ${kind} subject failed during its first presented frame`,
      );
      await waitFor(view, "wrelaPlayer.measure().frame.triangles>0");
    }
    await clickText(view, "Travel east");
    await waitFor(view, "!wrelaPlayer.inspect().preparing || document.querySelector('[data-player-error]')");
    assert(
      await view.evaluate(
        "!document.querySelector('[data-player-error]') && wrelaPlayer.inspect().camera.target[0]===128 && document.querySelector('.badge').textContent==='Ready'",
      ),
      "Exported player Travel east failed to prepare its destination",
    );
    const playerTravel = await view.evaluate<{
      busy: boolean;
      rejectedConcurrent: boolean;
      target: number[];
      pending: boolean;
      failures: string[];
    }>(
      `(async()=>{const movement=wrelaPlayer.travel(256,0);const busy=wrelaPlayer.inspect().preparing;const rejectedConcurrent=await wrelaPlayer.travel(384,0).then(()=>false,()=>true);await movement;return {busy,rejectedConcurrent,target:wrelaPlayer.inspect().camera.target,pending:wrelaPlayer.measure().world.pending,failures:wrelaPlayer.measure().world.failures}})()`,
    );
    assert(
      playerTravel.busy &&
        playerTravel.rejectedConcurrent &&
        playerTravel.target[0] === 256 &&
        playerTravel.target[2] === 0 &&
        !playerTravel.pending &&
        playerTravel.failures.length === 0,
      "Exported player API travel did not serialize a coherent ready destination",
    );
    await clickText(view, "Travel east");
    await waitFor(view, "!wrelaPlayer.inspect().preparing || document.querySelector('[data-player-error]')");
    assert(
      await view.evaluate(
        "!document.querySelector('[data-player-error]') && wrelaPlayer.inspect().camera.target[0]===384 && document.querySelector('.badge').textContent==='Ready'",
      ),
      "Repeated exported player travel failed",
    );
    await Bun.write(join(output, "standalone-travel.png"), await view.screenshot());
    checkpoint(
      "Exported local-file player repeated native/API travel holds evaluation, rejects concurrent transitions and prepares coherent destinations",
    );
    await clickText(view, "Pause");
    await view.cdp("Browser.setDownloadBehavior", { behavior: "allow", downloadPath: output });
    await clickText(view, "Export runtime");
    const runtimeFile = join(output, `${custom.id}-runtime.json`);
    for (let attempt = 0; attempt < 100 && !(await Bun.file(runtimeFile).exists()); attempt++)
      await Bun.sleep(50);
    assert(await Bun.file(runtimeFile).exists(), "Player did not download runtime JSON");
    const runtimeDOM = (await view.cdp("DOM.getDocument")) as { root: { nodeId: number } };
    const runtimeInput = (await view.cdp("DOM.querySelector", {
      nodeId: runtimeDOM.root.nodeId,
      selector: 'input[aria-label="Runtime save file"]',
    })) as { nodeId: number };
    await view.cdp("DOM.setFileInputFiles", { nodeId: runtimeInput.nodeId, files: [runtimeFile] });
    await waitFor(
      view,
      "(!wrelaPlayer.inspect().preparing && document.querySelector('.badge').textContent==='Ready')||document.querySelector('[data-player-error]')",
    );
    assert(
      await view.evaluate("!document.querySelector('[data-player-error]')"),
      "Player runtime JSON import failed",
    );
    checkpoint(
      "Standalone material/lighting/stage/world switching and runtime JSON download/import roundtrip",
    );
    await Bun.write(join(output, "standalone-player.png"), await view.screenshot());
    checkpoint("Exported HTML runs from local disk after the development server is stopped");
    const winterGame = await verifyWinterGame(view, output);
    return { checks, capture, channels, named, mobile, travel, playerTravel, exported, winterGame };
  } catch (error) {
    await Bun.write(join(output, "ui-failure.png"), await view.screenshot());
    console.error(await view.evaluate("document.body.innerText"));
    throw error;
  } finally {
    await server.stop();
  }
}
