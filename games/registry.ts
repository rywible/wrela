import type { GameDefinition } from "@wrela/runtime";
import { switchGateGame } from "./switch-gate/src/module";
import { winterGame } from "./winter-valley/src/module";
/** Trusted composition root. Authored manifests select one of these IDs, never a script URL. */
export const gameModules: readonly GameDefinition[] = [winterGame, switchGateGame];
