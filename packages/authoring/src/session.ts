import {
  canonical,
  contentKey,
  contentKeyFromCanonical,
  type Diagnostic,
  type Document,
  documentSchema,
  idSchema,
  type Project,
  parseProject,
  references,
  validateProjectRelationships,
} from "@wrela/model";
import { z } from "zod";
import {
  batchSchema,
  commandDescriptions,
  type EditBatch,
  type Operation,
  operationSchema,
} from "./commands";
export type TransactionConflict = {
  document?: string;
  reason: "revision" | "missing-read" | "missing-write" | "transaction-id" | "revert";
  expected?: number | null;
  actual?: number | null;
};
export class RevisionConflict extends Error {
  readonly code = "authoring.conflict";
  constructor(
    message = "The project changed. Inspect the current revision and retry.",
    public readonly conflicts: TransactionConflict[] = [],
  ) {
    super(message);
    this.name = "RevisionConflict";
  }
}
export type DocumentChange = {
  id: string;
  before?: Document;
  after?: Document;
  beforeIndex: number;
  afterIndex: number;
};
export type ChangeSummary = {
  id: string;
  action: "create" | "update" | "delete";
  properties: string[];
};
export type TransactionResult = {
  revision: number;
  changed: string[];
  key: string;
  transactionId: string;
  diff: ChangeSummary[];
};
export type TransactionRecord = {
  id: string;
  revision: number;
  actor?: string;
  intent?: string;
  label: string;
  changes: DocumentChange[];
  diff: ChangeSummary[];
  beforeOrder: string[];
  afterOrder: string[];
  reverts?: string;
};
export type AuthoringOptions = {
  historyLimit?: number;
  historyBytes?: number;
  /** Receipts are compact; expired IDs remain reserved for this session. */
  receiptLimit?: number;
};
export type SessionSnapshot = {
  project: Project;
  revision: number;
  savedRevision: number;
  documentRevisions: Record<string, number>;
  selection: string;
  canUndo: boolean;
  canRedo: boolean;
  historyLabel: string;
  diagnostics: Diagnostic[];
};
type History = {
  changes: DocumentChange[];
  label: string;
  gesture?: string;
  bytes: number;
  beforeOrder: string[];
  afterOrder: string[];
};
const revertOptionsSchema = z.object({
  transactionId: idSchema.optional(),
  actor: z.string().min(1).max(120).optional(),
  intent: z.string().min(1).max(1000).optional(),
});
const clone = <T>(v: T): T => structuredClone(v);
function freeze<T>(value: T): T {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const child of Object.values(value)) freeze(child);
  }
  return value;
}
function targetOf(op: Operation): string {
  return op.kind === "document.create" ? op.document.id : op.target;
}
/** Optional source properties need an explicit domain path; absent arbitrary
 * object keys remain rejected before schema parsing could silently strip them. */
