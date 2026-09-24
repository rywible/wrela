import type { Vec3 } from "@wrela/model";

import { z } from "zod";

export const WINTER_STEP = 1 / 60;
export const WINTER_HOME: Vec3 = [0, 0, 0];
export const WINTER_MARKERS = [
  { id: "pine", name: "Pine bend", position: [-16, 0, -16] as Vec3 },
  { id: "ridge", name: "Quiet ridge", position: [-34, 0, 12] as Vec3 },
  { id: "brook", name: "Snow hollow", position: [-10, 0, 34] as Vec3 },
] as const;
export type WinterInput = { move: [number, number]; interact: boolean; hurry: boolean };
export type WinterEvent = { tick: number; kind: "restored" | "home" | "lost" | "warning"; marker?: string };
const saveSchema = z
  .object({
    version: z.literal(1),
    game: z.literal("winter-valley"),
    tick: z.number().int().min(0).max(1_000_000),
    warmth: z.number().finite().min(0).max(150),
    restored: z.array(z.enum(["pine", "ridge", "brook"])).max(3),
    phase: z.enum(["exploring", "returning", "won", "lost"]),
    position: z.tuple([z.number().finite(), z.number().finite(), z.number().finite()]),
  })
  .strict();
export type WinterSave = z.infer<typeof saveSchema>;

/** Trusted game rules. All time advances come from successfully committed physics ticks. */
export class WinterValleyGame {
  private state: WinterSave = this.initial();
  private intent: WinterInput = { move: [0, 0], interact: false, hurry: false };
  private events: WinterEvent[] = [];
  private progress = 0;
  private nearby: string | null = null;
  private warned = false;
  paused = false;
  private initial(): WinterSave {
    return {
      version: 1,
      game: "winter-valley",
      tick: 0,
      warmth: 150,
      restored: [],
      phase: "exploring",
      position: [...WINTER_HOME],
    };
  }
  input(value: Partial<WinterInput>) {
    if (value.move && (value.move.length !== 2 || !value.move.every(Number.isFinite)))
      throw new Error("Movement must be a finite two-axis intent");
    this.intent = {
      ...this.intent,
      ...value,
      move: value.move
        ? (value.move.map((v) => Math.max(-1, Math.min(1, v))) as [number, number])
        : this.intent.move,
    };
  }
  movement(): [number, number] {
    if (this.paused || this.state.phase === "won" || this.state.phase === "lost") return [0, 0];
    const length = Math.max(1, Math.hypot(...this.intent.move));
    const speed = this.intent.hurry ? 6.2 : 4.3;
    return this.intent.move.map((v) => (v / length) * speed * WINTER_STEP) as [number, number];
  }
  pause(value = true) {
    this.paused = value;
    this.input({ move: [0, 0], interact: false, hurry: false });
  }
  restart() {
    this.state = this.initial();
    this.progress = 0;
    this.nearby = null;
    this.events = [];
    this.warned = false;
    this.paused = false;
    this.input({ move: [0, 0], interact: false, hurry: false });
  }
  private emit(kind: WinterEvent["kind"], marker?: string) {
    this.events.push({ tick: this.state.tick, kind, ...(marker ? { marker } : {}) });
    if (this.events.length > 32) this.events.shift();
  }
  step(position: Vec3) {
    if (this.paused || this.state.phase === "won" || this.state.phase === "lost") return;
    if (!position.every(Number.isFinite)) throw new Error("A game tick requires a finite actor position");
    this.state.position = [...position];
    this.state.tick++;
    this.state.warmth = Math.max(0, this.state.warmth - WINTER_STEP * (this.intent.hurry ? 1.35 : 1));
    const target = WINTER_MARKERS.find(
      (marker) =>
        !this.state.restored.includes(marker.id) &&
        Math.hypot(position[0] - marker.position[0], position[2] - marker.position[2]) <= 3.2,
    );
    if (target?.id !== this.nearby) this.progress = 0;
    this.nearby = target?.id ?? null;
    this.progress = target && this.intent.interact ? Math.min(1, this.progress + WINTER_STEP / 0.8) : 0;
    if (target && this.progress >= 1) {
      this.state.restored.push(target.id);
      this.state.warmth = Math.min(150, this.state.warmth + 25);
      this.progress = 0;
      this.nearby = null;
      this.warned = false;
      this.emit("restored", target.id);
      if (this.state.restored.length === 3) this.state.phase = "returning";
    }
    if (this.state.phase === "returning" && Math.hypot(position[0], position[2]) < 3.5) {
      this.state.phase = "won";
      this.emit("home");
    } else if (this.state.warmth <= 0 || position[1] < -30) {
      this.state.phase = "lost";
      this.emit("lost");
    } else if (this.state.warmth < 30 && !this.warned) {
      this.warned = true;
      this.emit("warning");
    }
  }
  inspect() {
    return {
      ...structuredClone(this.state),
      paused: this.paused,
      nearby: this.nearby,
      progress: this.progress,
      seconds: this.state.tick / 60,
    };
  }
  drainEvents(): WinterEvent[] {
    return this.events.splice(0);
  }
  save(): WinterSave {
    return structuredClone(this.state);
  }
  restore(value: unknown) {
    const saved = saveSchema.parse(value);
    if (
      new Set(saved.restored).size !== saved.restored.length ||
      ((saved.phase === "returning" || saved.phase === "won") && saved.restored.length !== 3) ||
      (saved.phase === "exploring" && saved.restored.length === 3)
    )
      throw new Error("Trail progress and game phase disagree");
    this.state = structuredClone(saved);
    this.progress = 0;
    this.nearby = null;
    this.events = [];
    this.warned = saved.warmth < 30;
    this.pause(true);
  }
}
