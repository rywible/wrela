import { dirname, join, resolve } from "node:path";
import { verifyWorkSession } from "@wrela/authoring";
import { WorkFileStore } from "./authoring-work-store";

/** Rehome a real work record without modifying its source or original benchmark evidence. */
export async function checkWorkPortability(record: string, output: string) {
  const source = resolve(record),
    destination = resolve(output),
    raw = await Bun.file(source).json();
  const original = verifyWorkSession(raw.work ?? raw);
  const first = new WorkFileStore(join(destination, "retained")),
    second = new WorkFileStore(join(destination, "restored"));
  const retained = Array.isArray(raw.artifacts)
    ? await first.restore(raw)
    : verifyWorkSession(await first.retainEvidence(original, dirname(source)));
  if (!Array.isArray(raw.artifacts)) await first.put(retained, null);
  const bundle = await first.backup(retained.id);
  await Bun.write(join(destination, "portable-work.json"), JSON.stringify(bundle));
  const restored = await second.restore(bundle);
  if (
    restored.baselineKey !== original.baselineKey ||
    restored.proposals.some((p, i) => p.candidateKey !== original.proposals[i].candidateKey)
  )
    throw Error("Portability changed authored source identities");
  const rebundled = await second.backup(restored.id);
  if (rebundled.artifacts.length !== bundle.artifacts.length) throw Error("Evidence was lost on restore");
  const before = bundle.artifacts
    .map((a) => (a.encoding === "blob" ? a.sha256 : null))
    .filter(Boolean)
    .sort();
  const after = rebundled.artifacts
    .map((a) => (a.encoding === "blob" ? a.sha256 : null))
    .filter(Boolean)
    .sort();
  if (JSON.stringify(before) !== JSON.stringify(after)) throw Error("Restored image bytes differ");
  const result = {
    output: destination,
    baselineKey: original.baselineKey,
    candidates: restored.proposals.length,
    artifacts: bundle.artifacts.length,
    imageFiles: before.length,
    sourceUnchanged: true,
    imageBytesUnchanged: true,
  };
  await Bun.write(join(destination, "result.json"), JSON.stringify(result, null, 2));
  return result;
}
if (import.meta.main)
  console.log(JSON.stringify(await checkWorkPortability(process.argv[2], process.argv[3]), null, 2));
