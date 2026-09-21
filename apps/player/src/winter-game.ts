import {
  type Camera,
  contentKey,
  type MeshData,
  type Project,
  type RenderMaterial,
  type RenderSurface,
  transformMatrix,
  type Vec3,
} from "@wrela/model";
import type { RenderQuality } from "@wrela/render-webgpu";
import type { BrowserSceneHost } from "@wrela/runtime";
import {
  WINTER_HOME,
  WINTER_MARKERS,
  WINTER_STEP,
  type WinterEvent,
  type WinterInput,
  WinterValleyGame,
} from "../../../packages/runtime/src/game/winter-valley";

const ACTOR = "bunny-instance";
const material = (color: Vec3, metallic = 0): RenderMaterial => ({
  color,
  secondary: color,
  roughness: 0.7,
  metallic,
  pattern: 0,
  scale: 1,
  normalStrength: 0,
  domain: "local",
});
const stone = material([0.25, 0.38, 0.4]),
  amber = material([1, 0.6, 0.15], 0.1),
  ice = material([0.46, 0.7, 0.76]);
function cylinder(radius: number, height: number, sides = 8): MeshData {
  const positions: number[] = [],
    normals: number[] = [],
    indices: number[] = [];
  for (let side = 0; side < sides; side++) {
    const a = (side / sides) * Math.PI * 2,
      b = ((side + 1) / sides) * Math.PI * 2;
    const x = Math.cos((a + b) / 2),
      z = Math.sin((a + b) / 2),
      offset = positions.length / 3;
    positions.push(
      Math.cos(a) * radius,
      0,
      Math.sin(a) * radius,
      Math.cos(b) * radius,
      0,
      Math.sin(b) * radius,
      Math.cos(b) * radius,
      height,
      Math.sin(b) * radius,
      Math.cos(a) * radius,
      height,
      Math.sin(a) * radius,
    );
    for (let j = 0; j < 4; j++) normals.push(x, 0, z);
    indices.push(offset, offset + 2, offset + 1, offset, offset + 3, offset + 2);
    const cap = positions.length / 3;
    positions.push(
      0,
      height,
      0,
      Math.cos(a) * radius,
      height,
      Math.sin(a) * radius,
      Math.cos(b) * radius,
      height,
      Math.sin(b) * radius,
    );
    normals.push(0, 1, 0, 0, 1, 0, 0, 1, 0);
    indices.push(cap, cap + 2, cap + 1);
  }
  return {
    positions: new Float32Array(positions),
    normals: new Float32Array(normals),
    indices: new Uint32Array(indices),
    bounds: { min: [-radius, 0, -radius], max: [radius, height, radius] },
  };
}
const baseMesh = cylinder(0.66, 0.45),
  postMesh = cylinder(0.22, 1.65),
  lightMesh = cylinder(0.4, 0.52, 6),
  roofMesh = cylinder(0.54, 0.13, 6);

class WinterAudio {
  private context?: AudioContext;
  muted = false;
  async unlock() {
    this.context ??= new AudioContext();
    if (this.context.state === "suspended") await this.context.resume();
  }
  note(frequency: number, duration = 0.22, delay = 0, gain = 0.065) {
    if (!this.context || this.muted) return;
    const t = this.context.currentTime + delay,
      oscillator = this.context.createOscillator(),
      envelope = this.context.createGain();
    oscillator.type = "sine";
    oscillator.frequency.setValueAtTime(frequency, t);
    envelope.gain.setValueAtTime(0, t);
    envelope.gain.linearRampToValueAtTime(gain, t + 0.015);
    envelope.gain.exponentialRampToValueAtTime(0.0001, t + duration);
    oscillator.connect(envelope).connect(this.context.destination);
    oscillator.start(t);
    oscillator.stop(t + duration + 0.01);
    oscillator.onended = () => {
      oscillator.disconnect();
      envelope.disconnect();
    };
  }
  event(event: WinterEvent) {
    if (event.kind === "restored")
      [392, 523.25, 659.25].forEach((frequency, n) => {
        this.note(frequency, 0.55, n * 0.12);
      });
    if (event.kind === "home")
      [392, 493.88, 587.33, 783.99].forEach((frequency, n) => {
        this.note(frequency, 1, n * 0.18);
      });
    if (event.kind === "lost")
      [220, 196, 146.83].forEach((frequency, n) => {
        this.note(frequency, 0.8, n * 0.25);
      });
    if (event.kind === "warning") this.note(196, 0.5);
  }
  dispose() {
    void this.context?.close().catch(() => {});
  }
}