function optionalProperty(document: Document, path: (string | number)[]): boolean {
  if (path.length === 1) {
    if (document.kind === "world" && path[0] === "water") return true;
    if (document.kind === "material" && ["domain", "layers"].includes(String(path[0]))) return true;
    if (document.kind === "object" && path[0] === "colliders") return true;
  }
  if (document.kind === "character" && path.length === 2 && path[0] === "physics" && path[1] === "colliders")
    return true;
  if ("field" in document && path[0] === "field") {
    if (path[1] === "fidelity")
      return (
        path.length === 2 ||
        (path.length === 3 && ["maxError", "minimumFeatureSize", "strict"].includes(String(path[2])))
      );
    if (path.length === 4 && path[1] === "nodes" && typeof path[2] === "number" && path[3] === "material")
      return true;
  }
  return false;
}
function applyOperation(project: Project, op: Operation, reads?: Set<string>) {
  if (op.kind === "document.create") {
    if (project.documents.some((d) => d.id === op.document.id))
      throw Error("A definition with that identity already exists");
    project.documents.push(op.document);
    return;
  }
  const doc = project.documents.find((d) => d.id === op.target);
  if (!doc) throw Error(`Definition ${op.target} does not exist`);
  if (doc.generated?.policy === "locked" && op.kind !== "document.detach")
    throw Error("Detach the generated definition before editing it");
  const previousTyped = new Set(references({ ...doc, dependencies: [] }));
  switch (op.kind) {
    case "document.detach":
      if (doc.generated) doc.generated.policy = "detached";
      break;
    case "document.delete":
      if (project.entry === doc.id) throw Error("The project entry cannot be deleted");
      project.documents = project.documents.filter((d) => d.id !== doc.id);
      break;
    case "document.rename":
      doc.name = op.name;
      break;
    case "document.set": {
      if (["id", "kind", "schemaVersion", "generated", "dependencies"].includes(String(op.path[0])))
        throw Error("Identity and source policy require semantic operations");
      let cursor: any = doc;
      for (const p of op.path.slice(0, -1)) {
        if (["__proto__", "prototype", "constructor"].includes(String(p)) || !Object.hasOwn(cursor, p))
          throw Error("Unknown property path");
        cursor = cursor[p];
        if (cursor === null || typeof cursor !== "object") throw Error("Invalid property path");
      }
      const last = op.path[op.path.length - 1];
      if (last === "id") throw Error("Stable identities cannot be rewritten through property edits");
      const optional = optionalProperty(doc, op.path);
      if (
        ["__proto__", "prototype", "constructor"].includes(String(last)) ||
        (!Object.hasOwn(cursor, last) && !optional)
      )
        throw Error("Unknown property");
      cursor[last] = clone(op.value);
      break;
    }
    case "field.update": {
      if (!("field" in doc)) throw Error("Select a field definition");
      const node = doc.field.nodes.find((n) => n.id === op.node);
      if (!node) throw Error("Shape does not exist");
      Object.assign(node, op.changes);
      break;
    }
    case "field.add": {
      if (!("field" in doc)) throw Error("Select a field definition");
      if (doc.field.nodes.some((n) => n.id === op.node.id)) throw Error("Shape identity already exists");
      const root = doc.field.nodes.find((n) => n.id === doc.field.root);
      if (!root) throw new Error("Field root is missing");
      doc.field.nodes.push(op.node);
      if (["union", "smoothUnion"].includes(root.kind)) root.children.push(op.node.id);
      else {
        const base = `composition-${contentKey([doc.id, op.node.id, root.id]).slice(0, 12)}`;
        let id = base,
          suffix = 0;
        while (doc.field.nodes.some((node) => node.id === id)) id = `${base}-${++suffix}`;
        doc.field.nodes.push({
          ...clone(root),
          id,
          name: "Composition",
          kind: "smoothUnion",
          children: [root.id, op.node.id],
          position: [0, 0, 0],
          rotation: [0, 0, 0],
          size: [1, 1, 1],
          material: undefined,
        });
        doc.field.root = id;
      }
      break;
    }
    case "field.remove": {
      if (!("field" in doc)) throw Error("Select a field definition");
      if (doc.field.root === op.node) throw Error("The root shape cannot be deleted");
      if (!doc.field.nodes.some((node) => node.id === op.node)) throw Error("Shape does not exist");
      const removed = new Set([op.node]);
      let changed = true;
      while (changed) {
        changed = false;
        for (const node of doc.field.nodes) {
          node.children = node.children.filter((child) => !removed.has(child));
          if (
            ["union", "subtract", "intersect", "smoothUnion"].includes(node.kind) &&
            node.children.length === 0 &&
            !removed.has(node.id)
          ) {
            removed.add(node.id);
            changed = true;
          }
        }
      }
      if (removed.has(doc.field.root)) throw Error("A field must retain at least one shape");
      const remaining = new Map(
        doc.field.nodes.filter((node) => !removed.has(node.id)).map((node) => [node.id, node]),
      );
      const reachable = new Set<string>();
      const visit = (id: string) => {
        if (reachable.has(id)) return;
        reachable.add(id);
        for (const child of remaining.get(id)?.children ?? []) visit(child);
      };
      visit(doc.field.root);
      // Unary compositions retain their transform and material. Collapsing them
      // would move a surviving shape or alter its material, especially in DAGs.
      doc.field.nodes = doc.field.nodes.filter((node) => reachable.has(node.id));
      break;
    }
    case "material.assign":
      if (!("material" in doc)) throw Error("This definition has no surface material");
      doc.material = op.material;
      break;
    case "material.makeLocal": {
      if (!("material" in doc)) throw Error("This definition has no surface material");
      reads?.add(doc.material);
      const material = project.documents.find((d) => d.id === doc.material);
      if (!material || material.kind !== "material") throw Error("Material is missing");
      if (project.documents.some((d) => d.id === op.newId)) throw Error("Variant identity already exists");
      const variant = {
        ...clone(material),
        id: op.newId,
        name: `${material.name} · ${doc.name}`.slice(0, 120),
      };
      if (variant.generated) variant.generated.policy = "detached";
      project.documents.push(variant);
      doc.material = op.newId;
      break;
    }
    case "terrain.intervene":
      if (doc.kind !== "terrain") throw Error("Select terrain");
      {
        const index = doc.interventions.findIndex((i) => i.id === op.intervention.id);
        if (index < 0) doc.interventions.push(op.intervention);
        else doc.interventions[index] = op.intervention;
      }
      break;
    case "terrain.widenValley":
      if (doc.kind !== "terrain") throw Error("Select terrain");
      {
        const i = doc.interventions.find((i) => i.id === op.intervention);
        if (!i || i.kind !== "valley") throw Error("Valley intervention is missing");
        i.radius = op.width / 2;
      }
      break;
    case "world.placeForest":
      if (doc.kind !== "world") throw Error("Select a world");
      {
        const rule = {
          id: op.rule,
          definition: op.definition,
          spacing: op.spacing,
          density: op.density,
          seed: op.seed,
          minHeight: -0.4,
          maxHeight: 100,
          maxSlope: 0.8,
        };
        const index = doc.populations.findIndex((r) => r.id === op.rule);
        if (index < 0) doc.populations.push(rule);
        else doc.populations[index] = rule;
      }
      break;
    case "character.addJoint":
      if (doc.kind !== "character") throw Error("Select a character");
      doc.joints.push(op.joint);
      break;
    case "character.setJointLimit":
    case "character.pose": {
      if (doc.kind !== "character") throw Error("Select a character");
      const joint = doc.joints.find((j) => j.id === op.joint);
      if (!joint) throw Error("Joint is missing");
      if (op.kind === "character.pose") joint.rotation = op.rotation;
      else {
        joint.minimum = op.minimum;
        joint.maximum = op.maximum;
      }
      break;
    }
    case "character.addKey": {
      if (doc.kind !== "character") throw Error("Select a character");
      const motion = doc.motions.find((m) => m.id === op.motion);
      if (!motion) throw Error("Motion is missing");
      motion.keys = motion.keys.filter((k) => !(k.joint === op.joint && k.time === op.time));
      motion.keys.push({
        joint: op.joint,
        time: op.time,
        rotation: op.rotation,
        translation: op.translation,
      });
      motion.keys.sort((a, b) => a.time - b.time || a.joint.localeCompare(b.joint));
      break;
    }
  }
  // Preserve additional declared dependencies while removing obsolete copies
  // of references owned by a typed property (e.g. a replaced material).
  const nextTyped = new Set(references({ ...doc, dependencies: [] }));
  doc.dependencies = doc.dependencies.filter((id) => !previousTyped.has(id) || nextTyped.has(id));
}
/** One synchronous authority shared by human controls and agent clients.
 * Unchanged definitions retain identity; history only retains changed definitions. */
