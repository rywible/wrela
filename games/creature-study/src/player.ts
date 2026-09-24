import { createCreatureFixture } from "@wrela/examples";
import {
  CREATURE_ENCOUNTER_STEP,
  CreatureEncounterGame,
  type CreatureEncounterInput,
} from "@wrela/game-creature/rules";
import type { Camera, Project, Quality } from "@wrela/model";
import { BrowserSceneHost, type SceneHostOptions } from "@wrela/runtime";
import { creaturePlayerMarker } from "./marker";

const emptyInput = (): CreatureEncounterInput => ({ move: [0, 0], dodge: false, strike: false });
export function supportsCreatureStudy(project: Project, id: string): boolean {
  return (
    (id === "ash-warden" || id === "reed-penitent") &&
    project.documents.some((document) => document.id === id && document.kind === "character")
  );
}

/** A named study stage may offer its one supported creature directly on export. */
export function selectedCreatureStudy(project: Project, subject: string): string | undefined {
  if (supportsCreatureStudy(project, subject)) return subject;
  const document = project.documents.find((entry) => entry.id === subject);
  if (document?.kind !== "stage") return;
  const studies = document.subjects.filter((id) => supportsCreatureStudy(project, id));
  return studies.length === 1 ? studies[0] : undefined;
}

/** Isolated, fixed-step encounter using the exported project's actual creature and motions.
 * Fixture metadata supplies only the named study's game rules; it never replaces source. */
export class CreatureStudyRun {
  readonly camera: Camera = { position: [0, 10, 15], target: [0, 0.8, 0], fov: 58 };
  paused = false;
  private disposed = false;
  private accumulator = 0;
  private controls = emptyInput();
  private constructor(
    readonly id: string,
    readonly host: BrowserSceneHost,
    readonly game: CreatureEncounterGame,
  ) {}
  static async create(project: Project, id: string, options: SceneHostOptions, quality: Quality = "review") {
    if (!supportsCreatureStudy(project, id))
      throw Error("Select Ash Warden or Reed Penitent to play its authored study");
    const source = structuredClone(project);
    const character = source.documents.find((document) => document.id === id);
    if (character?.kind !== "character") throw Error("Creature source is unavailable");
    const fixture = createCreatureFixture(id as "ash-warden" | "reed-penitent");
    const host = new BrowserSceneHost(source, options);
    try {
      await host.prepare(
        id,
        source.documents.some((document) => document.id === fixture.stageId)
          ? fixture.stageId
          : "neutral-stage",
        quality,
      );
      if (!host.runtime) throw Error("Creature runtime is unavailable");
      const game = new CreatureEncounterGame({
        creatureId: id,
        encounter: fixture.encounter,
        motions: character.motions,
        runtime: host.runtime,
      });
      return new CreatureStudyRun(id, host, game);
    } catch (error) {
      host.dispose();
      throw error;
    }
  }
  input(value: Partial<CreatureEncounterInput>) {
    if (this.disposed) throw Error("Encounter is closed");
    if (
      value.move !== undefined &&
      (!Array.isArray(value.move) ||
        value.move.length !== 2 ||
        !value.move.every((number) => Number.isFinite(number) && Math.abs(number) <= 1))
    )
      throw Error("Movement must contain two finite numbers in [-1,1]");
    for (const key of ["dodge", "strike"] as const)
      if (value[key] !== undefined && typeof value[key] !== "boolean") throw Error(`${key} must be boolean`);
    if (Object.keys(value).some((key) => !["move", "dodge", "strike"].includes(key)))
      throw Error("Unknown encounter input");
    this.controls = { ...this.controls, ...value, move: [...(value.move ?? this.controls.move)] };
    return this.game.snapshot();
  }
  update(seconds: number) {
    if (this.disposed) throw Error("Encounter is closed");
    if (!Number.isFinite(seconds) || seconds < 0)
      throw Error("Frame duration must be finite and nonnegative");
    if (this.paused || this.game.snapshot().outcome !== "playing") return;
    this.accumulator = Math.min(0.1, this.accumulator + seconds);
    while (this.accumulator + 1e-10 >= CREATURE_ENCOUNTER_STEP) {
      this.game.step(CREATURE_ENCOUNTER_STEP, this.controls);
      this.host.advance(CREATURE_ENCOUNTER_STEP, this.camera);
      this.accumulator = Math.max(0, this.accumulator - CREATURE_ENCOUNTER_STEP);
    }
  }
  pause() {
    this.paused = true;
    this.controls = emptyInput();
    this.accumulator = 0;
  }
  resume() {
    if (this.disposed) throw Error("Encounter is closed");
    this.paused = false;
  }
  scene() {
    if (this.disposed) throw Error("Encounter is closed");
    const scene = this.host.extract(this.camera);
    const snapshot = this.game.snapshot();
    return {
      ...scene,
      surfaces: [...scene.surfaces, creaturePlayerMarker(snapshot.playerPosition, snapshot.invulnerable)],
    };
  }
  inspect() {
    return { active: !this.disposed, paused: this.paused, subject: this.id, snapshot: this.game.snapshot() };
  }
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.game.dispose();
    this.host.dispose();
  }
}

