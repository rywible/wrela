import { type Document, documentSchema, type Project } from "./documents";
import { contentKey } from "./math";
import { parseProject, references } from "./validation";

export const WORKING_SET_LIMITS = { documents: 256, sourceBytes: 16 * 1024 * 1024 } as const;
export type DocumentDescriptor = {
  id: string;
  name: string;
  kind: Document["kind"];
  key: string;
  references: string[];
  bytes: number;
};
export interface DocumentRepository {
  readonly key: string;
  readonly project: Omit<Project, "documents">;
  readonly index: readonly DocumentDescriptor[];
  read(id: string): Promise<Document>;
}
export function describeDocument(document: Document): DocumentDescriptor {
  return {
    id: document.id,
    name: document.name,
    kind: document.kind,
    key: contentKey(document),
    references: references(document),
    bytes: new TextEncoder().encode(JSON.stringify(document)).byteLength,
  };
}
export function searchDocuments(
  repository: Pick<DocumentRepository, "index">,
  input: { search?: string; kind?: Document["kind"]; offset?: number; limit?: number } = {},
) {
  const offset = input.offset ?? 0,
    limit = input.limit ?? 12;
  if (!Number.isInteger(offset) || offset < 0 || !Number.isInteger(limit) || limit < 1 || limit > 64)
    throw Error("Catalog pages require offset >= 0 and limit 1–64");
  const terms = (input.search ?? "").toLowerCase().split(/\s+/).filter(Boolean);
  const matches = repository.index.filter(
    (d) =>
      (!input.kind || d.kind === input.kind) &&
      terms.every((term) => `${d.id} ${d.name} ${d.kind}`.toLowerCase().includes(term)),
  );
  return {
    entries: matches.slice(offset, offset + limit),
    total: matches.length,
    nextOffset: offset + limit < matches.length ? offset + limit : null,
  };
}
export function documentClosure(
  index: readonly Pick<DocumentDescriptor, "id" | "references">[],
  roots: readonly string[],
) {
  const byId = new Map(index.map((d) => [d.id, d])),
    selected = new Set<string>();
  const visit = (id: string) => {
    if (selected.has(id)) return;
    const doc = byId.get(id);
    if (!doc) throw Error(`Missing definition ${id}`);
    selected.add(id);
    for (const ref of doc.references) visit(ref);
  };
  for (const root of roots) visit(root);
  return selected;
}
export async function loadWorkingSet(
  repository: DocumentRepository,
  roots: string[],
  limits: { documents: number; sourceBytes: number } = WORKING_SET_LIMITS,
): Promise<Project> {
  if (!roots.length) throw Error("A working set needs explicit roots");
  const selected = documentClosure(repository.index, roots),
    descriptors = repository.index.filter((d) => selected.has(d.id));
  if (selected.size > limits.documents || descriptors.reduce((n, d) => n + d.bytes, 0) > limits.sourceBytes)
    throw Error("Task dependency closure exceeds working set budget; split the task or scene");
  const documents = await Promise.all(
    descriptors.map(async (descriptor) => {
      const document = documentSchema.parse(await repository.read(descriptor.id));
      if (contentKey(document) !== descriptor.key)
        throw Error(`Definition ${descriptor.id} changed; refresh the catalog`);
      return document;
    }),
  );
  return parseProject({
    ...repository.project,
    entry: roots[0],
    dynamicRoots: undefined,
    recipes: undefined,
    recipeInstances: undefined,
    documents,
  });
}
export function releaseProject(input: Project): Project {
  const project = parseProject(input),
    selected = documentClosure(
      project.documents.map((d) => ({ id: d.id, references: references(d) })),
      [project.entry, ...(project.dynamicRoots ?? [])],
    );
  // Recipe libraries are authoring state; realized documents retain their generated provenance.
  return {
    ...project,
    recipes: undefined,
    recipeInstances: undefined,
    documents: project.documents.filter((d) => selected.has(d.id)),
  };
}
export function memoryDocumentRepository(input: Project): DocumentRepository {
  const project = parseProject(input),
    documents = new Map(project.documents.map((d) => [d.id, d]));
  const { documents: _documents, ...metadata } = project;
  return {
    key: contentKey(project),
    project: metadata,
    index: project.documents.map(describeDocument),
    async read(id) {
      const doc = documents.get(id);
      if (!doc) throw Error(`Unknown definition ${id}`);
      return structuredClone(doc);
    },
  };
}
