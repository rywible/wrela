import { gameManifestSchema } from "@wrela/runtime";
import { gameModules } from "../../../games/registry";
import { launchGamePlayer } from "./module-player";

const embedded = document.getElementById("wrela-game"),
  manifest = embedded ? gameManifestSchema.parse(JSON.parse(embedded.textContent ?? "")) : undefined;
const requested = new URLSearchParams(location.search).get("game") ?? manifest?.game;
if (requested) {
  const definition = gameModules.find((game) => game.id === requested);
  if (!definition || (manifest && definition.version !== manifest.gameVersion))
    throw Error("This trusted game module is unavailable in the player build");
  await launchGamePlayer(definition, manifest);
} else {
  // The showcase owns its scene selectors and study UI. The engine player knows no sample IDs.
  await import("../../../games/showcase/src/player");
}