type SaveEnvelope = {
  version: 1;
  project: string;
  source: string;
  game: ReturnType<WinterValleyGame["save"]>;
  runtime: ReturnType<BrowserSceneHost["saveRuntime"]>;
};
export class WinterPlayer {
  readonly rules = new WinterValleyGame();
  active = false;
  camera: Camera = { position: [0, 9, 15], target: [0, 1, 0], fov: 48 };
  private accumulator = 0;
  private yaw = 0;
  private moving = false;
  private keys = new Set<string>();
  private touch: [number, number] = [0, 0];
  private action = false;
  private apiInput: Partial<WinterInput> | null = null;
  private root = document.createElement("div");
  private audio = new WinterAudio();
  private route: HTMLElement;
  private objective: HTMLElement;
  private warmth: HTMLMeterElement;
  private prompt: HTMLElement;
  private panel: HTMLElement;
  private notice: HTMLElement;
  private map: SVGSVGElement;
  private lastNotice = "";
  private noticeUntilTick = 0;
  private recentEvents: WinterEvent[] = [];
  private transitioning = false;
  private lastFootstep = -1;
  private blocked = false;
  private get runtime() {
    if (!this.host.runtime) throw new Error("The trail runtime is unavailable");
    return this.host.runtime;
  }
  private get world() {
    if (!this.host.world) throw new Error("The trail world is unavailable");
    return this.host.world;
  }
  get busy() {
    return this.transitioning;
  }
  private keyDown = (event: KeyboardEvent) => this.keyboard(event, true);
  private keyUp = (event: KeyboardEvent) => this.keyboard(event, false);
  private focusLost = () => {
    if (this.active && !this.rules.paused) this.pause();
  };
  private visibility = () => {
    if (document.hidden) this.focusLost();
  };
  constructor(
    private host: BrowserSceneHost,
    private project: Project,
    private onMode: (active: boolean) => void,
    private graphics: { profile: () => RenderQuality; change: (profile: RenderQuality) => Promise<unknown> },
  ) {
    this.root.className = "winter-game";
    this.root.hidden = true;
    this.root.innerHTML = `<div class="winter-top"><div><h1>Winter valley</h1><p class="winter-objective"></p></div><div class="winter-weather"><label for="winter-warmth">Warmth</label><meter id="winter-warmth" min="0" max="150" low="30" optimum="150" value="150"></meter><button data-action="pause" aria-label="Pause exploration">Pause</button></div></div><nav class="winter-route" aria-label="Trail markers"></nav><svg class="winter-map" viewBox="-45 -45 90 100" role="img" aria-label="Trail map. North is up."></svg><div class="winter-notice" role="status" aria-live="polite"></div><div class="winter-prompt"></div><div class="winter-help">WASD / arrows to move <span>Shift to hurry</span> <span>Hold E to restore a marker</span></div><div class="winter-touch"><div class="winter-stick" role="group" aria-label="Movement pad"><span></span></div><button class="winter-action" aria-label="Hold to restore trail marker">Restore</button></div><section class="winter-panel" role="dialog" aria-modal="true" aria-labelledby="winter-panel-title" hidden></section>`;
    document.body.append(this.root);
    const get = <T extends Element>(selector: string): T => {
      const element = this.root.querySelector<T>(selector);
      if (!element) throw new Error(`Missing trail control ${selector}`);
      return element;
    };
    this.route = get(".winter-route");
    this.objective = get(".winter-objective");
    this.warmth = get("meter");
    this.prompt = get(".winter-prompt");
    this.panel = get(".winter-panel");
    this.notice = get(".winter-notice");
    this.map = get("svg");
    this.root.addEventListener("click", (event) => {
      const action = (event.target as Element).closest<HTMLElement>("[data-action]")?.dataset.action;
      if (!action) return;
      void this.perform(action).catch((error) => this.announce(String(error)));
    });
    const stick = get<HTMLElement>(".winter-stick"),
      knob = get<HTMLElement>(".winter-stick span");
    let pointer: number | null = null;
    const update = (event: PointerEvent) => {
      const r = stick.getBoundingClientRect();
      let x = (event.clientX - r.left - r.width / 2) / 38,
        y = (event.clientY - r.top - r.height / 2) / 38;
      const length = Math.max(1, Math.hypot(x, y));
      x /= length;
      y /= length;
      this.touch = [x, y];
      knob.style.transform = `translate(${x * 27}px,${y * 27}px)`;
    };
    stick.addEventListener("pointerdown", (event) => {
      pointer = event.pointerId;
      stick.setPointerCapture(pointer);
      this.apiInput = null;
      update(event);
    });
    stick.addEventListener("pointermove", (event) => {
      if (event.pointerId === pointer) update(event);
    });
    const release = () => {
      pointer = null;
      this.touch = [0, 0];
      knob.style.transform = "";
    };
    stick.addEventListener("pointerup", release);
    stick.addEventListener("pointercancel", release);
    stick.addEventListener("lostpointercapture", release);
    const actionButton = get<HTMLButtonElement>(".winter-action");
    actionButton.addEventListener("pointerdown", (event) => {
      this.apiInput = null;
      this.action = true;
      actionButton.setPointerCapture(event.pointerId);
    });
    for (const event of ["pointerup", "pointercancel", "lostpointercapture"])
      actionButton.addEventListener(event, () => {
        this.action = false;
      });
    window.addEventListener("keydown", this.keyDown);
    window.addEventListener("keyup", this.keyUp);
    window.addEventListener("blur", this.focusLost);
    document.addEventListener("visibilitychange", this.visibility);
  }
  private async perform(action: string) {
    if (action === "pause") this.pause();
    if (action === "begin" || action === "continue") {
      await this.audio.unlock().catch(() => {});
      this.resume();
    }
    if (action === "restart") await this.restart();
    if (action === "save") {
      this.saveLocal();
      this.announce("Trail saved on this device.");
    }
    if (action === "restore") await this.restoreLocal();
    if (action === "export") this.download();
    if (action === "import") this.importFile();
    if (action === "leave") this.leave();
    if (action === "sound") {
      this.audio.muted = !this.audio.muted;
      this.showPanel("Rest a moment", "Your trail is waiting. Time stands still while paused.");
    }
  }
  private keyboard(event: KeyboardEvent, down: boolean) {
    if (!this.active) return;
    if (event.code === "Tab" && !this.panel.hidden && down) {
      const buttons = [
        ...this.panel.querySelectorAll<HTMLButtonElement | HTMLSelectElement>("button, select"),
      ];
      const first = buttons[0],
        last = buttons.at(-1);
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
      return;
    }
    if (event.code === "Escape" && down && !event.repeat) {
      event.preventDefault();
      this.rules.paused ? this.resume() : this.pause();
      return;
    }
    if (
      ![
        "KeyW",
        "KeyA",
        "KeyS",
        "KeyD",
        "ArrowUp",
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "KeyE",
        "Space",
        "ShiftLeft",
        "ShiftRight",
      ].includes(event.code)
    )
      return;
    if (event.target instanceof HTMLButtonElement && (event.code === "Space" || event.code === "KeyE"))
      return;
    event.preventDefault();
    this.apiInput = null;
    if (down) this.keys.add(event.code);
    else this.keys.delete(event.code);
  }
  supported() {
    const character = this.project.documents.find((document) => document.id === "polar-bunny");
    if (
      character?.kind !== "character" ||
      !["idle", "hop"].every((id) => character.motions.some((motion) => motion.id === id))
    )
      return false;
    return this.project.documents.some(
      (document) =>
        document.kind === "world" &&
        document.id === "winter-valley" &&
        document.instances.some((instance) => instance.id === ACTOR && instance.definition === "polar-bunny"),
    );
  }
  async start(showIntro = false) {
    if (this.transitioning) throw new Error("The trail is still preparing");
    if (!this.supported())
      throw new Error("This project does not contain the Winter valley trail and polar bunny");
    this.transitioning = true;
    try {
      await this.host.prepare("winter-valley", "neutral-stage", "review");
      await this.host.resetRuntime();
      this.runtime.setFocusCharacter(ACTOR);
      this.runtime.setRootMotionPolicy(ACTOR, "visual");
      this.world.setInterest({
        id: "winter-trail",
        position: [-16, 0, 8],
        visualRadius: 90,
        collisionRadius: 64,
      });
      const readiness = await this.world.prepare();
      if (!readiness.ready) throw new Error("The trail is not ready. Try opening it again.");
      await this.host.moveCharacter(ACTOR, WINTER_HOME);
      this.runtime.physics.drainContactEvents();
      this.rules.restart();
      this.accumulator = 0;
      this.yaw = 0;
      this.moving = false;
      this.recentEvents = [];
      this.apiInput = null;
      this.active = true;
      this.root.hidden = false;
      this.onMode(true);
      this.camera = { position: [0, 10, 16], target: [0, 1, 0], fov: 48 };
      this.runtime.playMotion(ACTOR, "idle", 0);
      this.rules.pause(showIntro);
      this.panel.hidden = !showIntro;
      if (showIntro)
        this.showPanel(
          "A light through the snow",
          "Three trail lanterns have gone dark. Guide the bunny through the valley, restore each lantern, then find your way home before your warmth runs out.",
          true,
        );
      this.renderHud();
    } finally {
      this.transitioning = false;
    }
  }
  async restart() {
    await this.start(false);
    await this.audio.unlock().catch(() => {});
  }
  pause() {
    this.rules.pause();
    this.clearInput();
    this.showPanel("Rest a moment", "Your trail is waiting. Time stands still while paused.");
  }
  resume() {
    if (!this.active || this.rules.inspect().phase === "won" || this.rules.inspect().phase === "lost") return;
    this.rules.pause(false);
    this.panel.hidden = true;
    this.accumulator = 0;
    this.clearInput();
  }
  leave() {
    this.rules.pause();
    this.active = false;
    this.root.hidden = true;
    this.clearInput();
    this.host.world?.removeInterest("winter-trail");
    this.onMode(false);
  }
  input(value: Partial<WinterInput>) {
    this.rules.input(value);
    this.apiInput = { ...this.apiInput, ...value };
  }
  inspect() {
    return {
      ...this.rules.inspect(),
      active: this.active,
      preparing: this.transitioning,
      blocked: this.blocked,
      camera: structuredClone(this.camera),
      markers: structuredClone(WINTER_MARKERS),
    };
  }
  events() {
    return structuredClone(this.recentEvents);
  }
  private clearInput() {
    this.keys.clear();
    this.touch = [0, 0];
    this.action = false;
    this.apiInput = null;
    this.rules.input({ move: [0, 0], interact: false, hurry: false });
  }
  update(delta: number, controlled = false) {
    if (!this.active || (this.transitioning && !controlled)) return;
    const runtime = this.runtime;
    if (!this.rules.paused) {
      this.accumulator = Math.min(this.accumulator + delta, 5 * WINTER_STEP);
      while (this.accumulator >= WINTER_STEP) {
        const state = this.rules.inspect();
        if (state.phase === "won" || state.phase === "lost") {
          this.accumulator = 0;
          break;
        }
        const x =
          Number(this.keys.has("KeyD") || this.keys.has("ArrowRight")) -
          Number(this.keys.has("KeyA") || this.keys.has("ArrowLeft")) +
          this.touch[0];
        const z =
          Number(this.keys.has("KeyS") || this.keys.has("ArrowDown")) -
          Number(this.keys.has("KeyW") || this.keys.has("ArrowUp")) +
          this.touch[1];
        this.rules.input(
          this.apiInput ?? {
            move: [x, z],
            interact: this.action || this.keys.has("KeyE") || this.keys.has("Space"),
            hurry: this.keys.has("ShiftLeft") || this.keys.has("ShiftRight"),
          },
        );
        const movement = this.rules.movement(),
          foot = this.host.characterPosition(ACTOR),
          body = runtime.bodyState(ACTOR);
        const targetX = foot[0] + movement[0],
          targetZ = foot[2] + movement[1],
          ground = this.world.queryGround(targetX, targetZ);
        this.blocked = ground.status !== "ready";
        if (this.blocked) break;
        if (ground.status === "ready") {
          const walking = Math.hypot(...movement) > 0.001;
          if (walking !== this.moving) {
            runtime.playMotion(ACTOR, walking ? "hop" : "idle", 0.18);
            this.moving = walking;
          }
          if (walking) {
            this.yaw = Math.atan2(movement[0], movement[1]);
            runtime.setFacing(ACTOR, this.yaw);
          }
          runtime.setTarget(ACTOR, [targetX, ground.height + (body.position[1] - foot[1]) + 0.025, targetZ]);
        }
        const steps = this.host.advance(WINTER_STEP, this.camera);
        if (!steps) {
          this.blocked = true;
          break;
        }
        this.rules.step(this.host.characterPosition(ACTOR));
        this.accumulator -= WINTER_STEP;
        const tick = this.rules.inspect().tick;
        if (this.moving && Math.floor(tick / 22) !== this.lastFootstep) {
          this.lastFootstep = Math.floor(tick / 22);
          this.audio.note(95 + (tick % 3) * 11, 0.055, 0, 0.015);
        }
        for (const event of this.rules.drainEvents()) {
          this.recentEvents.push(event);
          if (this.recentEvents.length > 32) this.recentEvents.shift();
          this.audio.event(event);
          if (event.kind === "restored")
            this.announce(
              `${WINTER_MARKERS.find((marker) => marker.id === event.marker)?.name} is shining. Warmth restored.`,
            );
          if (event.kind === "warning")
            this.announce("The cold is closing in. A lantern will restore your warmth.");
          if (event.kind === "home")
            this.showPanel(
              "Home, with a brighter valley",
              `All three lanterns are shining. You found your way home in ${Math.round(this.rules.inspect().seconds)} seconds.`,
            );
          if (event.kind === "lost")
            this.showPanel(
              "The snow grew too cold",
              "Your trail ends here, but the valley is waiting. Try again; each restored lantern gives you more warmth.",
            );
        }
      }
    }
    const position = this.host.characterPosition(ACTOR),
      blend = 1 - Math.exp(-delta * 7);
    const desiredTarget: Vec3 = [position[0], position[1] + 1, position[2]];
    const desiredEye: Vec3 = [position[0], position[1] + 9.5, position[2] + 15];
    this.camera = {
      ...this.camera,
      position: this.camera.position.map((v, i) => v + (desiredEye[i] - v) * blend) as Vec3,
      target: this.camera.target.map((v, i) => v + (desiredTarget[i] - v) * blend) as Vec3,
    };
    const hit = runtime.physics.raycast(
      desiredTarget,
      this.camera.position.map((v, i) => v - desiredTarget[i]) as Vec3,
      Math.hypot(...this.camera.position.map((v, i) => v - desiredTarget[i])),
      ACTOR,
    );
    if (hit && hit.distance > 0.8) {
      const d = this.camera.position.map((v, i) => v - desiredTarget[i]),
        length = Math.hypot(...d);
      this.camera.position = desiredTarget.map(
        (v, i) => v + (d[i] / length) * Math.max(1, hit.distance - 0.5),
      ) as Vec3;
    }
    this.host.updateView(this.camera);
    this.renderHud();
  }
  /** Bounded deterministic scenario runner; uses the same input, collision and fixed update path as RAF play. */
  async simulate(options: { ticks: number; input?: Partial<WinterInput> }) {
    if (!this.active || this.transitioning) throw new Error("Open an idle trail before running a scenario");
    if (!Number.isInteger(options.ticks) || options.ticks < 1 || options.ticks > 600)
      throw new Error("A scenario batch must contain 1 to 600 ticks");
    if (options.input) this.input(options.input);
    this.transitioning = true;
    this.accumulator = 0;
    let advanced = 0;
    try {
      for (let index = 0; index < options.ticks; index++) {
        if (this.world.metrics.pending) {
          const readiness = await this.world.prepare({ scope: "collision" });
          if (!readiness.ready) throw new Error("Scenario collision region is unavailable");
        }
        const before = this.rules.inspect().tick;
        this.update(WINTER_STEP, true);
        const steps = this.rules.inspect().tick - before;
        advanced += steps;
        if (!steps && !this.rules.paused && ["exploring", "returning"].includes(this.rules.inspect().phase))
          throw new Error("Scenario could not advance its physical actor");
        if (index % 60 === 0) await new Promise((resolve) => setTimeout(resolve, 0));
      }
      return { advanced, state: this.inspect(), clock: this.runtime.clock.tick };
    } finally {
      this.transitioning = false;
      this.accumulator = 0;
    }
  }
  scene() {
    const scene = this.host.extract(this.camera),
      state = this.rules.inspect();
    const origin = this.camera.position.map((v, i) => v - scene.camera.position[i]);
    const sources = [
      { id: "home", position: WINTER_HOME, lit: true },
      ...WINTER_MARKERS.map((marker) => ({ ...marker, lit: state.restored.includes(marker.id) })),
    ];
    const surfaces: RenderSurface[] = [];
    for (const marker of sources) {
      const ground = this.world.queryGround(marker.position[0], marker.position[2]);
      if (ground.status !== "ready") continue;
      const position: Vec3 = [
        marker.position[0] - origin[0],
        ground.height - origin[1],
        marker.position[2] - origin[2],
      ];
      for (const [name, mesh, y, appearance] of [
        ["base", baseMesh, 0, stone],
        ["post", postMesh, 0.35, stone],
        ["lantern", lightMesh, 1.7, marker.lit ? amber : ice],
        ["roof", roofMesh, 2.25, stone],
      ] as const) {
        surfaces.push({
          id: `trail/${marker.id}/${name}`,
          source: "winter-trail",
          instanceId: `trail/${marker.id}`,
          mesh,
          matrix: transformMatrix([position[0], position[1] + y, position[2]]),
          material: appearance,
        });
      }
      if (marker.lit)
        scene.environment.pointLights = [
          ...(scene.environment.pointLights ?? []),
          {
            position: [position[0], position[1] + 2.1, position[2]] as Vec3,
            color: [1, 0.63, 0.25] as Vec3,
            intensity: 5,
          },
        ].slice(0, 8);
    }
    scene.surfaces.push(...surfaces);
    return scene;
  }
  save(): SaveEnvelope {
    if (!this.active || this.transitioning) throw new Error("Open the trail before saving");
    return {
      version: 1,
      project: this.project.id,
      source: contentKey(this.project),
      game: this.rules.save(),
      runtime: this.host.saveRuntime(),
    };
  }
  async restore(value: unknown) {
    const data = value as Partial<SaveEnvelope>;
    if (
      !data ||
      data.version !== 1 ||
      data.project !== this.project.id ||
      data.source !== contentKey(this.project) ||
      !data.runtime
    )
      throw new Error("This trail save belongs to a different project version");
    const validated = new WinterValleyGame();
    validated.restore(data.game);
    if (!this.active) await this.start(false);
    if (this.transitioning) throw new Error("Wait for the trail to finish preparing");
    this.transitioning = true;
    try {
      await this.host.loadRuntime(data.runtime);
      this.rules.restore(validated.save());
      this.recentEvents = [];
      this.clearInput();
      this.accumulator = 0;
      this.moving = false;
      this.runtime.setRootMotionPolicy(ACTOR, "visual");
      this.runtime.playMotion(ACTOR, "idle", 0);
      const position = this.host.characterPosition(ACTOR);
      this.camera = {
        position: [position[0], position[1] + 9.5, position[2] + 15],
        target: [position[0], position[1] + 1, position[2]],
        fov: 48,
      };
      this.showPanel(
        "Your trail is safe",
        "Lanterns, warmth and your place in the valley have been restored.",
      );
      this.renderHud();
    } finally {
      this.transitioning = false;
    }
  }
  private saveLocal() {
    localStorage.setItem(`wrela-winter-trail:${this.project.id}`, JSON.stringify(this.save()));
  }
  private async restoreLocal() {
    const data = localStorage.getItem(`wrela-winter-trail:${this.project.id}`);
    if (!data) throw new Error("No trail has been saved on this device yet.");
    await this.restore(JSON.parse(data));
  }
  private download() {
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(this.save(), null, 2)], { type: "application/json" }),
    );
    const link = document.createElement("a");
    link.href = url;
    link.download = `${this.project.id}-trail.json`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  private importFile() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json,application/json";
    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (!file) return;
      if (file.size > 8 * 1024 * 1024) {
        this.announce("Trail saves must be smaller than 8 MiB.");
        return;
      }
      void file
        .text()
        .then((text) => this.restore(JSON.parse(text)))
        .catch((error) => this.announce(String(error)));
    });
    input.click();
  }
  private announce(message: string) {
    this.lastNotice = message;
    this.noticeUntilTick = this.rules.inspect().tick + 240;
    this.notice.textContent = message;
  }
  private showPanel(title: string, description: string, intro = false) {
    const state = this.rules.inspect(),
      ended = state.phase === "won" || state.phase === "lost";
    this.panel.replaceChildren();
    const heading = document.createElement("h2");
    heading.id = "winter-panel-title";
    heading.textContent = title;
    const body = document.createElement("p");
    body.textContent = description;
    this.panel.append(heading, body);
    const actions = document.createElement("div");
    actions.className = "winter-panel-actions";
    const add = (label: string, action: string, primary = false) => {
      const button = document.createElement("button");
      button.textContent = label;
      button.dataset.action = action;
      if (primary) button.className = "winter-primary";
      actions.append(button);
    };
    if (!ended) add(intro ? "Begin the trail" : "Continue exploring", intro ? "begin" : "continue", true);
    if (ended) add("Walk the trail again", "restart", true);
    if (!intro && !ended) add("Save trail", "save");
    add("Resume saved trail", "restore");
    if (!intro) {
      add("Export trail save", "export");
      add("Import trail save", "import");
    }
    add(this.audio.muted ? "Turn sound on" : "Mute sound", "sound");
    add("Return to viewer", "leave");
    const graphicsLabel = document.createElement("label");
    graphicsLabel.className = "winter-graphics";
    graphicsLabel.textContent = "Graphics";
    const graphicsSelect = document.createElement("select");
    graphicsSelect.setAttribute("aria-label", "Graphics quality");
    for (const [id, label] of [
      ["low", "Performance"],
      ["balanced", "Balanced"],
      ["high", "High quality"],
    ]) {
      const option = document.createElement("option");
      option.value = id;
      option.textContent = label;
      graphicsSelect.append(option);
    }
    graphicsSelect.value = this.graphics.profile();
    graphicsSelect.addEventListener("change", () => {
      void this.graphics.change(graphicsSelect.value as RenderQuality).catch((error) => {
        graphicsSelect.value = this.graphics.profile();
        this.announce(String(error));
      });
    });
    graphicsLabel.append(graphicsSelect);
    this.panel.append(graphicsLabel, actions);
    this.panel.hidden = false;
    actions.querySelector("button")?.focus();
  }
  private renderHud() {
    const state = this.rules.inspect();
    this.objective.textContent =
      state.phase === "returning"
        ? "The trail is bright. Return to the home lantern."
        : state.phase === "won"
          ? "The valley is bright again."
          : `${state.restored.length} of 3 trail lanterns restored`;
    this.warmth.value = state.warmth;
    this.warmth.setAttribute("aria-valuetext", `${Math.ceil(state.warmth)} seconds of warmth`);
    this.route.replaceChildren(
      ...WINTER_MARKERS.map((marker) => {
        const span = document.createElement("span");
        span.className = state.restored.includes(marker.id) ? "restored" : "";
        span.textContent = `${state.restored.includes(marker.id) ? "◆" : "◇"} ${marker.name}`;
        return span;
      }),
    );
    const nearby = WINTER_MARKERS.find((marker) => marker.id === state.nearby);
    this.prompt.textContent = this.blocked
      ? "Waiting for a safe path…"
      : nearby
        ? `Hold E or Restore · ${nearby.name}${state.progress ? ` · ${Math.round(state.progress * 100)}%` : ""}`
        : state.phase === "returning"
          ? `${Math.ceil(Math.hypot(state.position[0], state.position[2]))} m to home`
          : "Follow the blue lanterns on your map";
    if (state.tick > this.noticeUntilTick) {
      this.lastNotice = "";
      this.notice.textContent = "";
    }
    if (this.lastNotice && this.notice.textContent !== this.lastNotice)
      this.notice.textContent = this.lastNotice;
    this.map.innerHTML = `<path d="M0 0 L-16 -16 L-34 12 L-10 34 L0 0" fill="none" stroke="#b9d4d4" stroke-opacity=".45" stroke-width=".8" stroke-dasharray="2 2"/><text x="37" y="-34" fill="#e7f2ed" font-size="6">N</text><path d="M39 -31v8m-2-6 2-2 2 2" fill="none" stroke="#e7f2ed" stroke-width=".7"/><path d="M-3 1L0-3 3 1v4h-6z" fill="#f4bd6b"/>${WINTER_MARKERS.map((marker) => `<circle cx="${marker.position[0]}" cy="${marker.position[2]}" r="2.6" fill="${state.restored.includes(marker.id) ? "#f4bd6b" : "#82bdc9"}"/>`).join("")}<circle cx="${state.position[0]}" cy="${state.position[2]}" r="2.2" fill="#fff" stroke="#193a44" stroke-width=".9"/>`;
  }
  dispose() {
    this.audio.dispose();
    this.root.remove();
    window.removeEventListener("keydown", this.keyDown);
    window.removeEventListener("keyup", this.keyUp);
    window.removeEventListener("blur", this.focusLost);
    document.removeEventListener("visibilitychange", this.visibility);
  }
}