export class AuthoringSession {
  private state: SessionSnapshot;
  private listeners = new Set<() => void>();
  private undoStack: History[] = [];
  private redoStack: History[] = [];
  private closedGestures = new Set<string>();
  private documents = new Map<string, Document>();
  private serialized = new WeakMap<Document, string>();
  private records: TransactionRecord[] = [];
  private receipts = new Map<string, { fingerprint: string; result: TransactionResult }>();
  private expiredIds = new Set<string>();
  private readonly limits: Required<AuthoringOptions>;
  constructor(project: Project, options: AuthoringOptions = {}) {
    const parsed = parseProject(project);
    this.limits = {
      historyLimit: options.historyLimit ?? 100,
      historyBytes: options.historyBytes ?? 16 * 1024 * 1024,
      receiptLimit: options.receiptLimit ?? 256,
    };
    for (const value of Object.values(this.limits))
      if (!Number.isSafeInteger(value) || value < 1)
        throw Error("Authoring limits must be positive integers");
    this.state = freeze({
      project: parsed,
      revision: 0,
      savedRevision: -1,
      documentRevisions: Object.fromEntries(parsed.documents.map((d) => [d.id, 0])),
      selection: parsed.entry,
      canUndo: false,
      canRedo: false,
      historyLabel: "",
      diagnostics: [],
    });
    this.index(parsed);
  }
  private index(project: Project) {
    this.documents = new Map(project.documents.map((document) => [document.id, document]));
  }
  private source(document: Document) {
    let value = this.serialized.get(document);
    if (value === undefined) {
      value = canonical(document);
      this.serialized.set(document, value);
    }
    return value;
  }
  private key(project: Project) {
    const encoded = Object.keys(project)
      .sort()
      .map((key) => {
        const value =
          key === "documents"
            ? `[${project.documents.map((document) => this.source(document)).join(",")}]`
            : canonical(project[key as keyof Project]);
        return `${JSON.stringify(key)}:${value}`;
      })
      .join(",");
    return contentKeyFromCanonical(`{${encoded}}`);
  }
  getSnapshot = () => this.state;
  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };
  private emit() {
    freeze(this.state);
    for (const fn of this.listeners) {
      try {
        fn();
      } catch (error) {
        this.state = freeze({
          ...this.state,
          diagnostics: [
            ...this.state.diagnostics.slice(-31),
            {
              severity: "warning" as const,
              code: "authoring.observer",
              message: `An authoring observer failed: ${error instanceof Error ? error.message : String(error)}`,
            },
          ],
        });
      }
    }
  }
  select(id: string) {
    if (!this.documents.has(id)) return;
    this.state = { ...this.state, selection: id };
    this.emit();
  }
  inspect(id?: string) {
    return clone(id ? this.documents.get(id) : this.state);
  }
  transactions() {
    return clone(this.records);
  }
  inspectTransaction(id: string) {
    return clone(this.records.find((record) => record.id === id));
  }
  historyUsage() {
    return {
      undoEntries: this.undoStack.length,
      redoEntries: this.redoStack.length,
      retainedChangeBytes: this.retainedBytes(),
      undoRedoChangeBytes: [...this.undoStack, ...this.redoStack].reduce((sum, item) => sum + item.bytes, 0),
      transactionChangeBytes: this.records.reduce((sum, record) => sum + this.bytes(record.changes), 0),
      transactionEntries: this.records.length,
      ...this.limits,
    };
  }
  discover() {
    return {
      revision: this.state.revision,
      operations: Object.entries(commandDescriptions).map(([kind, description]) => ({ kind, description })),
      schema: z.toJSONSchema(operationSchema),
      batchSchema: z.toJSONSchema(batchSchema),
      transactions: {
        preview: "preview(batch) validates and returns proposed document changes without committing",
        inspect: "inspectTransaction(id) and transactions() expose retained changes, actor and intent",
        revert:
          "revert(id, { transactionId?, actor?, intent? }) reverses one retained transaction if its documents are unchanged",
        preconditions:
          "Supply expectedRevision, or preconditions.reads/writes mapping IDs to revisions (null means absent)",
        retries:
          "transactionId is reserved for identical retries; expired receipts reject rather than reapply",
      },
      limits: { operationsPerBatch: 256, definitions: 256, fieldNodes: 128, fieldDepth: 32, ...this.limits },
      conventions: { length: "metres", time: "seconds", angle: "radians", up: "+Y", handedness: "right" },
    };
  }
  private revision(id: string) {
    return Object.hasOwn(this.state.documentRevisions, id) ? this.state.documentRevisions[id] : null;
  }
  private writes(batch: EditBatch) {
    return new Set(
      batch.operations.flatMap((operation) =>
        operation.kind === "material.makeLocal" ? [operation.target, operation.newId] : [targetOf(operation)],
      ),
    );
  }
  private checkPreconditions(batch: EditBatch, writes: Set<string>) {
    const conflicts: TransactionConflict[] = [];
    if (batch.expectedRevision !== undefined && batch.expectedRevision !== this.state.revision)
      conflicts.push({ reason: "revision", expected: batch.expectedRevision, actual: this.state.revision });
    if (batch.expectedDocuments)
      for (const id of new Set(batch.operations.map(targetOf)))
        if (this.documents.has(id) && batch.expectedDocuments[id] !== this.revision(id))
          conflicts.push({
            document: id,
            reason: "revision",
            expected: batch.expectedDocuments[id],
            actual: this.revision(id),
          });
    if (batch.preconditions) {
      for (const id of writes)
        if (!Object.hasOwn(batch.preconditions.writes, id))
          conflicts.push({ document: id, reason: "missing-write", actual: this.revision(id) });
      for (const entries of [batch.preconditions.reads, batch.preconditions.writes])
        for (const [id, expected] of Object.entries(entries))
          if (expected !== this.revision(id))
            conflicts.push({ document: id, reason: "revision", expected, actual: this.revision(id) });
    }
    if (conflicts.length)
      throw new RevisionConflict("Transaction preconditions no longer match source", conflicts);
  }
  private changes(before: Project, after: Project, ids: Iterable<string>): DocumentChange[] {
    const result: DocumentChange[] = [];
    for (const id of ids) {
      const beforeIndex = before.documents.findIndex((document) => document.id === id),
        afterIndex = after.documents.findIndex((document) => document.id === id),
        previous = before.documents[beforeIndex],
        next = after.documents[afterIndex];
      if (
        beforeIndex === afterIndex &&
        (previous === next || (previous && next && this.source(previous) === this.source(next)))
      )
        continue;
      result.push({ id, before: previous, after: next, beforeIndex, afterIndex });
    }
    return result;
  }
  private summary(changes: DocumentChange[]): ChangeSummary[] {
    return changes.map(({ id, before, after }) => ({
      id,
      action: !before ? "create" : !after ? "delete" : "update",
      properties:
        !before || !after
          ? []
          : [...new Set([...Object.keys(before), ...Object.keys(after)])].filter(
              (key) => canonical((before as any)[key]) !== canonical((after as any)[key]),
            ),
    }));
  }
  private validateCandidate(candidate: Project, writes: Set<string>) {
    if (candidate.documents.length < 1 || candidate.documents.length > 256)
      throw Error("A project must contain 1–256 definitions");
    candidate.documents = candidate.documents.map((document) => {
      if (!writes.has(document.id)) return document;
      const parsed = documentSchema.parse(document),
        previous = this.documents.get(document.id);
      return previous && this.source(previous) === this.source(parsed) ? previous : parsed;
    });
    const diagnostics = validateProjectRelationships(candidate, writes);
    if (diagnostics.some((diagnostic) => diagnostic.severity === "error"))
      throw Error(diagnostics.map((diagnostic) => diagnostic.message).join("\n"));
    return candidate;
  }
  private prepare(batch: EditBatch) {
    const writes = this.writes(batch);
    this.checkPreconditions(batch, writes);
    const before = this.state.project;
    const candidate = {
      ...before,
      documents: before.documents.map((document) => (writes.has(document.id) ? clone(document) : document)),
    };
    const observedReads = new Set<string>();
    for (const operation of batch.operations) applyOperation(candidate, operation, observedReads);
    const after = this.validateCandidate(candidate, writes);
    if (batch.preconditions) {
      const readIds = new Set([...observedReads].filter((id) => !writes.has(id)));
      for (const id of writes) {
        const old = this.documents.get(id),
          next = after.documents.find((document) => document.id === id);
        for (const document of [old, next])
          if (document) for (const ref of references(document)) if (!writes.has(ref)) readIds.add(ref);
      }
      const conflicts: TransactionConflict[] = [];
      for (const id of readIds)
        if (!Object.hasOwn(batch.preconditions.reads, id))
          conflicts.push({ document: id, reason: "missing-read", actual: this.revision(id) });
      if (conflicts.length)
        throw new RevisionConflict("Declare the referenced definitions read by this proposal", conflicts);
    }
    const changes = this.changes(before, after, writes);
    return { after, changes, diff: this.summary(changes) };
  }
  preview(input: EditBatch) {
    const batch = batchSchema.parse(input),
      prepared = this.prepare(batch);
    return clone({ revision: this.state.revision, changes: prepared.changes, diff: prepared.diff });
  }
  private retry(id: string | undefined, fingerprint: string): TransactionResult | undefined {
    if (!id) return;
    const receipt = this.receipts.get(id);
    if (receipt) {
      if (receipt.fingerprint !== fingerprint)
        throw new RevisionConflict("A transaction ID cannot be reused for different work", [
          { reason: "transaction-id" },
        ]);
      return clone(receipt.result);
    }
    if (this.expiredIds.has(id))
      throw new RevisionConflict(
        "The transaction receipt expired; inspect current source before proposing new work",
        [{ reason: "transaction-id" }],
      );
  }
  private remember(id: string, fingerprint: string, result: TransactionResult) {
    this.receipts.set(id, { fingerprint, result: clone(result) });
    while (this.receipts.size > this.limits.receiptLimit) {
      const oldest = this.receipts.keys().next().value;
      if (oldest === undefined) break;
      this.receipts.delete(oldest);
      this.expiredIds.add(oldest);
    }
  }
  private bytes(changes: DocumentChange[]) {
    return changes.reduce(
      (size, change) =>
        size +
        (change.before ? this.source(change.before).length * 2 : 0) +
        (change.after ? this.source(change.after).length * 2 : 0),
      0,
    );
  }
  private retainedBytes() {
    return (
      [...this.undoStack, ...this.redoStack].reduce((sum, item) => sum + item.bytes, 0) +
      this.records.reduce((sum, record) => sum + this.bytes(record.changes), 0)
    );
  }
  private trimHistory() {
    while (this.undoStack.length + this.redoStack.length > this.limits.historyLimit) {
      if (this.undoStack.length) this.undoStack.shift();
      else this.redoStack.shift();
    }
    while (this.records.length > this.limits.historyLimit) this.records.shift();
    // Conservatively count versions present in both undo and review history twice.
    while (this.retainedBytes() > this.limits.historyBytes) {
      if (this.records.length) this.records.shift();
      else if (this.undoStack.length) this.undoStack.shift();
      else if (this.redoStack.length) this.redoStack.shift();
      else break;
    }
  }
  private pushHistory(changes: DocumentChange[], label: string, after: Project, gesture?: string) {
    const previous = this.undoStack.at(-1),
      afterOrder = after.documents.map((document) => document.id);
    if (
      gesture &&
      previous?.gesture === gesture &&
      !this.closedGestures.has(gesture) &&
      this.redoStack.length === 0
    ) {
      const merged = new Map(previous.changes.map((change) => [change.id, change]));
      for (const change of changes) {
        const old = merged.get(change.id);
        merged.set(change.id, old ? { ...change, before: old.before } : change);
      }
      previous.changes = [...merged.values()].map((change) => ({
        ...change,
        beforeIndex: previous.beforeOrder.indexOf(change.id),
        afterIndex: afterOrder.indexOf(change.id),
      }));
      previous.afterOrder = afterOrder;
      previous.bytes = this.bytes(previous.changes);
    } else
      this.undoStack.push({
        changes,
        label,
        gesture,
        bytes: this.bytes(changes),
        beforeOrder: this.state.project.documents.map((document) => document.id),
        afterOrder,
      });
    this.redoStack = [];
  }
  apply(input: EditBatch): TransactionResult {
    const batch = batchSchema.parse(input),
      fingerprint = contentKey(batch),
      retry = this.retry(batch.transactionId, fingerprint);
    if (retry) return retry;
    const prepared = this.prepare(batch),
      id = batch.transactionId ?? crypto.randomUUID(),
      label = batch.label ?? commandDescriptions[batch.operations[0].kind];
    if (prepared.changes.length) {
      this.pushHistory(prepared.changes, label, prepared.after, batch.gesture);
      this.records.push(
        freeze({
          id,
          revision: this.state.revision + 1,
          actor: batch.actor,
          intent: batch.intent,
          label,
          changes: prepared.changes,
          diff: prepared.diff,
          beforeOrder: this.state.project.documents.map((document) => document.id),
          afterOrder: prepared.after.documents.map((document) => document.id),
        }),
      );
      this.trimHistory();
      this.install(prepared.after, prepared.changes, label);
    }
    const result = {
      revision: this.state.revision,
      changed: prepared.changes.map((change) => change.id),
      key: this.key(this.state.project),
      transactionId: id,
      diff: prepared.diff,
    };
    this.remember(id, fingerprint, result);
    if (prepared.changes.length) this.emit();
    return clone(result);
  }
  endGesture(id: string) {
    this.closedGestures.add(id);
    if (this.closedGestures.size > 256) {
      const oldest = this.closedGestures.values().next().value;
      if (oldest !== undefined) this.closedGestures.delete(oldest);
    }
  }
  private install(project: Project, changes: DocumentChange[], label: string) {
    const revision = this.state.revision + 1,
      documentRevisions: Record<string, number> = Object.assign(
        Object.create(null),
        this.state.documentRevisions,
      );
    for (const change of changes) {
      if (change.after) documentRevisions[change.id] = revision;
      else delete documentRevisions[change.id];
    }
    this.index(project);
    this.state = {
      ...this.state,
      project,
      revision,
      documentRevisions,
      selection: this.documents.has(this.state.selection) ? this.state.selection : project.entry,
      canUndo: this.undoStack.length > 0,
      canRedo: this.redoStack.length > 0,
      historyLabel: label,
    };
    freeze(this.state);
  }
  private restore(
    changes: DocumentChange[],
    direction: "before" | "after",
    order: string[],
    targeted = false,
  ) {
    const updates = new Map(changes.map((change) => [change.id, change]));
    let documents = this.state.project.documents.flatMap((current) => {
      const change = updates.get(current.id);
      if (!change) return [current];
      const desired = change[direction];
      return desired ? [desired] : [];
    });
    const insertions = changes.filter(
      (change) =>
        change[direction] &&
        (!this.documents.has(change.id) || (targeted && change.beforeIndex !== change.afterIndex)),
    );
    for (const change of insertions.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id))) {
      documents = documents.filter((document) => document.id !== change.id);
      const desired = change[direction];
      if (!desired) continue;
      const rank = order.indexOf(change.id);
      const next = order.slice(rank + 1).find((id) => documents.some((document) => document.id === id));
      const previous = order
        .slice(0, rank)
        .reverse()
        .find((id) => documents.some((document) => document.id === id));
      const index = next
        ? documents.findIndex((document) => document.id === next)
        : previous
          ? documents.findIndex((document) => document.id === previous) + 1
          : Math.min(Math.max(rank, 0), documents.length);
      documents.splice(index, 0, desired);
    }
    if (!targeted) documents.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
    if (documents.length < 1 || documents.length > 256)
      throw new RevisionConflict("Reverting would exceed the 1–256 definition project limit", [
        { reason: "revert" },
      ]);
    const ids = new Set(changes.map((change) => change.id)),
      project = { ...this.state.project, documents },
      diagnostics = validateProjectRelationships(project, ids);
    if (diagnostics.some((diagnostic) => diagnostic.severity === "error"))
      throw new RevisionConflict(
        `Reverting would invalidate source: ${diagnostics.map((d) => d.message).join("; ")}`,
        [{ reason: "revert" }],
      );
    return { project, changes: this.changes(this.state.project, project, ids) };
  }
  undo() {
    const history = this.undoStack.at(-1);
    if (!history) return;
    const restored = this.restore(history.changes, "before", history.beforeOrder);
    this.undoStack.pop();
    if (history.gesture) this.endGesture(history.gesture);
    this.redoStack.push(history);
    this.install(restored.project, restored.changes, `Undo ${history.label}`);
    const result = {
      revision: this.state.revision,
      changed: restored.changes.map((change) => change.id),
      key: this.key(this.state.project),
    };
    this.emit();
    return result;
  }
  redo() {
    const history = this.redoStack.at(-1);
    if (!history) return;
    const restored = this.restore(history.changes, "after", history.afterOrder);
    this.redoStack.pop();
    if (history.gesture) this.endGesture(history.gesture);
    this.undoStack.push(history);
    this.install(restored.project, restored.changes, `Redo ${history.label}`);
    const result = {
      revision: this.state.revision,
      changed: restored.changes.map((change) => change.id),
      key: this.key(this.state.project),
    };
    this.emit();
    return result;
  }
  revert(
    id: string,
    options: { transactionId?: string; actor?: string; intent?: string } = {},
  ): TransactionResult {
    idSchema.parse(id);
    options = revertOptionsSchema.parse(options);
    const fingerprint = contentKey({ revert: id, options }),
      retry = this.retry(options.transactionId, fingerprint);
    if (retry) return retry;
    const record = this.records.find((transaction) => transaction.id === id);
    if (!record) throw Error("Transaction is not retained in history");
    const conflicts = record.changes
      .filter((change) => {
        const current = this.documents.get(change.id);
        return (
          current !== change.after &&
          (!current || !change.after || this.source(current) !== this.source(change.after))
        );
      })
      .map(
        (change): TransactionConflict => ({
          document: change.id,
          reason: "revert",
          actual: this.revision(change.id),
        }),
      );
    if (conflicts.length) throw new RevisionConflict("Later changes overlap this transaction", conflicts);
    const restored = this.restore(record.changes, "before", record.beforeOrder, true),
      transactionId = options.transactionId ?? crypto.randomUUID(),
      label = `Revert ${record.label}`,
      diff = this.summary(restored.changes);
    this.pushHistory(restored.changes, label, restored.project);
    this.records.push(
      freeze({
        id: transactionId,
        revision: this.state.revision + 1,
        actor: options.actor,
        intent: options.intent,
        label,
        changes: restored.changes,
        diff,
        beforeOrder: this.state.project.documents.map((document) => document.id),
        afterOrder: restored.project.documents.map((document) => document.id),
        reverts: id,
      }),
    );
    this.trimHistory();
    this.install(restored.project, restored.changes, label);
    const result = {
      revision: this.state.revision,
      changed: restored.changes.map((change) => change.id),
      key: this.key(this.state.project),
      transactionId,
      diff,
    };
    this.remember(transactionId, fingerprint, result);
    this.emit();
    return clone(result);
  }
  markSaved(revision: number) {
    if (!Number.isInteger(revision) || revision < 0)
      throw Error("Saved revision must be a nonnegative integer");
    if (revision > this.state.revision) throw Error("Cannot save a future revision");
    this.state = { ...this.state, savedRevision: revision };
    this.emit();
  }
  replace(project: Project) {
    const parsed = parseProject(project),
      ids = new Set([...this.documents.keys(), ...parsed.documents.map((d) => d.id)]),
      changes = this.changes(this.state.project, parsed, ids);
    this.undoStack = [];
    this.redoStack = [];
    this.records = [];
    for (const id of this.receipts.keys()) this.expiredIds.add(id);
    this.receipts.clear();
    this.closedGestures.clear();
    this.state = { ...this.state, diagnostics: [] };
    this.install(parsed, changes, "Open project");
    this.emit();
  }
  diagnostics(diagnostics: Diagnostic[]) {
    this.state = { ...this.state, diagnostics: clone(diagnostics) };
    this.emit();
  }
  export() {
    return JSON.stringify(this.state.project, null, 2);
  }
}
