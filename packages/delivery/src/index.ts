import type { CookedProject } from "@wrela/compiler";
import { type Project, releaseProject } from "@wrela/model";
import { type GameManifest, gameManifestSchema } from "@wrela/runtime";

const json = (value: unknown) => JSON.stringify(value).replace(/</g, "\\u003c");
export function standalonePlayer(
  project: Project,
  playerCode: string,
  workerCode: string,
  cooked: CookedProject,
  game?: GameManifest,
): Blob {
  if (game) {
    game = gameManifestSchema.parse(game);
    if (project.entry !== game.entry) throw Error("Game entry differs from source");
  }
  project = releaseProject(project);
  const title = project.name.replace(/[<>&"']/g, "");
  const script = (value: string) => value.replace(/<\/script/gi, "<\\/script");
  return new Blob(
    [
      `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${title} · Wrela</title></head><body>`,
      `<script type="application/json" id="wrela-project">${json(project)}</script>`,
      `<script type="application/json" id="wrela-cooked">${json(cooked)}</script>`,
      `<script type="text/plain" id="wrela-worker">${script(workerCode)}</script>`,
      ...(game ? [`<script type="application/json" id="wrela-game">${json(game)}</script>`] : []),
      `<script type="module">${script(playerCode)}</script></body></html>`,
    ],
    { type: "text/html" },
  );
}
