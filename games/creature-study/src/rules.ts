import type { CreatureFixture } from "@wrela/examples";
import type { Motion, Vec3 } from "@wrela/model";
import type { RuntimeSession } from "@wrela/runtime/session";

export const CREATURE_ENCOUNTER_STEP = 1 / 60;
export type CreatureEncounterInput = { move: [number, number]; dodge: boolean; strike: boolean };
export type CreatureEncounterPhase =
  | "watching"
  | "approach"
  | "telegraph"
  | "attack"
  | "recovery"
  | "stagger"
  | "won"
  | "lost";
export type CreatureEncounterEvent = {
  tick: number;
  kind:
    | "telegraph"
    | "commit"
    | "enemy-hit"
    | "player-hit"
    | "dodged"
    | "dodge"
    | "strike"
    | "recovery"
    | "won"
    | "lost";
  amount?: number;
};
export type CreatureEncounterSnapshot = {
  tick: number;
  phase: CreatureEncounterPhase;
  phaseTime: number;
  playerPosition: Vec3;
  creaturePosition: Vec3;
  playerHealth: number;
  creatureHealth: number;
  telegraph: boolean;
  invulnerable: boolean;
  outcome: "playing" | "won" | "lost";
  motion: string;
  creatureYaw: number;
  dodgeReady: boolean;
  strikeReady: boolean;
  events: CreatureEncounterEvent[];
};
type EncounterRuntime = Pick<
  RuntimeSession,
  "playMotion" | "setTarget" | "setFacing" | "setRootMotionPolicy" | "bodyState"
>;
export type CreatureEncounterOptions = {
  creatureId: string;
  encounter: CreatureFixture["encounter"];
  motions: Motion[];
  runtime?: EncounterRuntime;
};
const emptyInput = (): CreatureEncounterInput => ({ move: [0, 0], dodge: false, strike: false });
const planarDistance = (a: Vec3, b: Vec3) => Math.hypot(a[0] - b[0], a[2] - b[2]);

/** Small original encounter using the authored clips and damage windows. Game
 * state is deterministic at 60 Hz. A RuntimeSession adapter receives real motion,
 * facing and movement commands; the caller advances that runtime once after each tick.
 * The arena is deliberately flat and bounded, not a terrain-navigation system. */
