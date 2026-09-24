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
import { executeOperation } from "./capabilities";
import {
  batchSchema,
  commandDescriptions,
  type EditBatch,
  type Operation,
  operationSchema,
  parseEditBatch,
} from "./commands";
import {
  type CreaturePoint,
  explainCreatureSource,
  inspectCreatureSource,
  selectCreatureRegions,
} from "./creature";
import {
  type AuthoringCandidate,
  type CandidateAdoptOptions,
  type CandidateRequest,
  candidateAdoptOptionsSchema,
  candidateRequestSchema,
} from "./creature-candidates";
import { CREATURE_JOB_LIMITS, type CreatureSolveJobRequest, CreatureSolveJobs } from "./creature-jobs";
import { type CreatureRepairRequest, proposeCreatureRepair } from "./creature-repair";
import { type CreatureSolveRequest, solveCreatureEdits } from "./creature-solver";
import {
  authoringImpact,
  type DiscoveryQuery,
  discoverAuthoring,
  type InspectionQuery,
  inspectAuthoring,
} from "./discovery";
import { reviewSourcePreservation, sourceValueChanges } from "./domain-constraints";
import {
  availableDomainRecipes,
  domainRecipeDescriptions,
  domainRecipeSchema,
  planDomainRecipe,
} from "./domain-recipes";
import {
  type DomainVariantRequest,
  type DomainVariantResult,
  domainVariantCandidateId,
  domainVariantRequestSchema,
} from "./domain-variants";
import { assertSourceKeysPreserved } from "./source-keys";
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
  candidateRevision: number;
  jobRevision: number;
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
  private candidates = new Map<string, AuthoringCandidate>();
  private creatureJobs: CreatureSolveJobs;
  private adoptedCandidateOptions = new Map<string, string>();
  private adoptingCandidates = new Set<string>();
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
      candidateRevision: 0,
      jobRevision: 0,
      documentRevisions: Object.fromEntries(parsed.documents.map((d) => [d.id, 0])),
      selection: parsed.entry,
      canUndo: false,
      canRedo: false,
      historyLabel: "",
      diagnostics: [],
    });
    this.index(parsed);
    this.creatureJobs = new CreatureSolveJobs({
      changed: () => {
        this.state = { ...this.state, jobRevision: this.state.jobRevision + 1 };
        this.emit();
      },
      completed: (result) => {
        if (result.batch.expectedRevision !== this.state.revision)
          throw new RevisionConflict(
            "Creature solve completed against a stale source revision; inspect its retained batch and propose fresh work",
          );
        return this.proposeCandidate({ id: result.id, batch: result.batch }).id;
      },
    });
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
  discover(query: DiscoveryQuery = {}) {
    return discoverAuthoring(this.state.project, this.state.revision, query);
  }
  inspectFields(query: InspectionQuery) {
    return { revision: this.state.revision, ...inspectAuthoring(this.state.project, query) };
  }
  impact(batch: EditBatch) {
    const preview = this.preview(batch);
    return { ...authoringImpact(this.state.project, batch.operations), diff: preview.diff };
  }
  discoverFull() {
    return {
      revision: this.state.revision,
      operations: Object.entries(commandDescriptions).map(([kind, description]) => ({ kind, description })),
      schema: z.toJSONSchema(operationSchema),
      batchSchema: z.toJSONSchema(batchSchema),
      creatures: {
        inspect: "inspectCreature(target, region?) exposes anatomical source and its connected controls",
        repair:
          "repairCreature({id,target,sourceKey,expectedRevision,region,handles,protectedPoints,support,maxDisplacement?,detail?,intent}) fits bounded rest-space handles and proposes ordinary sculpt source with measured sensitivities; conflicting protection rejects",
        select:
          "selectCreature(target, point, radius?) returns approximate character-space region support selection",
        explain:
          "explainCreature(target, region) traces declared source dependencies without claiming measured sensitivities",
        solve:
          "solveCreature({id,target,expectedRevision,controls,objectives,preserve?,budget?}) proposes bounded source constraint edits with residuals; no automatic acceptance",
        proportion:
          "Scale a region and explicitly selected descendants; preserve stable chart coordinates, anchor offsets, groom length and width. Nonuniform edits retain spherical detail support radii. Unrepresentable shears fail atomically.",
      },
      creatureJobs: {
        start:
          "startCreatureSolveJob({request,deadlineMs?,evaluationsPerSlice?}) retains immutable source and returns immediately",
        inspect:
          "inspectCreatureSolveJob(id), listCreatureSolveJobs(), creatureSolveJobUsage() expose progress and bounded work",
        wait: "awaitCreatureSolveJob(id) resolves to a terminal snapshot without adopting source",
        lifecycle:
          "pauseCreatureSolveJob(id), resumeCreatureSolveJob(id), cancelCreatureSolveJob(id), releaseCreatureSolveJob(id)",
        timing:
          "Actual solver continuations yield between bounded evaluations; deadlines include queued and paused time. A single evaluation remains atomic.",
        stale:
          "A source change preserves a terminal stale result batch; only current-source completed work prepares a candidate, and adoption still checks revisions",
        limits: CREATURE_JOB_LIMITS,
      },
      candidates: {
        propose: "proposeCandidate({id,batch,budget?}) validates an isolated alternative",
        compare: "compareCandidates(left,right?) compares alternatives against their shared baseline",
        adopt: "adoptCandidate(id, options?) commits with original source preconditions and retry identity",
        cancel: "cancelCandidate(id) rejects unaccepted work; releaseCandidate(id) frees retained source",
        retainedLimit: 32,
      },
      domainVariants: {
        propose:
          "proposeDomainVariants({id,expectedRevision,variants:[{id,recipe,preserve?}]}) prepares up to eight isolated alternatives atomically through existing candidate transactions",
        schema: z.toJSONSchema(domainVariantRequestSchema),
        recipeSchema: z.toJSONSchema(domainRecipeSchema),
        recipes: Object.entries(domainRecipeDescriptions).map(([kind, description]) => ({
          kind,
          description,
        })),
        targets: this.state.project.documents.flatMap((document) => {
          const recipes = availableDomainRecipes(document);
          return recipes.length ? [{ id: document.id, name: document.name, recipes }] : [];
        }),
        acceptance:
          "Constraint results certify authored-source preservation. Source diffs and costs are immediate; rendered appearance, temporal error, complete-frame latency and GPU/memory cost remain unreviewed until measured.",
        adoption:
          "Use existing compareCandidates/adoptCandidate/cancelCandidate/releaseCandidate. Every variant shares an immutable revision baseline; adopting one makes sibling proposals stale.",
      },
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
  startCreatureSolveJob(input: CreatureSolveJobRequest) {
    return this.creatureJobs.start(input, () => {
      if (input.request.expectedRevision !== this.state.revision) throw new RevisionConflict();
      const document = this.documents.get(input.request.target);
      if (!document || document.kind !== "character") throw Error("Select a character");
      return { document, sourceKey: this.key(this.state.project), sourceRevision: this.state.revision };
    });
  }
  inspectCreatureSolveJob(id: string) {
    return this.creatureJobs.inspect(id);
  }
  listCreatureSolveJobs() {
    return this.creatureJobs.list();
  }
  creatureSolveJobUsage() {
    return this.creatureJobs.usage();
  }
  awaitCreatureSolveJob(id: string) {
    return this.creatureJobs.wait(id);
  }
  cancelCreatureSolveJob(id: string) {
    return this.creatureJobs.cancel(id);
  }
  pauseCreatureSolveJob(id: string) {
    return this.creatureJobs.pause(id);
  }
  resumeCreatureSolveJob(id: string) {
    return this.creatureJobs.resume(id);
  }
  releaseCreatureSolveJob(id: string) {
    return this.creatureJobs.release(id);
  }
  solveCreature(input: CreatureSolveRequest, signal?: AbortSignal) {
    if (input.expectedRevision !== this.state.revision) throw new RevisionConflict();
    const document = this.documents.get(input.target);
    if (!document || document.kind !== "character") throw Error("Select a character");
    const result = solveCreatureEdits(document, input, signal);
    const candidate = this.proposeCandidate({ id: result.id, batch: result.batch });
    return { ...result, candidate };
  }
  repairCreature(input: CreatureRepairRequest) {
    if (input.expectedRevision !== this.state.revision) throw new RevisionConflict();
    const document = this.documents.get(input.target);
    if (!document || document.kind !== "character") throw Error("Select a character");
    const result = proposeCreatureRepair(document, input);
    return { ...result, candidate: this.proposeCandidate({ id: result.id, batch: result.batch }) };
  }
  private emitCandidates() {
    this.state = { ...this.state, candidateRevision: this.state.candidateRevision + 1 };
    this.emit();
  }
  inspectCreature(target: string, region?: string) {
    return {
      revision: this.state.revision,
      documentRevision: this.revision(target),
      ...inspectCreatureSource(this.documents.get(target), region),
    };
  }
  selectCreature(target: string, point: CreaturePoint, radius = 0) {
    return {
      revision: this.state.revision,
      target,
      selections: selectCreatureRegions(this.documents.get(target), point, radius),
    };
  }
  explainCreature(target: string, region: string) {
    return { revision: this.state.revision, ...explainCreatureSource(this.documents.get(target), region) };
  }
  /** Create an isolated, inspectable alternative without modifying accepted source. */
  proposeCandidate(input: CandidateRequest): AuthoringCandidate {
    return this.proposeCandidates([input])[0];
  }
  /** Validate every alternative before retaining or notifying any of them. */
  proposeCandidates(inputs: CandidateRequest[]): AuthoringCandidate[] {
    if (!inputs.length || inputs.length > 32) throw Error("Propose between one and 32 candidates");
    const ids = inputs.map((input) => input.id);
    if (new Set(ids).size !== ids.length) throw Error("Candidate IDs must be unique in a batch");
    const candidates = inputs.map((input) => this.prepareAuthoringCandidate(input));
    const additions = candidates.filter((candidate) => !this.candidates.has(candidate.id));
    if (this.candidates.size + additions.length > 32)
      throw Error("Candidate budget exhausted; release retained candidates before proposing alternatives");
    for (const candidate of additions) this.candidates.set(candidate.id, freeze(candidate));
    if (additions.length) this.emitCandidates();
    return clone(candidates);
  }
  private prepareAuthoringCandidate(input: CandidateRequest): AuthoringCandidate {
    const request = candidateRequestSchema.parse(input),
      fingerprint = contentKey(request);
    assertSourceKeysPreserved(input, request, "candidate");
    const previous = this.candidates.get(request.id);
    if (previous) {
      if (previous.fingerprint !== fingerprint)
        throw new RevisionConflict("A candidate ID cannot be reused for different work", [
          { reason: "transaction-id" },
        ]);
      return clone(previous);
    }
    if (this.candidates.size >= 32)
      throw Error("Candidate budget exhausted; release a retained candidate before proposing another");
    if (request.batch.operations.length > (request.budget?.maxOperations ?? 256))
      throw Error("Candidate exceeds operation budget");
    const preview = this.preview(request.batch);
    const bytes = canonical(preview.changes).length * 2;
    if (bytes > (request.budget?.maxSourceBytes ?? 4 * 1024 * 1024))
      throw Error("Candidate exceeds source byte budget");
    const candidate: AuthoringCandidate = {
      id: request.id,
      status: "proposed",
      sourceRevision: this.state.revision,
      sourceKey: this.key(this.state.project),
      fingerprint,
      batch: clone(request.batch),
      changes: preview.changes,
      diff: preview.diff,
    };
    return candidate;
  }
  proposeDomainVariants(input: DomainVariantRequest): DomainVariantResult {
    const started = performance.now(),
      request = domainVariantRequestSchema.parse(input);
    if (request.expectedRevision !== this.state.revision) throw new RevisionConflict();
    const source = this.state.project,
      sourceKey = this.key(source),
      sourceRevision = this.state.revision;
    const planned = request.variants.map((variant) => {
      const plan = planDomainRecipe(source, variant.recipe);
      const batch: EditBatch = {
        expectedRevision: sourceRevision,
        actor: request.actor,
        intent: request.intent ?? domainRecipeDescriptions[variant.recipe.kind],
        label: `${request.id.slice(0, 58)}: ${variant.id.slice(0, 58)}`,
        operations: plan.operations,
      };
      const prepared = this.prepare(batch);
      if (!prepared.changes.length) throw Error(`Variant ${variant.id} does not change accepted source`);
      const constraints = reviewSourcePreservation(source, prepared.after, [
        ...plan.preserve,
        ...variant.preserve,
      ]);
      const failed = constraints.filter((constraint) => !constraint.passed);
      if (failed.length)
        throw Error(
          `Variant ${variant.id} changes protected source: ${failed.map((constraint) => `${constraint.target}/${constraint.path.join("/")}`).join(", ")}`,
        );
      const differences = prepared.changes.flatMap((change) =>
        change.before && change.after ? [sourceValueChanges(change.before, change.after)] : [],
      );
      const bytes = (side: "before" | "after") =>
        new TextEncoder().encode(JSON.stringify(prepared.changes.map((change) => change[side] ?? null)))
          .byteLength;
      return {
        variant,
        plan,
        batch,
        constraints,
        sourceDiff: {
          changes: differences.flatMap((diff) => diff.changes),
          truncated: differences.some((diff) => diff.truncated),
        },
        sourceCost: {
          operations: batch.operations.length,
          changedDocuments: prepared.changes.length,
          beforeBytes: bytes("before"),
          afterBytes: bytes("after"),
        },
        candidateKey: this.key(prepared.after),
      };
    });
    const candidates = this.proposeCandidates(
      planned.map(({ variant, batch }) => ({ id: domainVariantCandidateId(request.id, variant.id), batch })),
    );
    return {
      id: request.id,
      sourceRevision,
      sourceKey,
      sourcePreparationMs: performance.now() - started,
      variants: planned.map((value, index) => ({
        id: value.variant.id,
        candidate: candidates[index],
        domain: value.plan.domain,
        constraints: value.constraints,
        sourceDiff: value.sourceDiff,
        sourceCost: value.sourceCost,
        review: {
          ...value.plan.review,
          status: "unreviewed",
          baselineKey: sourceKey,
          candidateKey: value.candidateKey,
          pendingMeasurements: [...value.plan.review.measurements],
        },
      })),
    };
  }
  inspectCandidate(id: string) {
    return clone(this.candidates.get(idSchema.parse(id)));
  }
  listCandidates() {
    return [...this.candidates.values()].map(({ id, status, sourceRevision, sourceKey, diff }) =>
      clone({ id, status, sourceRevision, sourceKey, diff }),
    );
  }
  compareCandidates(left: string, right?: string) {
    const a = this.candidates.get(idSchema.parse(left));
    const b = right === undefined ? undefined : this.candidates.get(idSchema.parse(right));
    if (!a || (right !== undefined && !b)) throw Error("Candidate is not retained");
    if (b && a.sourceKey !== b.sourceKey)
      throw Error("Candidates must share a source baseline for a matched comparison");
    const ids = new Set([...a.changes, ...(b?.changes ?? [])].map((change) => change.id));
    return clone({
      baseline: a.sourceKey,
      left: a.id,
      right: b?.id ?? "baseline",
      comparisons: [...ids].map((id) => {
        const ac = a.changes.find((change) => change.id === id);
        const bc = b?.changes.find((change) => change.id === id);
        const base = ac?.before ?? bc?.before;
        const l = ac ? ac.after : base,
          r = bc ? bc.after : base;
        return { id, left: l, right: r, equal: canonical(l) === canonical(r) };
      }),
      evidence: "Source comparison only; visual and motion acceptance requires matched review scenarios",
    });
  }
  adoptCandidate(id: string, input: CandidateAdoptOptions = {}): TransactionResult {
    if (this.adoptingCandidates.has(id)) throw Error("Candidate adoption is already in progress");
    const candidate = this.candidates.get(idSchema.parse(id));
    if (!candidate) throw Error("Candidate is not retained");
    const options = candidateAdoptOptionsSchema.parse(input),
      fingerprint = contentKey(options);
    if (candidate.status === "cancelled") throw Error("Candidate was cancelled");
    if (candidate.status === "adopted") {
      if (this.adoptedCandidateOptions.get(id) !== fingerprint)
        throw new RevisionConflict("An adopted candidate cannot be retried with different metadata", [
          { reason: "transaction-id" },
        ]);
      if (!candidate.result) throw Error("Adopted candidate receipt is missing");
      return clone(candidate.result);
    }
    this.adoptingCandidates.add(id);
    let result: TransactionResult;
    try {
      result = this.apply({
        ...candidate.batch,
        ...options,
        transactionId:
          options.transactionId ?? candidate.batch.transactionId ?? `candidate-${contentKey(id)}`,
      });
    } finally {
      this.adoptingCandidates.delete(id);
    }
    this.candidates.set(id, freeze({ ...candidate, status: "adopted", result }));
    this.adoptedCandidateOptions.set(id, fingerprint);
    this.emitCandidates();
    return clone(result);
  }
  cancelCandidate(id: string) {
    if (this.adoptingCandidates.has(id)) throw Error("Candidate adoption is already in progress");
    const candidate = this.candidates.get(idSchema.parse(id));
    if (!candidate) throw Error("Candidate is not retained");
    if (candidate.status === "adopted") throw Error("Revert the adopted transaction to undo accepted source");
    this.candidates.set(id, freeze({ ...candidate, status: "cancelled" }));
    this.emitCandidates();
    return clone({ ...candidate, status: "cancelled" as const });
  }
  releaseCandidate(id: string) {
    idSchema.parse(id);
    if (this.adoptingCandidates.has(id)) throw Error("Candidate adoption is already in progress");
    this.candidates.delete(id);
    this.adoptedCandidateOptions.delete(id);
    this.emitCandidates();
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
      assertSourceKeysPreserved(document, parsed);
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
    for (const operation of batch.operations) executeOperation(candidate, operation, observedReads);
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
    const batch = parseEditBatch(input),
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
    const batch = parseEditBatch(input),
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
    this.candidates.clear();
    this.adoptedCandidateOptions.clear();
    this.state = { ...this.state, diagnostics: [] };
    this.install(parsed, changes, "Open project");
    this.creatureJobs.reset();
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
