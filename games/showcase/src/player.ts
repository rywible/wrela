import { type CookedProject, compileDocument, createCookedCompiler } from "@wrela/compiler";
import { BrowserCompiler } from "@wrela/compiler/client";
import { createForestEdge, forestEdgeCamera, referenceProject } from "@wrela/examples";
import { createWaterLookdev, waterLakeCamera, waterStudyCameras } from "@wrela/examples/water-lookdev";
import { createWaterValley, waterValleyCameras } from "@wrela/examples/water-valley";
import { type Camera, contentKey, type Project, parseProject, type Quality, type Vec3 } from "@wrela/model";
import { type RenderQuality, WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost, cameraRay, pickScene } from "@wrela/runtime";
import { CreatureStudyPlayer, selectedCreatureStudy } from "../../creature-study/src/player";
import { creatureStyle } from "../../creature-study/src/style";
import { WinterPlayer } from "../../winter-valley/src/player";
import { winterStyle } from "../../winter-valley/src/style";

const forestStudy = new URLSearchParams(location.search).has("forest");
let forestWalking = false,
  forestWalkTime = 0;
const waterStudy = new URLSearchParams(location.search).get("water");
const style = document.createElement("style");
style.textContent = `*{box-sizing:border-box}body{margin:0;background:#213543;color:#dcebf2;font:13px 'Avenir Next','Segoe UI',sans-serif}canvas{display:block;width:100vw;height:100dvh;touch-action:none}header{position:fixed;top:20px;left:24px;right:24px;display:flex;align-items:center;gap:18px;pointer-events:none;text-shadow:0 1px 3px #1239}header strong{font-size:19px;font-weight:600}header span{font-size:11px;opacity:.7}.controls{position:fixed;bottom:22px;left:24px;right:24px;display:flex;gap:8px;align-items:center;flex-wrap:wrap}button,select{font:inherit;background:#213b4cd9;border:1px solid #accbdb55;color:inherit;padding:9px 13px;border-radius:5px;cursor:pointer;backdrop-filter:blur(6px)}button:hover{background:#456575}button:focus-visible,select:focus-visible{outline:2px solid #d7edf8}select{max-width:200px}.hint{margin-left:auto;background:#213b4c99;padding:9px 12px;border-radius:5px;font-size:10px}.message{position:fixed;top:50%;left:50%;transform:translate(-50%,-50%);max-width:500px;text-align:center;line-height:1.8;padding:22px;background:#213b4cee;border:1px solid #638595;border-radius:9px}input[type=file]{display:none}.badge{padding:5px 9px;border:1px solid #d2e9f655;border-radius:20px;font-size:10px}@media(max-width:600px){.hint{display:none}header{left:15px;top:15px}.controls{left:15px;right:15px;bottom:15px;gap:5px}button,select{padding:8px;font-size:11px}}`;
style.textContent += winterStyle + creatureStyle;
document.head.append(style);
const canvas = document.createElement("canvas");
canvas.setAttribute("aria-label", "Wrela standalone world viewer. Drag to orbit and scroll to zoom.");
canvas.tabIndex = 0;
document.body.append(canvas);
const header = document.createElement("header");
const title = document.createElement("strong");
header.append(title);
const mark = document.createElement("span");
mark.textContent = "Wrela Player";
header.append(mark);
const badge = document.createElement("span");
badge.className = "badge";
badge.textContent = "Preparing";
header.append(badge);
document.body.append(header);
const controls = document.createElement("div");
controls.className = "controls";
document.body.append(controls);
const message = document.createElement("div");
message.className = "message";
message.textContent = "Preparing your world…";
document.body.append(message);
let host: BrowserSceneHost,
  renderer: WebGPURenderer,
  project: Project,
  playing = true,
  time = 0,
  frame = 0,
  last = 0,
  dirty = true,
  preparing = false;
let camera: Camera = { position: [24, 17, 32], target: [0, 1, 0], fov: 48 };
let transitionGeneration = 0,
  disposed = false,
  activeSubject = "";
let workerUrl: string | undefined, compiler: BrowserCompiler | undefined;
let winter: WinterPlayer | undefined;
let creature: CreatureStudyPlayer | undefined;
let deliveryQuality: Quality = "review";
let launchMs: number | null = null;
const launchPhases: Record<string, number> = {};
let deliveryCompilerSource: string | undefined;
const cookedDocuments = new Set<string>(),
  sourceCompiledDocuments = new Set<string>();
