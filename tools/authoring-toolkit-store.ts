import { mkdir, open, rename, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import {
  appendWorkEvent,
  builtinAuthoringToolkit,
  capabilityGapSchema,
  capabilityIdSchema,
  instantiateCreationRecipe,
  promoteCreationRecipe,
  promoteToolkitEntry,
  recordToolkitTrial,
  searchAuthoringToolkit,
  type ToolkitEntry,
  toolkitEntrySchema,
} from "@wrela/authoring";
import { contentKey, idSchema } from "@wrela/model";
import { z } from "zod";
import { WorkFileStore } from "./authoring-work-store";

export const toolkitRequestSchema = z.discriminatedUnion("action", [
  z.strictObject({ action: z.literal("search"), query: z.string().max(1000).default("") }),
  z.strictObject({ action: z.literal("gap"), gap: capabilityGapSchema }),
  z.strictObject({ action: z.literal("register"), entry: toolkitEntrySchema }),
  z.strictObject({
    action: z.literal("trial"),
    id: idSchema,
    revision: z.string(),
    proposal: idSchema,
    family: z.string().min(1),
    elapsedMs: z.number().nonnegative(),
    expectedKey: z.string(),
  }),
  z.strictObject({
    action: z.literal("promote"),
    id: idSchema,
    revision: z.string(),
    status: z.enum(["validated", "approved"]),
    expectedKey: z.string(),
  }),
  z.strictObject({
    action: z.literal("remember-creation"),
    proposal: idSchema,
    id: idSchema,
    revision: z.string(),
    description: z.string(),
    roots: z.array(idSchema).min(1),
    capabilities: z.array(z.object({ id: capabilityIdSchema, version: z.string() })).default([]),
    expectedKey: z.string(),
  }),
  z.strictObject({
    action: z.literal("instantiate"),
    id: idSchema,
    revision: z.string(),
    namespace: idSchema,
  }),
]);
export async function runAuthoringToolkit(
  workspace: string,
  workId: string,
  raw: z.input<typeof toolkitRequestSchema>,
  requestDirectory = workspace,
) {
  const request = toolkitRequestSchema.parse(raw),
    store = new WorkFileStore(workspace),
    directory = join(resolve(workspace), ".wrela", "toolkit");
  await mkdir(directory, { recursive: true });
  const path = (id: string, revision: string) => join(directory, `${id}-${contentKey(revision)}.json`);
  const write = async (entry: ToolkitEntry, expected: string | null) => {
    const file = path(entry.id, entry.revision),
      lock = await open(`${file}.lock`, "wx");
    try {
      const old = (await Bun.file(file).exists())
        ? toolkitEntrySchema.parse(await Bun.file(file).json())
        : null;
      if ((old ? contentKey(old) : null) !== expected) throw Error("Toolkit entry changed; search again");
      if (old && contentKey(old.implementation) !== contentKey(entry.implementation))
        throw Error("A published implementation version is immutable; create a new revision");
      const temp = `${file}.${crypto.randomUUID()}.tmp`;
      await Bun.write(temp, JSON.stringify(entry, null, 2));
      await rename(temp, file);
      return { entry, key: contentKey(entry) };
    } finally {
      await lock.close();
      await rm(`${file}.lock`);
    }
  };
  const read = async (id: string, revision: string) =>
    toolkitEntrySchema.parse(await Bun.file(path(id, revision)).json());
  if (request.action === "search") {
    const entries: ToolkitEntry[] = [];
    for (const file of new Bun.Glob("*.json").scanSync(directory)) {
      const parsed = toolkitEntrySchema.safeParse(await Bun.file(join(directory, file)).json());
      if (parsed.success) entries.push(parsed.data);
    }
    return {
      entries: searchAuthoringToolkit(
        [
          ...builtinAuthoringToolkit().filter(
            (b) => !entries.some((e) => e.id === b.id && e.revision === b.revision),
          ),
          ...entries,
        ],
        request.query,
      ).map(({ matchScore, ...entry }) => ({ ...entry, matchScore, key: contentKey(entry) })),
      scope: "Local versioned capabilities; status records evidence, not universal artistic quality",
    };
  }
  if (request.action === "gap") {
    const work = await store.get(workId);
    if (request.gap.work !== work.id) throw Error("Gap belongs to another work session");
    const retained = await store.retainEvidence(request.gap, requestDirectory);
    const evidence = await store.saveArtifact("capability-gap.json", retained);
    const next = appendWorkEvent(work, {
      id: `gap-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      kind: "tool-development",
      actor: "author",
      summary: request.gap.desiredResult,
      evidence: [evidence],
      durationMs: request.gap.engineeringMs,
    });
    return { evidence, key: await store.put(next, contentKey(work)) };
  }
  if (request.action === "register") {
    if (request.entry.status !== "experimental" || request.entry.trials.length)
      throw Error("New entries start experimental with no transfer claims");
    const previews = [];
    for (const reference of request.entry.previews) {
      const file = Bun.file(reference);
      if (file.size > 8 * 1024 * 1024) throw Error("Preview exceeds 8 MiB");
      previews.push(
        await store.saveArtifact(
          "toolkit-preview.png",
          new Blob([await file.arrayBuffer()], { type: "image/png" }),
        ),
      );
    }
    return write({ ...request.entry, previews }, null);
  }
  const work = await store.get(workId);
  if (request.action === "remember-creation") {
    if (contentKey(work) !== request.expectedKey) throw Error("Work changed before creation promotion");
    const recipe = promoteCreationRecipe(work, request.proposal, request),
      evidence = await store.saveArtifact("creation-recipe.json", recipe);
    const entry: ToolkitEntry = {
      version: 1,
      id: recipe.id,
      revision: recipe.revision,
      title: recipe.id,
      description: recipe.description,
      tags: ["creation", ...recipe.capabilities.map((c) => c.id)],
      level: "recipe",
      status: "experimental",
      implementation: { kind: "creation", recipe },
      controls: [],
      preserves: ["Internal document relationships", "Part identities", "Pinned recipe version"],
      limitations: ["New instantiations require source and visual review"],
      previews: [],
      engineeringMs: null,
      trials: [],
    };
    const saved = await write(entry, null),
      entryEvidence = await store.saveArtifact("toolkit-entry.json", saved.entry);
    const next = appendWorkEvent(work, {
      id: `creation-memory-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      kind: "handoff",
      actor: "author",
      summary: `Retained creation recipe ${entry.id}@${entry.revision}`,
      evidence: [evidence, entryEvidence],
    });
    return { ...saved, evidence, workKey: await store.put(next, contentKey(work)) };
  }
  const entry = await read(request.id, request.revision);
  if (request.action === "instantiate") {
    if (entry.implementation.kind !== "creation") throw Error("Choose a creation recipe");
    return instantiateCreationRecipe(work.baseline, entry.implementation.recipe, request.namespace);
  }
  if (contentKey(entry) !== request.expectedKey) throw Error("Toolkit entry changed; search again");
  if (request.action === "promote")
    return write(promoteToolkitEntry(entry, request.status), request.expectedKey);
  const p = work.proposals.find((p) => p.id === request.proposal);
  if (!p) throw Error("Trial candidate missing");
  const used = appendWorkEvent(work, {
    id: `toolkit-use-${crypto.randomUUID()}`,
    at: new Date().toISOString(),
    kind: "tool-development",
    actor: "author",
    summary: `${entry.id}@${entry.revision}`,
    evidence: [p.candidateKey],
    durationMs: request.elapsedMs,
  });
  const next = recordToolkitTrial(entry, {
    work: used,
    proposal: request.proposal,
    family: request.family,
    elapsedMs: request.elapsedMs,
  });
  const key = await store.put(used, contentKey(work));
  return { ...(await write(next, request.expectedKey)), workKey: key };
}
