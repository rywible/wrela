import { resolve } from "node:path";
import { cookProject } from "../packages/compiler/src/index";
import { parseProject, type Quality } from "../packages/model/src/index";

export async function compilerSourceFingerprint(): Promise<string> {
  const hasher = new Bun.CryptoHasher("sha256");
  const root = resolve(import.meta.dir, "..");
  const files = Array.from(new Bun.Glob("packages/{compiler,model}/src/**/*.ts").scanSync({ cwd: root }))
    .filter((path) => !path.endsWith(".test.ts"))
    .sort();
  for (const file of files) {
    const bytes = new Uint8Array(await Bun.file(resolve(root, file)).arrayBuffer());
    hasher.update(`${file}\0${bytes.byteLength}\0`);
    hasher.update(bytes);
  }
  return hasher.digest("hex");
}
export async function cookFile(source: string, destination: string, quality: Quality = "export") {
  if (resolve(source) === resolve(destination))
    throw new Error("Cooked products must not overwrite semantic source");
  const project = parseProject(await Bun.file(source).json());
  const bundle = cookProject(project, quality, await compilerSourceFingerprint());
  await Bun.write(destination, JSON.stringify(bundle));
  return {
    file: resolve(destination),
    artifacts: bundle.entries.length,
    source: bundle.source,
    compilerSource: bundle.compilerSource,
  };
}
if (import.meta.main) {
  const [source, destination, quality = "export"] = Bun.argv.slice(2);
  if (!source || !destination || !["interactive", "review", "export"].includes(quality))
    throw new Error("Usage: bun tools/cook.ts project.json products.json [interactive|review|export]");
  console.log(JSON.stringify(await cookFile(source, destination, quality as Quality)));
}
