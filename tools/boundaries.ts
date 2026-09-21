import { dirname, relative, resolve } from "node:path";

const allowed: Record<string, string[]> = {
  model: [],
  compiler: ["model"],
  world: ["model", "compiler"],
  runtime: ["model", "compiler", "world"],
  "render-webgpu": ["model"],
  authoring: ["model"],
  studio: ["model", "compiler", "world", "runtime", "render-webgpu", "authoring"],
  player: ["model", "compiler", "world", "runtime", "render-webgpu"],
};
const errors: string[] = [];
for (const file of new Bun.Glob("{packages,apps}/**/*.{ts,tsx}").scanSync(".")) {
  const owner = file.split("/")[1];
  if (!allowed[owner]) {
    errors.push(`${file}: workspace has no declared dependency policy`);
    continue;
  }
  const text = await Bun.file(file).text();
  for (const match of text.matchAll(
    /(?:from\s*|import\s*\()\s*['"](@wrela\/([^/'"]+)|(?:\.\.\/)+[^'"]+)['"]/g,
  )) {
    let target: string | undefined = match[2];
    if (!target) {
      const path = relative(process.cwd(), resolve(dirname(file), match[1]));
      target = path.match(/^(?:packages|apps)\/([^/]+)/)?.[1];
    }
    if (target && target !== owner && !allowed[owner].includes(target))
      errors.push(`${file}: ${owner} cannot import ${target}`);
  }
  if (
    file.startsWith("packages/") &&
    !file.endsWith(".test.ts") &&
    /\bBun\.|from\s*['"](?:bun|node:)/.test(text)
  )
    errors.push(`${file}: browser packages cannot depend on Bun or Node`);
}
if (errors.length) throw new Error(errors.join("\n"));
console.log("Workspace boundaries verified.");
