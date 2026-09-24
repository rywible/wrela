import { referenceProject } from "@wrela/examples";
import type { GameContext, GameDefinition, GameModule } from "@wrela/runtime";
import { z } from "zod";
import { WinterPlayer } from "./player";
import { winterStyle } from "./style";

const inputSchema = z.strictObject({
  move: z.tuple([z.number().finite(), z.number().finite()]).optional(),
  interact: z.boolean().optional(),
  hurry: z.boolean().optional(),
});
class WinterModule implements GameModule {
  private player!: WinterPlayer;
  private style?: HTMLStyleElement;
  async initialize({ project, sceneHost }: GameContext) {
    if (!sceneHost) throw Error("Winter Valley requires a scene host");
    this.style = document.createElement("style");
    this.style.textContent = winterStyle;
    document.head.append(this.style);
    this.player = new WinterPlayer(sceneHost, project, () => {}, {
      profile: () => "balanced",
      change: async () => {
        throw Error("Use host graphics settings");
      },
    });
    await this.player.start(false);
  }
  input(value: unknown) {
    this.player.input(inputSchema.parse(value));
  }
  fixedStep(seconds: number) {
    this.player.update(seconds, true);
  }
  inspect() {
    return this.player.inspect();
  }
  save() {
    return this.player.save();
  }
  load(state: unknown) {
    return this.player.restore(state);
  }
  scene() {
    return this.player.scene();
  }
  dispose() {
    this.player?.dispose();
    this.style?.remove();
  }
}
export const winterGame: GameDefinition = {
  inputSchema: z.toJSONSchema(inputSchema),
  id: "winter-valley",
  version: 1,
  title: "Winter Valley",
  description: "Restore the trail lanterns and return home before the cold catches you.",
  inputs: {
    move: "[x,z] movement axes",
    interact: "Hold to restore a lantern",
    hurry: "Run at the cost of warmth",
  },
  project: referenceProject,
  create: () => new WinterModule(),
};
