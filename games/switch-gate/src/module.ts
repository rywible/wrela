import { referenceProject, shape } from "@wrela/examples";
import { type Camera, type EvaluatedScene, type Project, parseProject, transformMatrix } from "@wrela/model";
import type { GameContext, GameDefinition, GameModule } from "@wrela/runtime";
import { z } from "zod";

export function switchGateProject(): Project {
  const base = referenceProject(),
    stone = base.documents.find((d) => d.id === "river-stone");
  if (stone?.kind !== "object") throw Error("Missing object template");
  const objects = [
    { id: "switch", name: "Brass switch", position: [-2, 0.25, 0], size: [0.6, 0.25, 0.6] },
    { id: "gate", name: "Exit gate", position: [2, 1.5, 0], size: [0.2, 1.5, 2] },
    { id: "traveler", name: "Traveler", position: [0, 0.5, 0], size: [0.3, 0.5, 0.3] },
  ].map(({ id, name, position, size }) => ({
    ...stone,
    id,
    name,
    dependencies: [],
    field: {
      ...stone.field,
      root: id,
      bounds: {
        min: position.map((v, axis) => v - size[axis] - 0.2) as [number, number, number],
        max: position.map((v, axis) => v + size[axis] + 0.2) as [number, number, number],
      },
      nodes: [shape(id, name, position as [number, number, number], size as [number, number, number], "box")],
    },
  }));
  const stage = base.documents.find((d) => d.kind === "stage");
  if (!stage || stage.kind !== "stage") throw Error("Missing stage template");
  return parseProject({
    schemaVersion: 1,
    id: "switch-gate",
    name: "The Quiet Gate",
    entry: "gate-stage",
    documents: [
      ...base.documents.filter((d) => [stone.material, stage.environment, stage.lighting].includes(d.id)),
      ...objects,
      {
        ...stage,
        id: "gate-stage",
        name: "The Quiet Gate",
        subjects: objects.map((o) => o.id),
        dependencies: [],
      },
    ],
  });
}
const stateSchema = z.strictObject({
  tick: z.number().int().nonnegative(),
  position: z.tuple([z.number().finite().min(-5).max(5), z.number().finite().min(-3).max(3)]),
  activated: z.boolean(),
  gate: z.number().finite().min(0).max(1),
  won: z.boolean(),
});
const inputSchema = z.strictObject({
  move: z.tuple([z.number().finite().min(-1).max(1), z.number().finite().min(-1).max(1)]).optional(),
  interact: z.boolean().optional(),
});
export class SwitchGateModule implements GameModule {
  private state = { tick: 0, position: [-4, 0] as [number, number], activated: false, gate: 0, won: false };
  private intent = { move: [0, 0] as [number, number], interact: false };
  private context?: GameContext;
  private camera: Camera = { position: [8, 8, 10], target: [0, 0.5, 0], fov: 48 };
  async initialize(context: GameContext) {
    this.context = context;
    await context.sceneHost?.prepare(context.project.entry, undefined, context.quality ?? "interactive");
  }
  input(value: unknown) {
    this.intent = { ...this.intent, ...inputSchema.parse(value) };
  }
  fixedStep(seconds: number) {
    if (this.state.won) return;
    this.state.tick++;
    const [x, z] = this.state.position,
      length = Math.max(1, Math.hypot(...this.intent.move));
    let nextX = Math.max(-5, Math.min(5, x + (this.intent.move[0] * seconds * 3) / length));
    const nextZ = Math.max(-3, Math.min(3, z + (this.intent.move[1] * seconds * 3) / length));
    if (x < 1.4 && nextX >= 1.4 && this.state.gate < 0.95) nextX = 1.39;
    this.state.position = [nextX, nextZ];
    if (Math.hypot(nextX + 2, nextZ) < 0.9 && this.intent.interact) this.state.activated = true;
    if (this.state.activated) this.state.gate = Math.min(1, this.state.gate + seconds);
    if (nextX > 3.5 && this.state.gate >= 0.95) this.state.won = true;
    this.context?.sceneHost?.advance(seconds, this.camera);
  }
  inspect() {
    return {
      ...structuredClone(this.state),
      objective: this.state.won
        ? "The gate is behind you. Journey complete."
        : this.state.activated
          ? "Pass through the open gate."
          : "Find the switch and press E to open the gate.",
      canInteract: Math.hypot(this.state.position[0] + 2, this.state.position[1]) < 0.9,
      collision: this.state.gate < 0.95 ? "gate closed" : "gate open",
    };
  }
  save() {
    return structuredClone(this.state);
  }
  load(value: unknown) {
    const state = stateSchema.parse(value);
    if ((!state.activated && state.gate !== 0) || (state.won && state.gate < 0.95))
      throw Error("Inconsistent gate save");
    this.state = state;
    this.intent = { move: [0, 0], interact: false };
  }
  scene(): EvaluatedScene {
    if (!this.context?.sceneHost) throw Error("Scene host unavailable");
    const scene = this.context.sceneHost.extract(this.camera);
    scene.surfaces = scene.surfaces.map((surface) =>
      surface.source === "traveler"
        ? { ...surface, matrix: transformMatrix([this.state.position[0], 0, this.state.position[1]]) }
        : surface.source === "gate"
          ? { ...surface, matrix: transformMatrix([0, this.state.gate * 3.5, 0]) }
          : surface,
    );
    return scene;
  }
  dispose() {
    this.context = undefined;
  }
}
export const switchGateGame: GameDefinition = {
  inputSchema: z.toJSONSchema(inputSchema),
  id: "switch-gate",
  version: 1,
  title: "The Quiet Gate",
  description: "A switch, a moving gate, and a way through.",
  inputs: { move: "[x,z] movement axes, WASD or arrows", interact: "Activate the nearby switch with E" },
  project: switchGateProject,
  create: () => new SwitchGateModule(),
};
