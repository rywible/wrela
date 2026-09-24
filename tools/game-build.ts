import { join } from "node:path";
import { cookProject } from "@wrela/compiler";
import { standalonePlayer } from "@wrela/delivery";
import { releaseProject } from "@wrela/model";
import { gameManifestSchema } from "@wrela/runtime";
import { gameModules } from "../games/registry";

/** The build discovers data manifests; implementation code comes only from the trusted registry. */
export async function buildGames(directory: string, compilerSource: string) {
  const [player, worker] = await Promise.all([
    Bun.file(join(directory, "player-bundle.js")).text(),
    Bun.file(join(directory, "compile-worker.js")).text(),
  ]);
  const games = [];
  for (const file of [...new Bun.Glob("games/*/game.json").scanSync()].sort()) {
    const manifest = gameManifestSchema.parse(await Bun.file(file).json()),
      definition = gameModules.find((game) => game.id === manifest.game);
    if (!definition || definition.version !== manifest.gameVersion)
      throw Error(`Unknown trusted game ${manifest.game}`);
    const project = releaseProject({
      ...definition.project(),
      entry: manifest.entry,
      dynamicRoots: manifest.dynamicRoots,
    });
    const cooked = cookProject(project, "review", compilerSource),
      path = `games/${manifest.game}/index.html`;
    await Bun.write(join(directory, path), standalonePlayer(project, player, worker, cooked, manifest));
    games.push({ ...manifest, title: definition.title, path, sourceKey: cooked.source });
  }
  await Bun.write(join(directory, "games.json"), JSON.stringify(games, null, 2));
  return games;
}
