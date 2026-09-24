import { AuthoringSession, type EditBatch, parseEditBatch } from "@wrela/authoring";
import { contentKey, documentClosure, type Project, references, WORKING_SET_LIMITS } from "@wrela/model";

export class WorkspaceConflict extends Error {
  readonly code = "authoring.conflict";
  readonly nextAction =
    "Inspect conflicting documents and prepare a new proposal; retain the original transaction ID only for an identical retry.";
  constructor(
    message: string,
    readonly conflicts: { document?: string; expected: string | null; actual: string | null }[] = [],
  ) {
    super(message);
  }
}
/** Whole-project identity records provenance. Only the authored read/write closure controls
 * rebasing. Validation against the latest project still enforces global relationships. */
export function prepareWorkspaceTransaction(baseline: Project, current: Project, input: EditBatch) {
  const batch = parseEditBatch(input);
  const originalIds = new Set(baseline.documents.map((d) => d.id));
  const roots = new Set(
    [
      ...Object.keys(batch.preconditions?.reads ?? {}),
      ...Object.keys(batch.preconditions?.writes ?? {}),
      ...Object.keys(batch.expectedDocuments ?? {}),
    ].filter((id) => originalIds.has(id)),
  );
  for (const operation of batch.operations) {
    if (operation.kind === "document.create") {
      for (const ref of references(operation.document)) if (originalIds.has(ref)) roots.add(ref);
    } else {
      if (originalIds.has(operation.target)) roots.add(operation.target);
      if (operation.kind === "material.assign") roots.add(operation.material);
    }
  }
  // Include newly selected references (including document.set's nested values)
  // before candidate validation, without pulling in the unrelated world entry.
  const includeReferences = (value: unknown): void => {
    if (typeof value === "string" && originalIds.has(value)) roots.add(value);
    else if (value && typeof value === "object")
      for (const child of Object.values(value)) includeReferences(child);
  };
  for (const operation of batch.operations) includeReferences(operation);
  const deleted = new Set(batch.operations.flatMap((o) => (o.kind === "document.delete" ? [o.target] : [])));
  let entry = [...roots].find((id) => !deleted.has(id));
  if (!entry) {
    entry = baseline.documents.find((d) => !deleted.has(d.id))?.id;
    if (!entry) throw Error("A project must retain an entry definition");
    roots.add(entry);
  }
  const closure = documentClosure(
    baseline.documents.map((d) => ({ id: d.id, references: references(d) })),
    [...roots],
  );
  const working = (project: Project) => {
    const documents = project.documents.filter((d) => closure.has(d.id));
    if (
      documents.length > WORKING_SET_LIMITS.documents ||
      new TextEncoder().encode(JSON.stringify(documents)).byteLength > WORKING_SET_LIMITS.sourceBytes
    )
      throw Error("Task dependency closure exceeds working set budget");
    return {
      ...project,
      entry,
      dynamicRoots: undefined,
      recipes: undefined,
      recipeInstances: undefined,
      documents,
    };
  };
  const original = new AuthoringSession(working(baseline)).preview(batch);
  const before = new Map(baseline.documents.map((d) => [d.id, d]));
  const latest = new Map(current.documents.map((d) => [d.id, d]));
  const candidates = new Map(before);
  for (const change of original.changes) {
    if (change.after) candidates.set(change.id, change.after);
    else candidates.delete(change.id);
  }
  const writes = new Set(
    batch.operations.flatMap((o) =>
      o.kind === "document.create"
        ? [o.document.id]
        : o.kind === "material.makeLocal"
          ? [o.target, o.newId]
          : [o.target],
    ),
  );
  const reads = new Set(Object.keys(batch.preconditions?.reads ?? {}));
  const visit = (id: string) => {
    if (reads.has(id)) return;
    reads.add(id);
    for (const doc of [before.get(id), candidates.get(id)])
      if (doc) for (const ref of references(doc)) visit(ref);
  };
  // Include explicitly declared reads and transitively referenced source on either side.
  const declared = [...reads];
  reads.clear();
  for (const id of [...writes, ...declared]) visit(id);
  const conflicts = [...reads].flatMap((id) => {
    const expected = before.has(id) ? contentKey(before.get(id)) : null,
      actual = latest.has(id) ? contentKey(latest.get(id)) : null;
    return expected === actual ? [] : [{ document: id, expected, actual }];
  });
  const projectKey = (project: Project) => contentKey({ ...project, documents: [] });
  if (projectKey(baseline) !== projectKey(current))
    conflicts.push({ document: "$project", expected: projectKey(baseline), actual: projectKey(current) });
  if (conflicts.length)
    throw new WorkspaceConflict(
      "Workspace documents changed; inspect conflicts before publishing",
      conflicts,
    );
  const revision = (id: string) => (latest.has(id) ? 0 : null);
  const session = new AuthoringSession(working(current));
  const result = session.apply({
    ...batch,
    expectedRevision: undefined,
    expectedDocuments: undefined,
    preconditions: {
      reads: Object.fromEntries([...reads].filter((id) => !writes.has(id)).map((id) => [id, revision(id)])),
      writes: Object.fromEntries([...writes].map((id) => [id, revision(id)])),
    },
  });
  const edited = new Map(session.getSnapshot().project.documents.map((d) => [d.id, d]));
  const documents = current.documents.flatMap((d) =>
    writes.has(d.id) ? (edited.has(d.id) ? [edited.get(d.id)!] : []) : [d],
  );
  for (const id of writes) if (!latest.has(id) && edited.has(id)) documents.push(edited.get(id)!);
  const project = { ...current, documents };
  return { project, result: { ...result, key: contentKey(project) } };
}
