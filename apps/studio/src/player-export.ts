import type { CookedProject } from "@wrela/compiler";
import type { Project } from "@wrela/model";

const json = (value: unknown) => JSON.stringify(value).replace(/</g, "\\u003c");
export function standalonePlayer(
  project: Project,
  playerCode: string,
  workerCode: string,
  cooked: CookedProject,
): Blob {
  const title = project.name.replace(/[<>&"']/g, "");
  const script = (value: string) => value.replace(/<\/script/gi, "<\\/script");
  return new Blob(
    [
      `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${title} · Wrela</title></head><body>`,
      `<script type="application/json" id="wrela-project">${json(project)}</script>`,
      `<script type="application/json" id="wrela-cooked">${json(cooked)}</script>`,
      `<script type="text/plain" id="wrela-worker">${script(workerCode)}</script>`,
      `<script type="module">${script(playerCode)}</script></body></html>`,
    ],
    { type: "text/html" },
  );
}
