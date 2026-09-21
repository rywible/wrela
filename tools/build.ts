import { mkdir, rm } from "node:fs/promises";
import { isAbsolute, join, relative, resolve } from "node:path";
import { cookProject } from "@wrela/compiler";
import { referenceProject } from "@wrela/model";
import { compilerSourceFingerprint } from "./cook";
import { writeEvidence } from "./evidence";

export function snapshotKey(files: { path: string; bytes: Uint8Array }[]): string {
  const hash = new Bun.CryptoHasher("sha256");
  for (const file of [...files].sort((a, b) => a.path.localeCompare(b.path))) {
    hash.update(`${file.path}\0${file.bytes.byteLength}\0`);
    hash.update(file.bytes);
  }
  return hash.digest("hex").slice(0, 20);
}

/** Build only into the checkout's dist or ignored output trees; never remove a source directory. */
export async function buildStudio(destination = "dist") {
  const directory = resolve(destination);
  const local = relative(process.cwd(), directory);
  if (isAbsolute(local) || !/^(dist|output)(?:[\\/]|$)/.test(local)) {
    throw new Error("Build destination must be inside this checkout's dist/ or output/ directory.");
  }
  await rm(directory, { recursive: true, force: true });
  await mkdir(join(directory, "player"), { recursive: true });
  const compilerSource = await compilerSourceFingerprint();
  const compilerDefine = { WRELA_COMPILER_SOURCE: JSON.stringify(compilerSource) };
  for (const [entry, outdir] of [
    ["apps/studio/index.html", directory],
    ["apps/player/index.html", join(directory, "player")],
  ]) {
    const result = await Bun.build({
      entrypoints: [entry],
      outdir,
      target: "browser",
      minify: true,
      sourcemap: "linked",
      loader: { ".wgsl": "text" },
      define: { ...compilerDefine, "process.env.NODE_ENV": '"production"', WRELA_PRODUCTION: "true" },
    });
    if (!result.success) throw new AggregateError(result.logs, "Build failed");
  }
  const notices = `${await Bun.file("LICENSE").text()}\n\n${await Bun.file("THIRD_PARTY_NOTICES.txt").text()}`;
  const banner = `/*! Wrela Studio distribution notices\n${notices.replace(/\*\//g, "* /")}\n*/`;
  for (const [entry, naming] of [
    ["packages/compiler/src/worker.ts", "compile-worker.js"],
    ["apps/player/src/main.ts", "player-bundle.js"],
  ]) {
    const result = await Bun.build({
      entrypoints: [entry],
      outdir: directory,
      naming,
      banner,
      target: "browser",
      minify: true,
      loader: { ".wgsl": "text" },
      define: { ...compilerDefine, WRELA_PRODUCTION: "true" },
    });
    if (!result.success) throw new AggregateError(result.logs, "Build failed");
  }
  for (const name of [
    "LICENSE",
    "THIRD_PARTY_NOTICES",
    "THIRD_PARTY_NOTICES.md",
    "THIRD_PARTY_NOTICES.txt",
  ]) {
    const file = Bun.file(name);
    if (await file.exists()) await Bun.write(join(directory, name), file);
  }
  const manifest = await writeEvidence(directory, "production-build");
  await Bun.write(join(directory, "compiler-identity.json"), JSON.stringify({ compilerSource }));
  await Bun.write(
    join(directory, "cooked-project.json"),
    JSON.stringify(cookProject(referenceProject(), "review", compilerSource)),
  );
  const files = Array.from(new Bun.Glob("**/*").scanSync({ cwd: directory, onlyFiles: true }))
    .filter((path) => !path.endsWith(".map"))
    .sort();
  const assets = ["/", "/build-manifest.json", ...files.map((path) => `/${path}`)];
  const version = snapshotKey(
    await Promise.all(
      files
        .filter((path) => path !== "source-manifest.json")
        .map(async (path) => ({
          path,
          bytes: new Uint8Array(await Bun.file(join(directory, path)).arrayBuffer()),
        })),
    ),
  );
  await Bun.write(
    join(directory, "sw.js"),
    `const CACHE='wrela-${version}';const ASSETS=${JSON.stringify(assets)};
self.addEventListener('install',event=>event.waitUntil(caches.open(CACHE).then(cache=>cache.addAll(ASSETS))));
self.addEventListener('activate',event=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(key=>key.startsWith('wrela-')&&key!==CACHE).map(key=>caches.delete(key)))).then(()=>self.clients.claim())));
self.addEventListener('fetch',event=>{if(event.request.method==='GET'&&new URL(event.request.url).origin===location.origin)event.respondWith(caches.match(event.request).then(cached=>cached||fetch(event.request)));});`,
  );
  const products = await Promise.all(
    Array.from(new Bun.Glob("**/*").scanSync({ cwd: directory, onlyFiles: true }))
      .filter((path) => !path.endsWith(".map") && path !== "source-manifest.json")
      .sort()
      .map(async (path) => {
        const bytes = new Uint8Array(await Bun.file(join(directory, path)).arrayBuffer());
        return {
          path,
          bytes: bytes.byteLength,
          sha256: new Bun.CryptoHasher("sha256").update(bytes).digest("hex"),
        };
      }),
  );
  await Bun.write(
    join(directory, "build-manifest.json"),
    JSON.stringify(
      {
        schemaVersion: 1,
        sourceFingerprint: manifest.sourceFingerprint,
        buildFingerprint: new Bun.CryptoHasher("sha256").update(JSON.stringify(products)).digest("hex"),
        products,
      },
      null,
      2,
    ),
  );
  console.log(
    `Built Studio, standalone player, worker and offline snapshot (${assets.length} assets) in ${directory}.`,
  );
  return directory;
}
if (import.meta.main) {
  const index = process.argv.indexOf("--outdir");
  if (index >= 0 && !process.argv[index + 1]) throw new Error("--outdir requires a directory.");
  await buildStudio(index >= 0 ? process.argv[index + 1] : "dist");
}
