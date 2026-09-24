import {
  type AuthoringStudyRequest,
  appendWorkEvent,
  authorHeroStudy,
  authoringJob,
  authoringTaskContext,
  type EvaluationRequest,
  evaluateWork,
  heroDesignSchema,
  instantiateCreationRecipe,
  iterateAuthoring,
  latestReviewPacket,
  promoteCreationRecipe,
  promoteStudyRecipe,
  reviewVerdict,
  studyAuthoring,
} from "@wrela/authoring";
import { contentKey } from "@wrela/model";
import { createAuthoringReviewSession } from "@wrela/review";
import type { AuthoringWorkbench } from "./authoring-workbench";
/** A small orchestration layer over the existing persistence/publication authority. */
export function fastAuthoringWork(workbench: AuthoringWorkbench) {
  let session = createAuthoringReviewSession(),
    idle: ReturnType<typeof setTimeout> | undefined;
  async function warm<T>(run: (review: ReturnType<typeof createAuthoringReviewSession>) => Promise<T>) {
    clearTimeout(idle);
    try {
      return await run(session);
    } finally {
      idle = setTimeout(() => {
        const old = session;
        session = createAuthoringReviewSession();
        void old.close();
      }, 15000);
    }
  }
  const save = (name: string, value: Blob | object) => workbench.store.saveArtifact(name, value);
  return {
    async rememberCreation(id: string, proposal: string, target: string) {
      const work = await workbench.export(id),
        recipe = promoteCreationRecipe(work, proposal, {
          id: `creation-${crypto.randomUUID().slice(0, 8)}`,
          revision: "1",
          description: work.brief,
          roots: [target],
          capabilities: [],
        });
      const packet = await latestReviewPacket(work, proposal, (ref) => workbench.store.artifact(ref));
      const saved = await workbench.store.saveToolkit({
        version: 1,
        id: recipe.id,
        revision: recipe.revision,
        title: recipe.id,
        description: recipe.description,
        tags: ["creation", "hero"],
        level: "recipe",
        status: "experimental",
        implementation: { kind: "creation", recipe },
        controls: [],
        preserves: ["Internal source relationships", "Part identities"],
        limitations: ["New instances require visual review"],
        previews: [packet.contactSheet],
        engineeringMs: null,
        trials: [],
      });
      const evidence = await save("toolkit-entry.json", saved);
      await workbench.store.put(
        appendWorkEvent(work, {
          id: `remember-${crypto.randomUUID()}`,
          at: new Date().toISOString(),
          kind: "handoff",
          actor: "author",
          summary: `Retained construction ${recipe.id}@${recipe.revision}`,
          evidence: [evidence, packet.contactSheet],
        }),
        contentKey(work),
      );
      return saved;
    },
    async advanceHero(
      id: string,
      proposal: string,
      stage: "blockout" | "construction" | "surface" | "production",
      age: number,
    ) {
      const work = await workbench.export(id),
        base = work.proposals.find((p) => p.id === proposal);
      if (!base) throw Error("Choose a construction alternative first");
      for (const event of [...work.events].reverse())
        for (const ref of [...event.evidence].reverse()) {
          if (!ref.endsWith("hero-design.json") && !ref.endsWith("iteration.json")) continue;
          const evidence = (await workbench.store.artifact(ref)) as {
            candidateKey?: string;
            design?: unknown;
            request?: { design?: unknown };
          };
          if (evidence.candidateKey !== base.candidateKey) continue;
          const parsed = heroDesignSchema.safeParse(evidence.design ?? evidence.request?.design);
          if (!parsed.success) continue;
          const design = { ...parsed.data, stage, age };
          return warm((review) =>
            iterateAuthoring(
              workbench.store,
              id,
              {
                expectedKey: contentKey(work),
                proposal: `${stage}-${crypto.randomUUID().slice(0, 8)}`,
                target: design.id,
                baseProposal: proposal,
                design,
                critique: `Advance retained construction to ${stage}`,
              },
              (b, c, w, s) => review.packet(b, c, w.constraints, s, save),
            ),
          );
        }
      throw Error("This branch has no retained construction design; use targeted repairs");
    },
    async reuseCreation(id: string, recipeId: string, revision: string) {
      const work = await workbench.export(id),
        entry = (await workbench.store.toolkit(recipeId)).find(
          (e) => e.id === recipeId && e.revision === revision,
        );
      if (entry?.implementation.kind !== "creation") throw Error("Creation recipe missing");
      const plan = instantiateCreationRecipe(
        work.baseline,
        entry.implementation.recipe,
        `instance-${crypto.randomUUID().slice(0, 8)}`,
      );
      return warm((review) =>
        iterateAuthoring(
          workbench.store,
          id,
          {
            expectedKey: contentKey(work),
            proposal: `reuse-${crypto.randomUUID().slice(0, 8)}`,
            target: plan.roots[0],
            operations: plan.operations,
            critique: `Review transferred recipe ${entry.id}@${entry.revision}`,
          },
          (b, c, w, s) => review.packet(b, c, w.constraints, s, save),
        ),
      );
    },
    hero(id: string, input: Parameters<typeof authorHeroStudy>[2]) {
      return warm((review) =>
        authorHeroStudy(workbench.store, id, input, (b, c, cs, s) => review.study(b, c, cs, s, save)),
      );
    },
    iterate(id: string, input: Parameters<typeof iterateAuthoring>[2]) {
      return warm((review) =>
        iterateAuthoring(workbench.store, id, input, (b, c, w, s) =>
          review.packet(b, c, w.constraints, s, save),
        ),
      );
    },
    job(id: string, input: Parameters<typeof authoringJob>[2]) {
      return warm((review) =>
        authoringJob(workbench.store, id, input, (b, c, cs, s) => review.study(b, c, cs, s, save)),
      );
    },
    async remember(id: string, proposal: string, recipe: string) {
      const work = await workbench.export(id);
      const saved = await workbench.store.saveRecipe(
        await promoteStudyRecipe(work, proposal, recipe, (ref) => workbench.store.artifact(ref)),
      );
      const evidence = await workbench.store.saveArtifact("semantic-recipe.json", saved);
      await workbench.store.put(
        appendWorkEvent(work, {
          id: `remember-${crypto.randomUUID()}`,
          at: new Date().toISOString(),
          kind: "handoff",
          actor: "author",
          summary: `Retained semantic recipe ${saved.id}`,
          evidence: [evidence],
        }),
        contentKey(work),
      );
      return saved;
    },
    study(id: string, input: AuthoringStudyRequest) {
      return warm((review) =>
        studyAuthoring(workbench.store, id, input, (b, c, cs, s) => review.study(b, c, cs, s, save)),
      );
    },
    async context(id: string, target?: string) {
      const w = await workbench.export(id);
      return {
        key: contentKey(w),
        brief: w.brief,
        ...authoringTaskContext(
          w.baseline,
          target ??
            w.constraints.find((c) => c.kind === "dimensions" || c.kind === "clearance")?.target ??
            w.baseline.entry,
          w.constraints,
        ),
      };
    },
    evaluate(id: string, input: EvaluationRequest) {
      return warm((review) =>
        evaluateWork(workbench.store, id, input, (b, c, w, s) => review.packet(b, c, w.constraints, s, save)),
      );
    },
    async finish(id: string, input: { expectedKey: string; proposal: string; requireArtReview?: boolean }) {
      const w = await workbench.export(id);
      if (contentKey(w) !== input.expectedKey) throw Error("Work changed; inspect before finishing");
      const packet = await latestReviewPacket(w, input.proposal, (ref) => workbench.store.artifact(ref));
      if (!reviewVerdict(packet.report, w.constraints, w.baselineKey, packet.report.candidateKey).passed)
        throw Error("Every constraint must pass before finishing");
      await workbench.backup(id);
      const publication =
        w.proposals.find((p) => p.id === input.proposal)?.status === "adopted"
          ? undefined
          : await workbench.adopt(id, input.expectedKey, input.proposal, input.requireArtReview);
      if (publication?.receiptPending)
        return {
          published: publication.outcome === "committed",
          accepted: true,
          receiptPending: true,
          packagingPending: true,
          publication,
          key: contentKey(await workbench.export(id)),
        };
      const current = await workbench.export(id),
        published =
          current.publication?.state === "committed" ||
          current.proposals.find((p) => p.id === input.proposal)?.status === "adopted";
      if (!published) return { published: false, publication, packagingPending: true };
      try {
        return {
          published: true,
          packagingPending: false,
          publication,
          bundle: await workbench.backup(id),
          packet,
        };
      } catch (error) {
        return {
          published: true,
          packagingPending: true,
          publication,
          key: contentKey(current),
          reason: String(error),
        };
      }
    },
  };
}