/** Browser controls are separate from simulation so exported/offline delivery is CPU-testable. */
export class CreatureStudyPlayer {
  private run?: CreatureStudyRun;
  private generation = 0;
  private closed = false;
  private keys = new Set<string>();
  private touchKeys = new Set<string>();
  private abort = new AbortController();
  private panel = document.createElement("section");
  private status = document.createElement("p");
  private playerHealth = document.createElement("progress");
  private creatureHealth = document.createElement("progress");
  private pauseButton = document.createElement("button");
  busy = false;
  get camera() {
    return this.run?.camera;
  }
  get active() {
    return !!this.run;
  }
  constructor(
    private project: Project,
    private options: SceneHostOptions,
    private quality: Quality,
    private onMode: (active: boolean) => void,
  ) {
    this.panel.className = "creature-game";
    this.panel.hidden = true;
    this.panel.setAttribute("aria-label", "Creature study encounter");
    const title = document.createElement("strong");
    title.textContent = "Creature encounter study";
    const meters = document.createElement("div");
    meters.className = "creature-health";
    for (const [name, meter, maximum] of [
      ["You", this.playerHealth, 100],
      ["Creature", this.creatureHealth, 96],
    ] as const) {
      const label = document.createElement("label");
      label.textContent = name;
      meter.max = maximum;
      meter.setAttribute("aria-label", `${name} health`);
      label.append(meter);
      meters.append(label);
    }
    this.status.setAttribute("role", "status");
    const instructions = document.createElement("p");
    instructions.className = "creature-instructions";
    instructions.textContent =
      "You are the blue marker. WASD / arrows move · Space dodge · J strike · Esc pause. Watch the windup, dodge the lunge, strike during recovery.";
    const commands = document.createElement("div");
    commands.className = "creature-commands";
    this.pauseButton.textContent = "Pause";
    this.pauseButton.addEventListener("click", () => (this.run?.paused ? this.resume() : this.pause()));
    const restart = document.createElement("button");
    restart.textContent = "Restart";
    restart.addEventListener("click", () => {
      void this.restart().catch((error) => {
        this.status.textContent = String(error);
      });
    });
    const leave = document.createElement("button");
    leave.textContent = "Return to viewer";
    leave.addEventListener("click", () => this.leave());
    commands.append(this.pauseButton, restart, leave);
    const touch = document.createElement("div");
    touch.className = "creature-touch";
    touch.setAttribute("aria-label", "Touch movement and actions");
    for (const [label, key] of [
      ["↑", "KeyW"],
      ["←", "KeyA"],
      ["↓", "KeyS"],
      ["→", "KeyD"],
      ["Dodge", "Space"],
      ["Strike", "KeyJ"],
    ]) {
      const button = document.createElement("button");
      button.textContent = label;
      button.setAttribute(
        "aria-label",
        (
          { KeyW: "Move forward", KeyA: "Move left", KeyS: "Move backward", KeyD: "Move right" } as Record<
            string,
            string
          >
        )[key] ?? label,
      );
      button.addEventListener("pointerdown", (event) => {
        event.preventDefault();
        button.setPointerCapture(event.pointerId);
        this.touchKeys.add(key);
        this.syncInput();
      });
      const release = () => {
        this.touchKeys.delete(key);
        this.syncInput();
      };
      button.addEventListener("pointerup", release);
      button.addEventListener("pointercancel", release);
      button.addEventListener("lostpointercapture", release);
      touch.append(button);
    }
    this.panel.append(title, meters, this.status, instructions, commands, touch);
    document.body.append(this.panel);
    const signal = this.abort.signal;
    window.addEventListener("keydown", (event) => this.key(event, true), { signal });
    window.addEventListener("keyup", (event) => this.key(event, false), { signal });
    window.addEventListener(
      "blur",
      () => {
        if (this.active) this.pause();
      },
      { signal },
    );
    document.addEventListener(
      "visibilitychange",
      () => {
        if (document.hidden && this.active) this.pause();
      },
      { signal },
    );
  }
  async start(id: string) {
    if (this.closed) throw Error("Player is closed");
    if (this.busy) throw Error("Wait for the encounter to finish preparing");
    const generation = ++this.generation;
    this.busy = true;
    try {
      const run = await CreatureStudyRun.create(this.project, id, this.options, this.quality);
      if (this.closed || generation !== this.generation) {
        run.dispose();
        throw Error("Encounter preparation cancelled");
      }
      this.run?.dispose();
      this.run = run;
      this.clearKeys();
      this.panel.hidden = false;
      this.onMode(true);
      this.paint();
      return this.inspect();
    } finally {
      if (generation === this.generation) this.busy = false;
    }
  }
  private key(event: KeyboardEvent, down: boolean) {
    if (!this.active) return;
    if (event.code === "Escape") {
      if (down && !event.repeat) this.run?.paused ? this.resume() : this.pause();
      event.preventDefault();
      return;
    }
    if (
      ![
        "KeyW",
        "KeyA",
        "KeyS",
        "KeyD",
        "ArrowUp",
        "ArrowLeft",
        "ArrowDown",
        "ArrowRight",
        "Space",
        "KeyJ",
      ].includes(event.code)
    )
      return;
    if (
      event.target instanceof HTMLElement &&
      event.target.closest("button,input,select,textarea") &&
      (event.code === "Space" || event.code === "KeyJ")
    )
      return;
    event.preventDefault();
    if (down) this.keys.add(event.code);
    else this.keys.delete(event.code);
    this.syncInput();
  }
  private syncInput() {
    if (!this.run || this.run.paused) return;
    const held = (...codes: string[]) =>
      codes.some((code) => this.keys.has(code) || this.touchKeys.has(code));
    this.run.input({
      move: [
        Number(held("KeyD", "ArrowRight")) - Number(held("KeyA", "ArrowLeft")),
        Number(held("KeyS", "ArrowDown")) - Number(held("KeyW", "ArrowUp")),
      ],
      dodge: held("Space"),
      strike: held("KeyJ"),
    });
  }
  private clearKeys() {
    this.keys.clear();
    this.touchKeys.clear();
  }
  input(value: Partial<CreatureEncounterInput>) {
    if (!this.run) throw Error("Start an encounter first");
    return this.run.input(value);
  }
  update(seconds: number) {
    this.run?.update(seconds);
    this.paint();
  }
  scene() {
    if (!this.run) throw Error("Start an encounter first");
    return this.run.scene();
  }
  inspect() {
    return this.run?.inspect() ?? { active: false, paused: false, subject: undefined, snapshot: undefined };
  }
  pause() {
    this.clearKeys();
    this.run?.pause();
    this.paint();
  }
  resume() {
    this.clearKeys();
    this.run?.resume();
    this.paint();
  }
  async restart() {
    if (!this.run) throw Error("Start an encounter first");
    return this.start(this.run.id);
  }
  private paint() {
    if (!this.run) return;
    const state = this.run.inspect();
    const snapshot = state.snapshot;
    this.playerHealth.value = snapshot.playerHealth;
    this.creatureHealth.value = snapshot.creatureHealth;
    this.pauseButton.textContent = state.paused ? "Resume" : "Pause";
    this.panel.dataset.phase = snapshot.phase;
    const text = state.paused
      ? "Paused"
      : snapshot.outcome === "won"
        ? "Study complete — creature defeated"
        : snapshot.outcome === "lost"
          ? "Defeated — restart and watch the windup"
          : snapshot.telegraph
            ? "Windup — prepare to dodge"
            : snapshot.phase === "attack"
              ? "Lunge!"
              : snapshot.phase === "recovery"
                ? "Recovery — an opening to strike"
                : "Keep the creature in view";
    const label = `${text} · Dodge ${snapshot.dodgeReady ? "ready" : "recovering"} · Strike ${snapshot.strikeReady ? "ready" : "recovering"}`;
    if (this.status.textContent !== label) this.status.textContent = label;
  }
  leave() {
    this.generation++;
    this.busy = false;
    const active = this.active;
    this.run?.dispose();
    this.run = undefined;
    this.clearKeys();
    this.panel.hidden = true;
    if (active) this.onMode(false);
  }
  dispose() {
    this.closed = true;
    this.leave();
    this.abort.abort();
    this.panel.remove();
  }
}
