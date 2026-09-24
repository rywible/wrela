import {
  type AuthoringExperiment,
  type AuthoringIntent,
  type AuthoringSession,
  appendWorkEvent,
  attachWorkReview,
  BrowserWorkStore,
  createWorkSession,
  decideWork,
  type EditRecipe,
  experimentAuthoring,
  instantiateEditRecipe,
  materializeWorkProposal,
  type Operation,
  planAuthoringIntent,
  promoteEditRecipe,
  proposeWork,
  publishWork,
  sessionWorkPublisher,
  verifyWorkSession,
  type WorkSession,
  workHandoff,
} from "@wrela/authoring";
import { contentKey } from "@wrela/model";

import { captureAuthoring, reviewAuthoring } from "@wrela/review";

import { fastAuthoringWork } from "./authoring-fast-work";

/** Persistent API shared by human UI and agents; publication always uses the current AuthoringSession. */
export class AuthoringWorkbench {
  readonly store = new BrowserWorkStore();
  readonly fast = fastAuthoringWork(this);
  constructor(
    private session: () => AuthoringSession,
    private persist: (sourceKey: string) => Promise<void>,
  ) {}
  list() {
    return this.store.list();
  }
  async open(id: string) {
    const work = await this.store.get(id);
    return { key: contentKey(work), ...workHandoff(work) };
  }
  async export(id: string) {
    return this.store.get(id);
  }
  async backup(id: string) {
    return this.store.backup(id);
  }
  async restore(input: unknown, expectedKey: string | null = null) {
    const work = await this.store.restore(input, expectedKey);
    return this.open(work.id);
  }
  async import(input: unknown, expectedKey: string | null = null) {
    return this.store.put(verifyWorkSession(input), expectedKey);
  }
  async create(input: Parameters<typeof createWorkSession>[1]) {
    const work = createWorkSession(this.session().getSnapshot().project, input);
    await this.store.put(work, null);
    return this.open(work.id);
  }
  private async change(
    id: string,
    expectedKey: string,
    transform: (work: WorkSession) => WorkSession | Promise<WorkSession>,
  ) {
    const work = await this.store.get(id);
    if (contentKey(work) !== expectedKey) throw Error("Work changed; reopen before editing");
    const started = performance.now();
    let next = await transform(work);
    if (next.events.length === work.events.length)
      next = appendWorkEvent(next, {
        id: `action-${crypto.randomUUID()}`,
        at: new Date().toISOString(),
        kind: next.proposals.length > work.proposals.length ? "propose" : "review",
        actor: "author",
        summary: `Updated work ${id}`,
        durationMs: performance.now() - started,
      });
    await this.store.put(next, expectedKey);
    return this.open(id);
  }
  propose(id: string, expectedKey: string, proposal: string, operations: Operation[]) {
    return this.change(id, expectedKey, (w) => proposeWork(w, proposal, operations));
  }
  plan(input: AuthoringIntent) {
    return planAuthoringIntent(this.session().getSnapshot().project, input);
  }
  review(id: string, expectedKey: string, proposal: string) {
    return this.change(id, expectedKey, async (w) => {
      const p = w.proposals.find((p) => p.id === proposal);
      if (!p) throw Error("Unknown proposal");
      const report = await reviewAuthoring(w.baseline, materializeWorkProposal(w, p.id), w.constraints, {
        saveArtifact: (name, value) => this.store.saveArtifact(name, value),
      });
      return attachWorkReview(w, proposal, report);
    });
  }
  feedback(id: string, expectedKey: string, event: Parameters<typeof appendWorkEvent>[1]) {
    return this.change(id, expectedKey, (w) => appendWorkEvent(w, event));
  }
  decide(id: string, expectedKey: string, proposal: string, decision: Parameters<typeof decideWork>[2]) {
    return this.change(id, expectedKey, (w) => decideWork(w, proposal, decision));
  }
  async adopt(id: string, expectedKey: string, proposal: string, requireArtReview = false) {
    return publishWork(
      this.store,
      { ...sessionWorkPublisher(this.session), flush: this.persist },
      {
        id,
        expectedKey,
        proposal,
        requireArtReview,
      },
    );
  }
  async experiment(id: string, proposal: string | undefined, input: AuthoringExperiment) {
    const w = await this.store.get(id),
      p = proposal ? w.proposals.find((p) => p.id === proposal) : undefined;
    if (proposal && !p) throw Error("Unknown experiment baseline");
    const started = performance.now();
    const result = await experimentAuthoring(
      p ? materializeWorkProposal(w, p.id) : w.baseline,
      w.constraints,
      input,
      (baseline, candidate, constraints) =>
        reviewAuthoring(baseline, candidate, constraints, {
          saveArtifact: (name, value) => this.store.saveArtifact(name, value),
        }),
    );
    const evidence = await this.store.saveArtifact("experiment.json", result);
    await this.change(id, contentKey(w), (work) =>
      appendWorkEvent(work, {
        id: `experiment-${crypto.randomUUID()}`,
        at: new Date().toISOString(),
        kind: "review",
        actor: "author",
        summary: `Measured control experiment; suggestion ${result.suggested ?? "none"}`,
        durationMs: performance.now() - started,
        evidence: [evidence],
      }),
    );
    return { ...result, evidence };
  }
  async recipe(id: string, proposal: string, input: Parameters<typeof promoteEditRecipe>[2]) {
    const work = await this.store.get(id),
      recipe = await this.store.saveRecipe(promoteEditRecipe(work, proposal, input));
    const evidence = await this.store.saveArtifact("recipe.json", recipe);
    await this.change(id, contentKey(work), (w) =>
      appendWorkEvent(w, {
        id: `recipe-${crypto.randomUUID()}`,
        at: new Date().toISOString(),
        kind: "handoff",
        actor: "author",
        summary: `Retained recipe ${recipe.id}`,
        evidence: [evidence],
      }),
    );
    return recipe;
  }
  recipes(search?: string) {
    return this.store.recipes(search);
  }
  instantiate(recipe: EditRecipe, parameters?: Record<string, number>, targets?: Record<string, string>) {
    return instantiateEditRecipe(this.session().getSnapshot().project, recipe, parameters, targets);
  }
  async capture(id: string, proposal: string | undefined, settings: Parameters<typeof captureAuthoring>[1]) {
    const w = await this.store.get(id),
      p = proposal ? w.proposals.find((p) => p.id === proposal) : undefined;
    if (proposal && !p) throw Error("Unknown capture proposal");
    const result = await captureAuthoring(p ? materializeWorkProposal(w, p.id) : w.baseline, settings);
    const { blob, ...metadata } = result;
    const image = await this.store.saveArtifact("capture.png", blob);
    const evidence = await this.store.saveArtifact("capture.json", { ...metadata, image });
    await this.change(id, contentKey(w), (work) =>
      appendWorkEvent(work, {
        id: `capture-${crypto.randomUUID()}`,
        at: new Date().toISOString(),
        kind: "review",
        actor: "author",
        summary: "Revision-bound hardware capture",
        evidence: [image, evidence],
      }),
    );
    return { ...metadata, image, evidence };
  }
}
