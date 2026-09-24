import { contentKey, type EvaluatedScene, type Project, type Quality } from "@wrela/model";
import { z } from "zod";
import type { BrowserSceneHost } from "./scene-host";

export const GAME_STEP = 1 / 60;
export const gameManifestSchema = z.strictObject({
  version: z.literal(1),
  game: z.string().regex(/^[a-z][a-z0-9-]*$/),
  gameVersion: z.number().int().positive(),
  entry: z.string().min(1),
  dynamicRoots: z.array(z.string()).max(256).default([]),
});
export type GameManifest = z.infer<typeof gameManifestSchema>;
export type GameContext = { project: Project; sceneHost?: BrowserSceneHost; quality?: Quality };
/** Trusted TypeScript selected by the host registry. Authored JSON can select an ID;
 * it cannot supply code, imports, or an arbitrary module URL. Inputs are semantic intents. */
export interface GameModule {
  initialize(context: GameContext): void | Promise<void>;
  input(intent: unknown): void;
  fixedStep(seconds: number): void | Promise<void>;
  inspect(): unknown;
  save(): unknown;
  load(state: unknown): void | Promise<void>;
  scene?(): EvaluatedScene;
  dispose(): void;
}
export type GameDefinition = {
  id: string;
  version: number;
  title: string;
  description: string;
  inputs: Record<string, string>;
  inputSchema: Record<string, unknown>;
  project(): Project;
  create(): GameModule;
};
const saveSchema = z.strictObject({
  version: z.literal(1),
  game: z.string(),
  gameVersion: z.number().int(),
  sourceKey: z.string(),
  state: z.unknown(),
});
export class GameDriver {
  private disposed = false;
  private accumulator = 0;
  private serial: Promise<unknown> = Promise.resolve();
  private constructor(
    readonly definition: GameDefinition,
    readonly module: GameModule,
    readonly project: Project,
  ) {}
  static async create(definition: GameDefinition, context: GameContext) {
    const module = definition.create(),
      driver = new GameDriver(definition, module, context.project);
    try {
      await module.initialize(context);
      return driver;
    } catch (error) {
      module.dispose();
      throw error;
    }
  }
  private assertActive() {
    if (this.disposed) throw Error("Game is disposed");
  }
  input(intent: unknown) {
    this.assertActive();
    this.module.input(intent);
  }
  inspect() {
    this.assertActive();
    return { game: this.definition.id, version: this.definition.version, state: this.module.inspect() };
  }
  private enqueue<T>(action: () => T | Promise<T>): Promise<T> {
    const next = this.serial.then(() => {
      this.assertActive();
      return action();
    });
    this.serial = next.catch(() => undefined);
    return next;
  }
  /** At most eight fixed steps per frame; long suspensions cannot create an unbounded backlog. */
  advance(seconds: number) {
    if (!Number.isFinite(seconds) || seconds < 0) return Promise.reject(Error("Invalid frame duration"));
    return this.enqueue(async () => {
      this.accumulator = Math.min(this.accumulator + seconds, GAME_STEP * 8);
      let steps = 0;
      while (this.accumulator + 1e-10 >= GAME_STEP) {
        await this.module.fixedStep(GAME_STEP);
        this.accumulator -= GAME_STEP;
        steps++;
      }
      return steps;
    });
  }
  save() {
    return this.enqueue(() => ({
      version: 1 as const,
      game: this.definition.id,
      gameVersion: this.definition.version,
      sourceKey: contentKey(this.project),
      state: this.module.save(),
    }));
  }
  load(input: unknown) {
    return this.enqueue(async () => {
      const save = saveSchema.parse(input);
      if (
        save.game !== this.definition.id ||
        save.gameVersion !== this.definition.version ||
        save.sourceKey !== contentKey(this.project)
      )
        throw Error("Save belongs to a different game or source revision");
      await this.module.load(save.state);
      this.accumulator = 0;
    });
  }
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.module.dispose();
  }
}
