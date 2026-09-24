import { z } from "zod";
import { parseWorkSession, verifyWorkSession, type WorkSession } from "./work-session";

const artifactSchema = z.strictObject({
  reference: z.string().min(1).max(1000),
  mime: z.string().max(200),
  encoding: z.enum(["blob", "json"]),
  data: z.string(),
  sha256: z.string().regex(/^[a-f0-9]{64}$/),
});
export const workBundleSchema = z.strictObject({
  format: z.literal("wrela-work-bundle"),
  version: z.literal(1),
  work: z.unknown(),
  references: z.array(z.string()).max(4096),
  artifacts: z.array(artifactSchema).max(4096),
});
export type WorkBundle = z.infer<typeof workBundleSchema>;
const limit = 64 * 1024 * 1024;
const hash = async (bytes: Uint8Array<ArrayBuffer>) =>
  [...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))]
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
function encode(bytes: Uint8Array) {
  let text = "";
  for (let i = 0; i < bytes.length; i += 8192) text += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return btoa(text);
}
function strings(value: unknown, visit: (value: string) => void): void {
  if (typeof value === "string") visit(value);
  else if (Array.isArray(value)) for (const child of value) strings(child, visit);
  else if (value && typeof value === "object")
    for (const child of Object.values(value)) strings(child, visit);
}
function replace(value: unknown, mapping: Map<string, string>): unknown {
  if (typeof value === "string") return mapping.get(value) ?? value;
  if (Array.isArray(value)) return value.map((v) => replace(v, mapping));
  if (value && typeof value === "object")
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, replace(v, mapping)]));
  return value;
}

/** Include referenced images and nested measurement reports; missing evidence makes export fail. */
export async function bundleWork(
  work: WorkSession,
  isReference: (value: string) => boolean,
  read: (reference: string) => Promise<Blob | object>,
): Promise<WorkBundle> {
  const parsed = parseWorkSession(work),
    references = new Set<string>(),
    artifacts: WorkBundle["artifacts"] = [];
  const collect = (value: unknown) =>
    strings(value, (s) => {
      if (isReference(s)) references.add(s);
    });
  const checkEvidence = (value: unknown): void => {
    if (!value || typeof value !== "object") return;
    if (Array.isArray(value)) {
      for (const child of value) checkEvidence(child);
      return;
    }
    for (const [key, child] of Object.entries(value)) {
      if (key === "evidence" && Array.isArray(child))
        for (const reference of child) {
          if (typeof reference === "string" && !isReference(reference) && !/^https?:\/\//.test(reference))
            throw Error(
              `Evidence is not retained in this work store: ${reference}. Attach local evidence before portable backup.`,
            );
        }
      checkEvidence(child);
    }
  };
  checkEvidence(parsed);
  collect(parsed);
  let size = JSON.stringify(parsed).length;
  for (const reference of references) {
    const value = await read(reference),
      blob = value instanceof Blob;
    if (!blob) {
      checkEvidence(value);
      collect(value);
    }
    const bytes = blob
      ? new Uint8Array(await value.arrayBuffer())
      : new TextEncoder().encode(JSON.stringify(value));
    size += Math.ceil((bytes.length * 4) / 3);
    if (size > limit) throw Error("Portable work evidence exceeds 64 MiB");
    artifacts.push({
      reference,
      mime: blob ? value.type : "application/json",
      encoding: blob ? "blob" : "json",
      data: encode(bytes),
      sha256: await hash(bytes),
    });
  }
  const bundle: WorkBundle = {
    format: "wrela-work-bundle",
    version: 1,
    work: parsed,
    references: [...references],
    artifacts,
  };
  if (JSON.stringify(bundle).length > limit) throw Error("Portable work evidence exceeds 64 MiB");
  return bundle;
}

/** Verify all bytes before storing anything, then relocate nested evidence references. */
export async function restoreWorkBundle(
  input: unknown,
  save: (name: string, value: Blob | object) => Promise<string>,
): Promise<WorkSession> {
  // The author CLI wraps command responses in {work}; accept that portable response directly.
  if (input && typeof input === "object" && "work" in input && !("format" in input)) input = input.work;
  if (JSON.stringify(input).length > limit) throw Error("Portable work evidence exceeds 64 MiB");
  const bundle = workBundleSchema.parse(input),
    entries = new Map<string, { name: string; value: Blob | object }>();
  verifyWorkSession(bundle.work);
  if (
    bundle.references.length !== bundle.artifacts.length ||
    new Set(bundle.references).size !== bundle.references.length ||
    bundle.artifacts.some((a) => !bundle.references.includes(a.reference))
  )
    throw Error("Work bundle is missing referenced evidence");
  for (const artifact of bundle.artifacts) {
    if (entries.has(artifact.reference)) throw Error("Duplicate artifact reference");
    const bytes = Uint8Array.from(atob(artifact.data), (c) => c.charCodeAt(0));
    if ((await hash(bytes)) !== artifact.sha256)
      throw Error("Work evidence bytes do not match their checksum");
    entries.set(artifact.reference, {
      name: artifact.reference.split("/").at(-1) || "artifact",
      value:
        artifact.encoding === "json"
          ? JSON.parse(new TextDecoder().decode(bytes))
          : new Blob([bytes], { type: artifact.mime }),
    });
  }
  const mapping = new Map<string, string>(),
    visiting = new Set<string>();
  async function restore(reference: string): Promise<void> {
    if (mapping.has(reference)) return;
    if (visiting.has(reference)) throw Error("Cyclic work evidence references");
    visiting.add(reference);
    const entry = entries.get(reference);
    if (!entry) throw Error("Missing work evidence");
    if (!(entry.value instanceof Blob)) {
      const dependencies = new Set<string>();
      strings(entry.value, (s) => {
        if (entries.has(s)) dependencies.add(s);
      });
      for (const dependency of dependencies) await restore(dependency);
    }
    mapping.set(
      reference,
      await save(
        entry.name,
        entry.value instanceof Blob ? entry.value : (replace(entry.value, mapping) as object),
      ),
    );
    visiting.delete(reference);
  }
  for (const reference of entries.keys()) await restore(reference);
  return parseWorkSession(replace(bundle.work, mapping));
}
