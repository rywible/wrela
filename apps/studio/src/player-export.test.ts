import { expect, test } from "bun:test";
import { cookProject, createCookedCompiler } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import { releaseProject } from "@wrela/model";
import { standalonePlayer } from "./player-export";

test("portable player preserves source and cooked products without allowing source to close its script", async () => {
  const project = referenceProject();
  project.name = 'Snow </script><script>broken()</script> & "valley"';
  const cooked = cookProject(project, "interactive", "export-test");
  const html = await standalonePlayer(
    project,
    'const closing = "</script>";',
    "// </script>\nself.onmessage = () => {};",
    cooked,
  ).text();
  const scripts = [...html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/g)];
  expect(scripts).toHaveLength(4);
  expect(JSON.parse(scripts[0][1])).toEqual(JSON.parse(JSON.stringify(releaseProject(project))));
  const restored = JSON.parse(scripts[1][1]);
  expect(restored).toEqual(JSON.parse(JSON.stringify(cooked)));
  const compile = createCookedCompiler(restored, project, { compilerSource: "export-test" });
  const character = project.documents.find((document) => document.kind === "character");
  if (!character) throw new Error("Fixture is missing a character");
  expect((await compile(character, "interactive"))?.kind).toBe("character");
  expect(scripts[2][1]).toContain("<\\/script>");
  expect(scripts[3][1]).toContain("<\\/script>");
});