function deliveryState() {
  return {
    launchMs,
    launchPhases,
    mode: cookedDocuments.size ? (sourceCompiledDocuments.size ? "mixed" : "cooked") : "source",
    cookedDocuments: cookedDocuments.size,
    sourceCompiledDocuments: sourceCompiledDocuments.size,
    compilerSource: deliveryCompilerSource,
    cookedDocumentIds: [...cookedDocuments],
    sourceCompiledDocumentIds: [...sourceCompiledDocuments],
  };
}
const graphicsChoices = [
  ["low", "Performance"],
  ["balanced", "Balanced"],
  ["high", "High quality"],
] as const;
let preferredGraphics: RenderQuality = "balanced";
try {
  const saved = localStorage.getItem("wrela-render-quality");
  if (graphicsChoices.some(([key]) => key === saved)) preferredGraphics = saved as RenderQuality;
} catch {
  /* Graphics still work when preference storage is unavailable. */
}
function graphicsState() {
  return {
    profile: renderer?.qualityProfile ?? preferredGraphics,
    changing: preparing,
    available: graphicsChoices.map(([id, label]) => ({ id, label })),
  };
}
async function setGraphics(profile: RenderQuality) {
  if (!graphicsChoices.some(([key]) => key === profile)) throw new Error("Unknown graphics quality");
  if (!renderer || preparing || winter?.busy || creature?.busy)
    throw new Error("Wait for the player to finish preparing");
  if (renderer.qualityProfile === profile) return graphicsState();
  if (winter?.active && !winter.rules.paused) winter.pause();
  if (creature?.active) creature.pause();
  const previous = renderer.qualityProfile;
  await transition(
    "Changing graphics",
    async () => {
      const scene = creature?.active
        ? creature.scene()
        : winter?.active
          ? winter.scene()
          : host.extract(camera);
      renderer.dispose();
      try {
        renderer = await WebGPURenderer.create(canvas, { quality: profile });
        renderer.render(scene);
        await renderer.capture();
      } catch (error) {
        renderer.dispose();
        renderer = await WebGPURenderer.create(canvas, { quality: previous });
        renderer.render(scene);
        await renderer.capture();
        throw error;
      }
    },
    () => {
      preferredGraphics = profile;
      graphicsSelect.value = profile;
      try {
        localStorage.setItem("wrela-render-quality", profile);
      } catch {
        /* The active preference is still applied. */
      }
    },
  );
  return graphicsState();
}
function trail() {
  if (!winter) throw new Error("Wait for the player to finish preparing");
  return winter;
}
function creatureStudy() {
  if (!creature) throw Error("Wait for the player to finish preparing");
  return creature;
}
function report(error: unknown) {
  if (disposed) return;
  message.dataset.playerError = "true";
  message.textContent = String(error);
  message.style.display = "block";
  badge.textContent = "Needs attention";
}
async function transition<T>(
  label: string,
  work: () => Promise<T>,
  publish: (result: T) => void,
): Promise<T> {
  if (disposed) throw new DOMException("Player is closed", "AbortError");
  if (preparing) throw Error("Wait for the current preview to finish preparing");
  const generation = ++transitionGeneration;
  const current = () => !disposed && generation === transitionGeneration;
  const inputs = [
    ...document.querySelectorAll<HTMLButtonElement | HTMLSelectElement>(
      ".controls button, .controls select, .winter-panel button, .winter-panel select",
    ),
  ];
  const disabled = inputs.map((input) => input.disabled);
  preparing = true;
  drag = undefined;
  canvas.setAttribute("aria-busy", "true");
  inputs.forEach((input) => {
    input.disabled = true;
  });
  badge.textContent = label;
  try {
    const result = await work();
    if (!current()) throw new DOMException("Player transition superseded", "AbortError");
    publish(result);
    dirty = true;
    badge.textContent = "Ready";
    delete message.dataset.playerError;
    message.style.display = "none";
    return result;
  } catch (error) {
    if (current()) report(error);
    throw error;
  } finally {
    if (current()) {
      preparing = false;
      last = performance.now();
      // A failed destination restores the previous camera's region interest on
      // the next frame; no old-camera evaluation runs while work is pending.
      dirty = true;
      canvas.removeAttribute("aria-busy");
      inputs.forEach((input, index) => {
        input.disabled = disabled[index];
      });
    }
  }
}
async function travel(x: number, z: number) {
  if (winter?.active || creature?.active) throw new Error("Return to the viewer before using viewer travel");
  return transition(
    "Preparing destination",
    async () => {
      if (!Number.isFinite(x) || !Number.isFinite(z)) throw Error("Travel coordinates must be finite");
      const ready = await host.teleport([x, 0, z]);
      if (!ready.ready) throw Error(`Missing regions: ${ready.missing.join(", ")}`);
      return ready;
    },
    () => {
      camera = { ...camera, position: [x + 24, 17, z + 32], target: [x, 1, z] };
    },
  );
}
function button(text: string, fn: () => unknown) {
  const el = document.createElement("button");
  el.textContent = text;
  if (
    (waterStudy || forestStudy) &&
    ["Travel east", "Save runtime", "Restore runtime", "Export runtime", "Import runtime"].includes(text)
  )
    el.hidden = true;
  el.addEventListener("click", () => {
    try {
      Promise.resolve(fn()).catch(report);
    } catch (e) {
      report(e);
    }
  });
  controls.append(el);
  return el;
}
const explore = button("Play Winter valley", async () => {
  if (creature?.active || creature?.busy) throw Error("Return to the viewer before opening Winter valley");
  await transition(
    "Opening trail",
    () => trail().start(true),
    () => {},
  );
});
explore.className = "game-launch";
async function startCreature(id = selectedCreatureStudy(project, activeSubject) ?? activeSubject) {
  if (!creature) throw Error("Wait for the player to finish preparing");
  if (winter?.active || winter?.busy) throw Error("Return to the viewer before opening a creature study");
  return transition(
    "Opening creature encounter",
    () => creatureStudy().start(id),
    () => {},
  );
}
const study = button("Study encounter", () => startCreature());
study.hidden = true;
const play = button("Pause", () => {
  playing = !playing;
  play.textContent = playing ? "Pause" : "Play";
});
const graphicsLabel = document.createElement("label");
graphicsLabel.className = "graphics-control";
graphicsLabel.textContent = "Graphics ";
const graphicsSelect = document.createElement("select");
graphicsSelect.setAttribute("aria-label", "Graphics quality");
for (const [id, label] of graphicsChoices) {
  const option = document.createElement("option");
  option.value = id;
  option.textContent = label;
  graphicsSelect.append(option);
}
graphicsSelect.value = preferredGraphics;
graphicsSelect.addEventListener("change", () => {
  void setGraphics(graphicsSelect.value as RenderQuality).catch((error) => {
    graphicsSelect.value = renderer.qualityProfile;
    report(error);
  });
});
graphicsLabel.append(graphicsSelect);
controls.append(graphicsLabel);
button("Reset view", () => {
  camera =
    activeSubject === "water-study-valley"
      ? structuredClone(waterValleyCameras.bank)
      : waterStudy && waterStudyCameras[activeSubject]
        ? structuredClone(waterStudyCameras[activeSubject])
        : { position: [24, 17, 32], target: [0, 1, 0], fov: 48 };
  dirty = true;
});
button("Travel east", () => travel(camera.target[0] + 128, camera.target[2]));
const select = document.createElement("select");
select.setAttribute("aria-label", "View subject");
controls.append(select);
select.addEventListener("change", () => {
  void openSubject(select.value).catch(report);
});
button("Save runtime", () => {
  if (!host.world) throw Error("Open a world before saving runtime state");
  localStorage.setItem(`wrela-player-save:${project.id}`, JSON.stringify(host.saveRuntime()));
  badge.textContent = "Runtime saved";
});
async function restoreRuntime(saved: Parameters<BrowserSceneHost["loadRuntime"]>[0]) {
  if (!host.world) throw Error("Open the world before restoring");
  await transition(
    "Restoring runtime",
    () => host.loadRuntime(saved),
    (restored) => {
      time = restored.time;
    },
  );
}
button("Restore runtime", async () => {
  const saved = localStorage.getItem(`wrela-player-save:${project.id}`);
  if (!saved) throw Error("No runtime save exists for this project");
  await restoreRuntime(JSON.parse(saved));
});
button("Export runtime", () => {
  if (!host.world) throw Error("Open a world before exporting runtime state");
  const blob = new Blob([JSON.stringify(host.saveRuntime(), null, 2)], { type: "application/json" });
  if (blob.size > 8 * 1024 * 1024) throw Error("Runtime save exceeds the 8 MiB file limit");
  const url = URL.createObjectURL(blob),
    link = document.createElement("a");
  link.href = url;
  link.download = `${project.id}-runtime.json`;
  document.body.append(link);
  try {
    link.click();
    badge.textContent = "Runtime exported";
  } finally {
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
});
const runtimeFile = document.createElement("input");
runtimeFile.type = "file";
runtimeFile.accept = ".json,application/json";
runtimeFile.setAttribute("aria-label", "Runtime save file");
document.body.append(runtimeFile);
runtimeFile.addEventListener("change", () => {
  const file = runtimeFile.files?.[0];
  if (!file) return;
  void (async () => {
    try {
      if (file.size > 8 * 1024 * 1024) throw Error("Runtime save files must be 8 MiB or smaller");
      await restoreRuntime(JSON.parse(await file.text()));
    } catch (error) {
      report(error);
    } finally {
      runtimeFile.value = "";
    }
  })();
});
button("Import runtime", () => {
  if (!host.world) throw Error("Open a world before importing runtime state");
  runtimeFile.click();
});
function activeWaterId() {
  const subject = project.documents.find((document) => document.id === activeSubject);
  return subject?.kind === "world" ? subject.water : subject?.kind === "water" ? subject.id : undefined;
}
const ripple = waterStudy
  ? button("Make a ripple", () => {
      if (!host.runtime?.waterBodies.get(activeWaterId() ?? "")?.simulation) return;
      host.runtime.disturbWater(activeWaterId()!, camera.target[0], camera.target[2], 1.4, 2.5);
      playing = true;
      play.textContent = "Pause";
    })
  : undefined;
const forestWalk = forestStudy
  ? button("Walk the trail", () => {
      forestWalking = !forestWalking;
      if (forestWalkTime >= 24) forestWalkTime = 0;
      forestWalk!.textContent = forestWalking ? "Stop walking" : "Walk the trail";
      playing = true;
      play.textContent = "Pause";
    })
  : undefined;
forestWalk?.setAttribute("data-forest-walk", "true");
const lakeView = waterStudy
  ? button("Lake view", () => {
      camera = structuredClone(
        activeSubject === "water-study-valley" ? waterValleyCameras.pool : waterLakeCamera,
      );
      dirty = true;
    })
  : undefined;
canvas.addEventListener("dblclick", (event) => {
  if (!waterStudy || preparing || !host.runtime?.waterBodies.get(activeWaterId() ?? "")?.simulation) return;
  const rect = canvas.getBoundingClientRect();
  const scene = host.extract(camera);
  const relativeCamera = {
    ...camera,
    position: camera.position.map((value, i) => value - (scene.origin?.[i] ?? 0)) as Vec3,
    target: camera.target.map((value, i) => value - (scene.origin?.[i] ?? 0)) as Vec3,
  };
  const hit = pickScene(
    scene,
    cameraRay(relativeCamera, event.clientX - rect.left, event.clientY - rect.top, rect.width, rect.height),
  );
  if (hit && hit.surfaceId === activeWaterId()) {
    host.runtime.disturbWater(
      activeWaterId()!,
      hit.position[0] + (scene.origin?.[0] ?? 0),
      hit.position[2] + (scene.origin?.[2] ?? 0),
      1.4,
      2.5,
    );
    playing = true;
    play.textContent = "Pause";
  }
});
const hint = document.createElement("span");
hint.className = "hint";
hint.textContent = forestStudy
  ? "Walk the trail · Drag to inspect · Scroll to zoom"
  : waterStudy
    ? "Double-click water for ripples · Drag to orbit · Scroll to zoom"
    : "Drag to orbit · Shift drag to pan · Scroll to zoom";
controls.append(hint);
async function openSubject(id: string) {
  if (creature?.active || creature?.busy) throw Error("Return to the viewer before changing subjects");
  try {
    await transition(
      "Preparing",
      () => host.prepare(id, "neutral-stage", deliveryQuality),
      () => {
        const doc = project.documents.find((d) => d.id === id);
        camera =
          forestStudy && id === "forest-edge"
            ? forestEdgeCamera()
            : id === "water-study-valley"
              ? structuredClone(waterValleyCameras.bank)
              : waterStudy && waterStudyCameras[id]
                ? structuredClone(waterStudyCameras[id])
                : doc && ["world", "terrain"].includes(doc.kind)
                  ? { position: [24, 17, 32], target: [0, 1, 0], fov: 48 }
                  : doc?.kind === "vegetation"
                    ? { position: [13, 8, 16], target: [0, 3, 0], fov: 42 }
                    : doc?.kind === "stage"
                      ? structuredClone(doc.camera)
                      : { position: [5, 3.2, 7], target: [0, 1.2, 0], fov: 42 };
        activeSubject = id;
        select.value = id;
        study.hidden = !selectedCreatureStudy(project, id);
      },
    );
    // Apply subject-specific availability after the transition restores its controls.
    const water = project.documents.find((document) => document.id === activeWaterId());
    if (ripple) ripple.disabled = water?.kind !== "water" || !water.domain?.simulate;
    if (lakeView) lakeView.disabled = water?.kind !== "water" || !water.domain;
  } catch (error) {
    if (!disposed && activeSubject) select.value = activeSubject;
    throw error;
  }
}
async function start() {
  try {
    launchPhases.moduleReady = performance.now();
    const embedded = document.getElementById("wrela-project")?.textContent;
    project = parseProject(
      embedded
        ? JSON.parse(embedded)
        : forestStudy
          ? createForestEdge()
          : waterStudy
            ? waterStudy === "valley"
              ? createWaterValley()
              : createWaterLookdev(
                  waterStudy === "ocean" ? "ocean" : waterStudy === "surf" ? "surf" : "creek",
                )
            : referenceProject(),
    );
    title.textContent = project.name;
    for (const doc of project.documents) {
      if (forestStudy && doc.id !== "forest-edge") continue;
      if (waterStudy && doc.kind !== "water" && doc.id !== "water-study-valley") continue;
      const option = document.createElement("option");
      option.value = doc.id;
      option.textContent = doc.name;
      select.append(option);
    }
    select.value = project.entry;
    const workerSource = document.getElementById("wrela-worker")?.textContent;
    if (workerSource) {
      workerUrl = URL.createObjectURL(new Blob([workerSource], { type: "text/javascript" }));
      compiler = new BrowserCompiler(workerUrl, 2, "classic");
    } else if (location.protocol !== "file:") {
      compiler = new BrowserCompiler("/compile-worker.js");
    }
    let cooked: unknown;
    const embeddedCooked = document.getElementById("wrela-cooked")?.textContent;
    if (embeddedCooked) cooked = JSON.parse(embeddedCooked);
    else if (location.protocol !== "file:") {
      try {
        const response = await fetch("/cooked-project.json");
        if (response.ok) cooked = await response.json();
      } catch {
        /* Development servers may not publish cooked products. */
      }
    }
    const sourceCompile = async (
      document: Parameters<typeof compileDocument>[0],
      quality: Parameters<typeof compileDocument>[1],
    ) => {
      const artifact = compiler
        ? await compiler.compile(document, quality ?? "review")
        : compileDocument(document, quality);
      if (artifact) sourceCompiledDocuments.add(document.id);
      return artifact;
    };
    let compile = sourceCompile;
    launchPhases.sourceReady = performance.now();
    if (cooked) {
      try {
        const manifest = cooked as CookedProject;
        const provider = createCookedCompiler(manifest, project, { fallback: sourceCompile });
        compile = async (document, quality) => {
          const result = await provider(document, quality ?? "review");
          if (
            result &&
            quality === manifest.quality &&
            manifest.entries.some(
              (entry) => entry.document === document.id && entry.source === contentKey(document),
            )
          )
            cookedDocuments.add(document.id);
          return result;
        };
        deliveryCompilerSource = manifest.compilerSource;
        deliveryQuality = manifest.quality;
      } catch (error) {
        if (embeddedCooked) throw error;
      }
    }
    host = new BrowserSceneHost(project, {
      compile,
      ...(compiler
        ? { generateTerrain: compiler.generateTerrain, growVegetation: compiler.growVegetation }
        : {}),
    });
    launchPhases.productsReady = performance.now();
    renderer = await WebGPURenderer.create(canvas, { quality: preferredGraphics });
    launchPhases.rendererReady = performance.now();
    await openSubject(project.entry);
    host.updateView(camera);
    if (host.world?.metrics.pending) {
      const ready = await host.world.prepare();
      if (!ready.ready) throw new Error("The opening world is not ready");
    }
    launchPhases.sceneReady = performance.now();
    renderer.render(host.extract(camera));
    launchPhases.submitted = performance.now();
    await renderer.capture();
    launchMs = performance.now();
    message.style.display = "none";
    winter = new WinterPlayer(
      host,
      project,
      (active) => {
        activeSubject = "winter-valley";
        select.value = activeSubject;
        header.style.display = active ? "none" : "";
        controls.style.display = active ? "none" : "";
        canvas.setAttribute(
          "aria-label",
          active
            ? "Winter valley exploration. Use WASD or arrow keys to move. Hold E near a trail lantern to restore it."
            : "Wrela standalone world viewer. Drag to orbit and scroll to zoom.",
        );
        if (!active) {
          camera = structuredClone(trail().camera);
          time = host.runtime?.clock.time ?? time;
          dirty = true;
        }
      },
      { profile: () => renderer.qualityProfile, change: setGraphics },
    );
    explore.hidden = forestStudy || !winter.supported();
    creature = new CreatureStudyPlayer(
      project,
      {
        compile,
        ...(compiler
          ? { generateTerrain: compiler.generateTerrain, growVegetation: compiler.growVegetation }
          : {}),
      },
      deliveryQuality,
      (active) => {
        header.style.display = active ? "none" : "";
        controls.style.display = active ? "none" : "";
        canvas.setAttribute(
          "aria-label",
          active
            ? "Creature encounter. WASD or arrows move, Space dodges, J strikes, Escape pauses."
            : "Wrela standalone world viewer. Drag to orbit and scroll to zoom.",
        );
        drag = undefined;
        last = performance.now();
        dirty = true;
        canvas.focus();
      },
    );
    frame = requestAnimationFrame(tick);
    (window as any).wrelaPlayer = {
      ready: true,
      measure: () => ({
        frame: renderer.measurements,
        world: host.world?.metrics,
        completeness: renderer.completeness,
        delivery: deliveryState(),
      }),
      inspect: () => ({
        project: project.id,
        time: creature?.active
          ? (creature.inspect().snapshot?.tick ?? 0) / 60
          : (host.runtime?.clock.time ?? time),
        subject: creature?.active ? creature.inspect().subject : activeSubject,
        playing: creature?.active
          ? !creature.inspect().paused && creature.inspect().snapshot?.outcome === "playing"
          : winter?.active
            ? !winter.rules.paused && ["exploring", "returning"].includes(winter.rules.inspect().phase)
            : playing,
        camera: structuredClone(creature?.active ? creature.camera : winter?.active ? winter.camera : camera),
        preparing: preparing || !!winter?.busy || !!creature?.busy,
        graphics: graphicsState(),
      }),
      graphics: { inspect: graphicsState, set: setGraphics },
      game: {
        start: () => {
          if (creature?.active || creature?.busy)
            throw Error("Return to the viewer before opening Winter valley");
          return trail().start();
        },
        inspect: () => trail().inspect(),
        input: (value: Parameters<WinterPlayer["input"]>[0]) => trail().input(value),
        pause: () => trail().pause(),
        resume: () => trail().resume(),
        restart: () => trail().restart(),
        save: () => trail().save(),
        restore: (value: unknown) => trail().restore(value),
        events: () => trail().events(),
        simulate: (options: Parameters<WinterPlayer["simulate"]>[0]) => trail().simulate(options),
        leave: () => trail().leave(),
      },
      creature: {
        start: startCreature,
        inspect: () => creatureStudy().inspect(),
        input: (value: Parameters<CreatureStudyPlayer["input"]>[0]) => creatureStudy().input(value),
        pause: () => creatureStudy().pause(),
        resume: () => creatureStudy().resume(),
        restart: () => creatureStudy().restart(),
        leave: () => creatureStudy().leave(),
      },
      capture: () => renderer.capture(),
      travel,
      dispose,
    };
    if (location.hash === "#play" && winter.supported()) await winter.start(true);
  } catch (e) {
    report(e);
  }
}
function tick(now: number) {
  const delta = Math.max(0, Math.min(0.1, (now - last) / 1000 || 0));
  last = Math.max(last, now);
  if (preparing || winter?.busy || creature?.busy) {
    frame = requestAnimationFrame(tick);
    return;
  }
  if (creature?.active) {
    try {
      creature.update(delta);
      renderer.render(creature.scene());
    } catch (error) {
      creature.pause();
      report(error);
    }
    frame = requestAnimationFrame(tick);
    return;
  }
  if (winter?.active) {
    try {
      winter.update(delta);
      renderer.render(winter.scene());
      time = host.runtime?.clock.time ?? time;
    } catch (error) {
      winter.pause();
      report(error);
    }
    frame = requestAnimationFrame(tick);
    return;
  }
  try {
    if (playing) {
      if (forestWalking) {
        forestWalkTime = Math.min(24, forestWalkTime + delta);
        camera = forestEdgeCamera(forestWalkTime);
        if (forestWalkTime === 24) {
          forestWalking = false;
          if (forestWalk) forestWalk.textContent = "Walk the trail";
        }
      }
      host.advance(delta, camera);
      dirty = true;
    } else host.updateView(camera);
    time = host.runtime?.clock.time ?? time;
    if (dirty || renderer.needsRender || host.world?.metrics.pending) {
      renderer.render(host.extract(camera));
      dirty = false;
    }
  } catch (error) {
    report(error);
    playing = false;
  }
  frame = requestAnimationFrame(tick);
}
let drag: { x: number; y: number } | undefined;
canvas.addEventListener("pointerdown", (e) => {
  if (preparing || disposed || winter?.active || creature?.active) return;
  forestWalking = false;
  if (forestWalk) forestWalk.textContent = "Walk the trail";
  drag = { x: e.clientX, y: e.clientY };
  canvas.setPointerCapture(e.pointerId);
});
canvas.addEventListener("pointerup", () => {
  drag = undefined;
});
canvas.addEventListener("pointermove", (e) => {
  if (!drag || preparing || disposed || winter?.active || creature?.active) return;
  const dx = e.clientX - drag.x,
    dy = e.clientY - drag.y;
  drag = { x: e.clientX, y: e.clientY };
  const d = camera.position.map((v, i) => v - camera.target[i]) as Vec3,
    r = Math.hypot(...d),
    yaw = Math.atan2(d[0], d[2]);
  if (e.shiftKey) {
    const shift: Vec3 = [
      (-dx * Math.cos(yaw) + dy * Math.sin(yaw)) * r * 0.0015,
      0,
      (dx * Math.sin(yaw) + dy * Math.cos(yaw)) * r * 0.0015,
    ];
    camera = {
      ...camera,
      position: camera.position.map((v, i) => v + shift[i]) as Vec3,
      target: camera.target.map((v, i) => v + shift[i]) as Vec3,
    };
  } else {
    const theta = yaw - dx * 0.007,
      phi = Math.max(0.06, Math.min(1.5, Math.acos(d[1] / r) + dy * 0.007));
    camera = {
      ...camera,
      position: [
        camera.target[0] + r * Math.sin(phi) * Math.sin(theta),
        camera.target[1] + r * Math.cos(phi),
        camera.target[2] + r * Math.sin(phi) * Math.cos(theta),
      ],
    };
  }
  dirty = true;
});
canvas.addEventListener(
  "wheel",
  (e) => {
    e.preventDefault();
    if (preparing || disposed || winter?.active || creature?.active) return;
    const factor = Math.exp(Math.max(-0.2, Math.min(0.2, e.deltaY * 0.001)));
    camera = {
      ...camera,
      position: camera.position.map((v, i) => camera.target[i] + (v - camera.target[i]) * factor) as Vec3,
    };
    dirty = true;
  },
  { passive: false },
);
function dispose() {
  disposed = true;
  transitionGeneration++;
  cancelAnimationFrame(frame);
  creature?.dispose();
  winter?.dispose();
  host?.dispose();
  renderer?.dispose();
  compiler?.dispose();
  if (workerUrl) URL.revokeObjectURL(workerUrl);
}
window.addEventListener("pagehide", dispose);
void start();
