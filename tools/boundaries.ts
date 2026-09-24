import { dirname, relative, resolve } from "node:path";
import ts from "typescript";

const policy: Record<string, readonly string[]> = {
  model: [],
  examples: ["model"],
  compiler: ["model"],
  world: ["model", "compiler"],
  runtime: ["model", "compiler", "world"],
  "render-webgpu": ["model"],
  authoring: ["model"],
  review: ["model", "compiler", "world", "runtime", "render-webgpu", "authoring"],
  delivery: ["model", "compiler", "runtime"],
  studio: [
    "model",
    "examples",
    "compiler",
    "world",
    "runtime",
    "render-webgpu",
    "authoring",
    "review",
    "delivery",
    "creature-study",
  ],
  player: [
    "model",
    "examples",
    "compiler",
    "world",
    "runtime",
    "render-webgpu",
    "authoring",
    "delivery",
    "winter-valley",
    "creature-study",
    "switch-gate",
    "showcase",
  ],
};
export function importedSpecifiers(source: string, file = "source.ts") {
  const syntax = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true),
    imports: string[] = [];
  const visit = (node: ts.Node) => {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier &&
      ts.isStringLiteralLike(node.moduleSpecifier)
    )
      imports.push(node.moduleSpecifier.text);
    if (
      ts.isCallExpression(node) &&
      (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
        (ts.isIdentifier(node.expression) && node.expression.text === "require")) &&
      node.arguments[0] &&
      ts.isStringLiteralLike(node.arguments[0])
    )
      imports.push(node.arguments[0].text);
    ts.forEachChild(node, visit);
  };
  visit(syntax);
  return imports;
}
export async function checkBoundaries(root = process.cwd()) {
  const errors: string[] = [],
    packages = new Map<
      string,
      {
        directory: string;
        manifest: {
          name: string;
          exports?: string | Record<string, unknown>;
          dependencies?: Record<string, string>;
          devDependencies?: Record<string, string>;
        };
      }
    >();
  for (const file of new Bun.Glob("{packages,apps,games}/*/package.json").scanSync(root)) {
    const manifest = await Bun.file(resolve(root, file)).json();
    packages.set(manifest.name, { directory: dirname(file), manifest });
  }
  const owner = (file: string) => [...packages.values()].find((p) => file.startsWith(`${p.directory}/`));
  for (const file of new Bun.Glob("{packages,apps,games}/**/*.{ts,tsx}").scanSync(root)) {
    const own = owner(file);
    if (!own) {
      if (!file.startsWith("games/")) errors.push(`${file}: missing workspace manifest`);
      continue;
    }
    const name = own.directory.split("/")[1],
      source = await Bun.file(resolve(root, file)).text(),
      test = /\.test\.tsx?$/.test(file);
    const allowed = own.directory.startsWith("games/")
      ? [
          "model",
          "examples",
          "compiler",
          "world",
          "runtime",
          "render-webgpu",
          "winter-valley",
          "creature-study",
        ]
      : policy[name];
    if (!allowed) {
      errors.push(`${file}: missing dependency policy`);
      continue;
    }
    for (const spec of importedSpecifiers(source, file)) {
      let target = spec.startsWith("@wrela/")
        ? packages.get(spec.split("/").slice(0, 2).join("/"))
        : undefined;
      if (spec.startsWith(".")) {
        const path = relative(root, resolve(root, dirname(file), spec));
        target = owner(path);
        if (target && target !== own && target.directory.startsWith("packages/"))
          errors.push(`${file}: import ${target.manifest.name} through an exported entry point, not ${spec}`);
      }
      if (target && target !== own) {
        const targetName = target.directory.split("/")[1];
        if (!allowed.includes(targetName) && !(test && targetName === "examples"))
          errors.push(`${file}: ${name} cannot import ${targetName}`);
        if (spec.startsWith("@wrela/")) {
          const subpath = spec.slice(target.manifest.name.length),
            key = subpath ? `.${subpath}` : ".";
          if (
            typeof target.manifest.exports === "string"
              ? key !== "."
              : !target.manifest.exports || !Object.hasOwn(target.manifest.exports, key)
          )
            errors.push(`${file}: private entry point ${spec}`);
          if (
            !own.manifest.dependencies?.[target.manifest.name] &&
            !(test && own.manifest.devDependencies?.[target.manifest.name])
          )
            errors.push(`${file}: undeclared workspace dependency ${target.manifest.name}`);
        }
      } else if (spec.startsWith("@wrela/") && !target) errors.push(`${file}: unknown workspace ${spec}`);
      if (file.startsWith("packages/") && !test && /^(node:|bun$|bun:)/.test(spec))
        errors.push(`${file}: browser package imports ${spec}`);
    }
    if (file.startsWith("packages/") && !test && /\bBun\./.test(source))
      errors.push(`${file}: browser package depends on Bun`);
  }
  return errors;
}
if (import.meta.main) {
  const errors = await checkBoundaries();
  if (errors.length) throw Error(errors.join("\n"));
  console.log("Workspace public APIs and dependency boundaries verified.");
}
