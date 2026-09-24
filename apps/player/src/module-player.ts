import { type CookedProject, compileDocument, createCookedCompiler } from "@wrela/compiler";
import { parseProject } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost, type GameDefinition, GameDriver, type GameManifest } from "@wrela/runtime";

export async function launchGamePlayer(definition: GameDefinition, manifest?: GameManifest) {
  const started = performance.now();
  const embedded = document.getElementById("wrela-project");
  const project = parseProject(embedded ? JSON.parse(embedded.textContent ?? "") : definition.project());
  if (manifest && project.entry !== manifest.entry) throw Error("Game entry does not match its source");
  const canvas = document.createElement("canvas");
  canvas.style.cssText = "width:100vw;height:100dvh;display:block";
  document.body.style.margin = "0";
  document.body.append(canvas);
  const hud = document.createElement("div");
  hud.style.cssText =
    "position:fixed;left:24px;top:24px;background:#162531dd;color:#fff;padding:16px;border-radius:8px;font:16px system-ui;max-width:420px";
  const title = document.createElement("strong"),
    objective = document.createElement("p"),
    hint = document.createElement("p");
  title.textContent = definition.title;
  hint.textContent = "WASD / arrows to move · E to interact";
  hud.append(title, objective, hint);
  document.body.append(hud);
  const bundle = document.getElementById("wrela-cooked");
  const cooked: CookedProject | undefined = bundle ? JSON.parse(bundle.textContent ?? "") : undefined;
  const sourceDocuments = new Set<string>(),
    cookedDocuments = new Set<string>();
  const fallback = async (
    doc: Parameters<typeof compileDocument>[0],
    quality: Parameters<typeof compileDocument>[1],
  ) => {
    const artifact = compileDocument(doc, quality);
    if (artifact) sourceDocuments.add(doc.id);
    return artifact;
  };
  const provider = cooked ? createCookedCompiler(cooked, project, { fallback }) : fallback;
  const compile = async (
    doc: Parameters<typeof compileDocument>[0],
    quality: Parameters<typeof compileDocument>[1],
  ) => {
    const artifact = await provider(doc, quality ?? "review");
    if (artifact && !sourceDocuments.has(doc.id)) cookedDocuments.add(doc.id);
    return artifact;
  };
  const host = new BrowserSceneHost(project, { compile });
  let driver: GameDriver | undefined, renderer: WebGPURenderer | undefined;
  const keys = new Set<string>();
  let stopped = false,
    last = performance.now(),
    frame = 0,
    paused = false;
  const input = () =>
    driver?.input({
      move: [
        Number(keys.has("KeyD") || keys.has("ArrowRight")) -
          Number(keys.has("KeyA") || keys.has("ArrowLeft")),
        Number(keys.has("KeyS") || keys.has("ArrowDown")) - Number(keys.has("KeyW") || keys.has("ArrowUp")),
      ],
      interact: keys.has("KeyE"),
    });
  const keydown = (event: KeyboardEvent) => {
    if (
      ["KeyW", "KeyA", "KeyS", "KeyD", "KeyE", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(
        event.code,
      )
    ) {
      event.preventDefault();
      keys.add(event.code);
      input();
    }
  };
  const keyup = (event: KeyboardEvent) => {
    keys.delete(event.code);
    input();
  };
  const blur = () => {
    keys.clear();
    input();
    paused = true;
  };
  const dispose = () => {
    if (stopped) return;
    stopped = true;
    cancelAnimationFrame(frame);
    driver?.dispose();
    host.dispose();
    renderer?.dispose();
    window.removeEventListener("keydown", keydown);
    window.removeEventListener("keyup", keyup);
    window.removeEventListener("blur", blur);
    window.removeEventListener("pagehide", dispose);
    canvas.remove();
    hud.remove();
  };
  try {
    driver = await GameDriver.create(definition, { project, sceneHost: host, quality: cooked?.quality });
    renderer = await WebGPURenderer.create(canvas);
    const active = driver,
      graphics = renderer;
    const button = (label: string, action: () => unknown) => {
      const control = document.createElement("button");
      control.textContent = label;
      control.style.marginRight = "8px";
      control.onclick = () => {
        void Promise.resolve()
          .then(action)
          .catch((error: unknown) => {
            objective.textContent = error instanceof Error ? error.message : String(error);
          });
      };
      hud.append(control);
      return control;
    };
    button("Pause / resume", () => {
      paused = !paused;
      last = performance.now();
    });
    button("Save", async () =>
      localStorage.setItem(`wrela-game:${project.id}:${definition.id}`, JSON.stringify(await active.save())),
    );
    button("Load", async () => {
      const save = localStorage.getItem(`wrela-game:${project.id}:${definition.id}`);
      if (!save) throw Error("No saved journey yet");
      await active.load(JSON.parse(save));
    });
    window.addEventListener("keydown", keydown);
    window.addEventListener("keyup", keyup);
    window.addEventListener("blur", blur);
    window.addEventListener("pagehide", dispose);
    const draw = () => {
      const scene = active.module.scene?.();
      if (scene) graphics.render(scene);
      const state = active.inspect().state as { objective?: string };
      objective.textContent = state.objective ?? definition.description;
    };
    let launchMs: number | undefined;
    Object.assign(window, {
      wrelaGame: {
        discover: () => ({ id: definition.id, inputs: definition.inputs, schema: definition.inputSchema }),
        input: (value: unknown) => active.input(value),
        inspect: () => active.inspect(),
        measure: () => ({
          completeness: graphics.completeness,
          diagnostics: graphics.diagnostics,
          measurements: graphics.measurements,
          delivery: {
            launchMs,
            cookedDocuments: [...cookedDocuments],
            sourceDocuments: [...sourceDocuments],
          },
        }),
        step: async (count = 1) => {
          paused = true;
          if (!Number.isInteger(count) || count < 1 || count > 3600) throw Error("Step count must be 1–3600");
          for (let i = 0; i < count; i++) await active.advance(1 / 60);
          draw();
          return active.inspect();
        },
        save: () => active.save(),
        load: (value: unknown) => active.load(value),
        render: draw,
        capture: async () => {
          draw();
          return graphics.capture();
        },
        dispose,
      },
    });
    const render = async (now: number) => {
      if (stopped) return;
      try {
        const delta = (now - last) / 1000;
        last = now;
        if (!paused) await active.advance(delta);
        if (stopped) return;
        const scene = active.module.scene?.();
        if (scene) graphics.render(scene);
        const state = active.inspect().state as { objective?: string };
        objective.textContent = state.objective ?? definition.description;
        frame = requestAnimationFrame((time) => {
          void render(time);
        });
      } catch (error) {
        objective.textContent = String(error);
        paused = true;
      }
    };
    frame = requestAnimationFrame((time) => {
      void render(time);
    });
    draw();
    await graphics.capture();
    launchMs = performance.now() - started;
    return { driver, renderer, render: draw, dispose };
  } catch (error) {
    dispose();
    throw error;
  }
}
