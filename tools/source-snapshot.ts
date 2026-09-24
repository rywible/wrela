import { mkdir, symlink } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { sourceManifest } from "./evidence";

/** Copy verified bytes once so a multi-category review can run while authors keep editing. */
export async function snapshotSource(destination: string, scenario: string) {
  const original = process.cwd();
  const manifest = await sourceManifest(scenario);
  await mkdir(destination, { recursive: true });
  for (const file of manifest.files) {
    const bytes = new Uint8Array(await Bun.file(file.path).arrayBuffer());
    if (new Bun.CryptoHasher("sha256").update(bytes).digest("hex") !== file.sha256)
      throw new Error(`Source changed while snapshotting ${file.path}; rerun capture`);
    await mkdir(dirname(join(destination, file.path)), { recursive: true });
    await Bun.write(join(destination, file.path), bytes);
  }
  await symlink(resolve(original, "node_modules"), join(destination, "node_modules"), "dir");
  await Bun.write(
    join(dirname(destination), "original-source-manifest.json"),
    JSON.stringify(manifest, null, 2),
  );
  return manifest;
}