export class CreatureEncounterGame {
  private state!: CreatureEncounterSnapshot;
  private accumulator = 0;
  private disposed = false;
  private previousInput = emptyInput();
  private dodgeTime = Infinity;
  private dodgeCooldown = 0;
  private dodgeDirection: [number, number] = [0, 1];
  private strikeTime = Infinity;
  private strikeCooldown = 0;
  private strikeApplied = false;
  private attackTime = 0;
  private attackApplied = false;
  private attackOrigin: Vec3 = [0, 0, 0];
  private readonly lunge: Motion;
  private readonly window: CreatureFixture["encounter"]["attackWindows"][number];
  private readonly options: CreatureEncounterOptions;
  private readonly bodyHeightOffset: number;
  constructor(options: CreatureEncounterOptions) {
    this.options = {
      ...options,
      encounter: structuredClone(options.encounter),
      motions: structuredClone(options.motions),
    };
    const window = this.options.encounter.attackWindows.find((entry) => entry.motion === "lunge");
    const lunge = this.options.motions.find((motion) => motion.id === window?.motion);
    if (
      !window ||
      !lunge ||
      !(window.start > 0 && window.end > window.start && window.end <= lunge.duration) ||
      !(window.reach > 0) ||
      !(options.encounter.arenaRadius >= 3) ||
      !options.encounter.playerStart.every(Number.isFinite)
    )
      throw new Error("Encounter requires a bounded arena and a valid authored lunge damage window.");
    for (const required of ["idle", "walk", "recovery", "hit"])
      if (!this.options.motions.some((motion) => motion.id === required))
        throw new Error(`Encounter is missing motion ${required}`);
    this.window = window;
    this.lunge = lunge;
    this.bodyHeightOffset = options.runtime?.bodyState(options.creatureId).position[1] ?? 0;
    this.reset();
  }
  private emit(kind: CreatureEncounterEvent["kind"], amount?: number) {
    this.state.events.push({ tick: this.state.tick, kind, ...(amount === undefined ? {} : { amount }) });
    if (this.state.events.length > 64) this.state.events.shift();
  }
  private motion(id: string, blend = 0.12) {
    if (this.state.motion === id) return;
    // Adopt completed physical root displacement before changing clips. Blending
    // the old absolute root track again would apply that displacement twice.
    if (this.state.motion === "lunge") {
      this.target(this.state.creaturePosition);
      blend = 0;
    }
    this.state.motion = id;
    this.options.runtime?.playMotion(this.options.creatureId, id, blend);
  }
  private target(position: Vec3) {
    this.options.runtime?.setTarget(this.options.creatureId, [
      position[0],
      position[1] + this.bodyHeightOffset,
      position[2],
    ]);
  }
  private transition(phase: CreatureEncounterPhase) {
    this.state.phase = phase;
    this.state.phaseTime = 0;
  }
  private bounded(position: Vec3): Vec3 {
    const radius = Math.hypot(position[0], position[2]);
    const limit = this.options.encounter.arenaRadius - 0.4;
    return radius > limit
      ? [(position[0] / radius) * limit, position[1], (position[2] / radius) * limit]
      : position;
  }
  private directionToPlayer(): [number, number] {
    const x = this.state.playerPosition[0] - this.state.creaturePosition[0],
      z = this.state.playerPosition[2] - this.state.creaturePosition[2];
    const length = Math.hypot(x, z);
    return length > 1e-8
      ? [x / length, z / length]
      : [Math.sin(this.state.creatureYaw), Math.cos(this.state.creatureYaw)];
  }
  private rootTranslation(time: number): Vec3 {
    const keys = this.lunge.keys.filter((key) => key.joint === "root").sort((a, b) => a.time - b.time);
    if (!keys.length) return [0, 0, 0];
    let a = keys[0],
      b = keys[keys.length - 1];
    if (time <= a.time) return [...a.translation];
    for (let index = 1; index < keys.length; index++)
      if (keys[index].time >= time) {
        a = keys[index - 1];
        b = keys[index];
        break;
      }
    const alpha = Math.max(0, Math.min(1, (time - a.time) / Math.max(1e-8, b.time - a.time)));
    return a.translation.map((value, axis) => value + (b.translation[axis] - value) * alpha) as Vec3;
  }
  private beginAttack() {
    this.transition("telegraph");
    this.attackTime = 0;
    this.attackApplied = false;
    this.attackOrigin = [...this.state.creaturePosition];
    const direction = this.directionToPlayer();
    this.state.creatureYaw = Math.atan2(direction[0], direction[1]);
    this.options.runtime?.setFacing(this.options.creatureId, this.state.creatureYaw);
    this.motion("lunge", 0.06);
    this.emit("telegraph");
  }
  private attackPosition(time: number): Vec3 {
    const translation = this.rootTranslation(time),
      yaw = this.state.creatureYaw;
    return [
      this.attackOrigin[0] + Math.cos(yaw) * translation[0] + Math.sin(yaw) * translation[2],
      this.attackOrigin[1] + translation[1],
      this.attackOrigin[2] - Math.sin(yaw) * translation[0] + Math.cos(yaw) * translation[2],
    ];
  }
  private tick(input: CreatureEncounterInput) {
    if (this.state.outcome !== "playing") return;
    const dt = CREATURE_ENCOUNTER_STEP;
    this.state.tick++;
    this.state.phaseTime += dt;
    if (this.options.runtime) {
      const position = this.options.runtime.bodyState(this.options.creatureId).position;
      this.state.creaturePosition = [position[0], position[1] - this.bodyHeightOffset, position[2]];
    }
    this.dodgeCooldown = Math.max(0, this.dodgeCooldown - dt);
    this.strikeCooldown = Math.max(0, this.strikeCooldown - dt);
    this.dodgeTime += dt;
    this.strikeTime += dt;
    const magnitude = Math.hypot(...input.move);
    const direction: [number, number] =
      magnitude > 1e-8
        ? [input.move[0] / Math.max(1, magnitude), input.move[1] / Math.max(1, magnitude)]
        : [0, 0];
    if (input.dodge && !this.previousInput.dodge && this.dodgeCooldown <= 0) {
      const away = this.directionToPlayer();
      this.dodgeDirection = magnitude > 1e-8 ? [input.move[0] / magnitude, input.move[1] / magnitude] : away;
      this.dodgeTime = 0;
      this.dodgeCooldown = 0.85;
      this.strikeTime = Infinity;
      this.emit("dodge");
    }
    this.state.invulnerable = this.dodgeTime >= 0.06 && this.dodgeTime < 0.28;
    const dodging = this.dodgeTime < 0.34;
    const speed = dodging ? 7.5 : this.strikeTime < 0.3 ? 1.2 : 3.8;
    const movement = dodging ? this.dodgeDirection : direction;
    this.state.playerPosition = this.bounded([
      this.state.playerPosition[0] + movement[0] * speed * dt,
      0,
      this.state.playerPosition[2] + movement[1] * speed * dt,
    ]);
    // Resolve a simple circular body exclusion. Attacks have their own facing/reach gate.
    const separation = planarDistance(this.state.playerPosition, this.state.creaturePosition);
    if (separation < 0.72) {
      const away = this.directionToPlayer();
      this.state.playerPosition = this.bounded([
        this.state.creaturePosition[0] + away[0] * 0.72,
        0,
        this.state.creaturePosition[2] + away[1] * 0.72,
      ]);
    }
    if (input.strike && !this.previousInput.strike && this.strikeCooldown <= 0 && !dodging) {
      this.strikeTime = 0;
      this.strikeApplied = false;
      this.strikeCooldown = 0.62;
      this.emit("strike");
    }
    if (!this.strikeApplied && this.strikeTime >= 0.12 && this.strikeTime < 0.3) {
      this.strikeApplied = true;
      if (planarDistance(this.state.playerPosition, this.state.creaturePosition) <= 2.2) {
        const exposed = this.state.phase === "recovery";
        const armored = this.state.phase === "attack" || this.state.phase === "telegraph";
        const damage = exposed ? 24 : armored ? 6 : 12;
        this.state.creatureHealth = Math.max(0, this.state.creatureHealth - damage);
        this.emit("enemy-hit", damage);
        if (this.state.creatureHealth === 0) {
          this.state.outcome = "won";
          this.transition("won");
          this.motion("hit", 0.05);
          this.emit("won");
        } else if (!armored) {
          this.transition("stagger");
          this.motion("hit", 0.05);
        }
      }
    }
    if (this.state.outcome === "playing") {
      if (this.state.phase === "watching") {
        if (this.state.phaseTime >= 0.8) {
          this.transition("approach");
          this.motion("walk");
        }
      } else if (this.state.phase === "approach") {
        const direction = this.directionToPlayer();
        this.state.creatureYaw = Math.atan2(direction[0], direction[1]);
        this.options.runtime?.setFacing(this.options.creatureId, this.state.creatureYaw);
        const edge = Math.hypot(this.state.creaturePosition[0], this.state.creaturePosition[2]);
        if (edge > this.options.encounter.arenaRadius - 2) {
          this.state.creaturePosition = [
            this.state.creaturePosition[0] * (1 - (1.45 * dt) / edge),
            0,
            this.state.creaturePosition[2] * (1 - (1.45 * dt) / edge),
          ];
          this.target(this.state.creaturePosition);
        } else if (planarDistance(this.state.playerPosition, this.state.creaturePosition) <= 3.2)
          this.beginAttack();
        else {
          this.state.creaturePosition = this.bounded([
            this.state.creaturePosition[0] + direction[0] * 1.45 * dt,
            0,
            this.state.creaturePosition[2] + direction[1] * 1.45 * dt,
          ]);
          this.target(this.state.creaturePosition);
        }
      } else if (
        this.state.phase === "telegraph" ||
        this.state.phase === "attack" ||
        this.state.phase === "recovery"
      ) {
        this.attackTime += dt;
        // Physical root-motion ownership belongs to RuntimeSession when attached.
        if (!this.options.runtime) this.state.creaturePosition = this.attackPosition(this.attackTime);
        const commitAt = Math.max(0.15, this.window.start - 0.32);
        if (this.state.phase === "telegraph" && this.attackTime >= commitAt) {
          this.transition("attack");
          this.emit("commit");
        }
        if (
          this.attackTime >= this.window.start &&
          this.attackTime < this.window.end &&
          !this.attackApplied
        ) {
          const dx = this.state.playerPosition[0] - this.state.creaturePosition[0],
            dz = this.state.playerPosition[2] - this.state.creaturePosition[2];
          const forward = dx * Math.sin(this.state.creatureYaw) + dz * Math.cos(this.state.creatureYaw);
          const side = dx * Math.cos(this.state.creatureYaw) - dz * Math.sin(this.state.creatureYaw);
          if (forward >= -0.25 && forward <= this.window.reach + 0.65 && Math.abs(side) <= 0.85) {
            this.attackApplied = true;
            if (this.state.invulnerable) this.emit("dodged");
            else {
              this.state.playerHealth = Math.max(0, this.state.playerHealth - 28);
              this.emit("player-hit", 28);
              if (this.state.playerHealth === 0) {
                this.state.outcome = "lost";
                this.transition("lost");
                this.motion("idle");
                this.emit("lost");
              }
            }
          }
        }
        if (
          this.state.outcome === "playing" &&
          this.attackTime >= this.window.end &&
          this.state.phase !== "recovery"
        ) {
          this.transition("recovery");
          this.emit("recovery");
        }
        if (this.state.outcome === "playing" && this.attackTime >= this.lunge.duration + 0.45) {
          this.transition("approach");
          this.motion("walk");
        }
      } else if (this.state.phase === "stagger" && this.state.phaseTime >= 0.6) {
        this.transition("approach");
        this.motion("walk");
      }
    }
    this.state.telegraph = this.state.phase === "telegraph";
    this.state.dodgeReady = this.dodgeCooldown <= 0;
    this.state.strikeReady = this.strikeCooldown <= 0 && !dodging;
    this.previousInput = { ...input, move: [...input.move] };
  }
  step(seconds: number, input: CreatureEncounterInput = emptyInput()): CreatureEncounterSnapshot {
    if (this.disposed) throw new Error("Encounter is disposed");
    if (
      !Number.isFinite(seconds) ||
      seconds < 0 ||
      seconds > 0.25 ||
      input.move.length !== 2 ||
      !input.move.every(Number.isFinite) ||
      typeof input.dodge !== "boolean" ||
      typeof input.strike !== "boolean"
    )
      throw new RangeError("Encounter requires finite bounded time and input.");
    if (this.options.runtime && seconds > CREATURE_ENCOUNTER_STEP + 1e-8)
      throw new RangeError("Advance the attached runtime after each 1/60 second encounter tick.");
    this.accumulator += seconds;
    while (this.accumulator + 1e-10 >= CREATURE_ENCOUNTER_STEP) {
      this.accumulator -= CREATURE_ENCOUNTER_STEP;
      this.tick(input);
    }
    return this.snapshot();
  }
  snapshot(): CreatureEncounterSnapshot {
    return structuredClone(this.state);
  }
  reset() {
    if (this.disposed) throw new Error("Encounter is disposed");
    this.accumulator = 0;
    this.previousInput = emptyInput();
    this.dodgeTime = Infinity;
    this.strikeTime = Infinity;
    this.dodgeCooldown = 0;
    this.strikeCooldown = 0;
    this.strikeApplied = false;
    this.attackTime = 0;
    this.attackApplied = false;
    this.state = {
      tick: 0,
      phase: "watching",
      phaseTime: 0,
      playerPosition: [...this.options.encounter.playerStart],
      creaturePosition: [0, 0, 0],
      playerHealth: 100,
      creatureHealth: 96,
      telegraph: false,
      invulnerable: false,
      outcome: "playing",
      motion: "",
      creatureYaw: 0,
      dodgeReady: true,
      strikeReady: true,
      events: [],
    };
    this.options.runtime?.setRootMotionPolicy(this.options.creatureId, "physical");
    this.target([0, 0, 0]);
    this.options.runtime?.setFacing(this.options.creatureId, 0);
    this.motion("idle", 0);
  }
  dispose() {
    this.disposed = true;
  }
}
