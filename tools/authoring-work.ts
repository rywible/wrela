import { mkdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import {
  appendWorkEvent,
  attachWorkReview,
  authorHeroStudy,
  authoringExemplars,
  authoringJob,
  authoringTaskContext,
  createWorkSession,
  decideWork,
  evaluateWork,
  experimentAuthoring,
  instantiateEditRecipe,
  iterateAuthoring,
  materializeWorkProposal,
  parseWorkSession,
  planAuthoringIntent,
  promoteEditRecipe,
  promoteStudyRecipe,
  proposeWork,
  publishWork,
  studyAuthoring,
  verifyWorkSession,
  workHandoff,
} from "@wrela/authoring";
import { botanicalStructure } from "@wrela/compiler";
import { contentKey } from "@wrela/model";
import { finishAuthoringWork } from "./authoring-finish";
import { runAuthoringPacket } from "./authoring-packet-runner";
import { runAuthoringCapture, runAuthoringReview } from "./authoring-review-runner";
import { studyBrowserReviewer } from "./authoring-study-runner";
import { runAuthoringToolkit } from "./authoring-toolkit-store";
import { WorkFileStore } from "./authoring-work-store";
import { WorkspaceBridge } from "./bridge";

async function request(path?: string) {
  if (!path) throw Error("A request JSON path is required");
  const file = Bun.file(path);
  if (file.size > 64 * 1024 * 1024) throw Error("Request exceeds 64 MiB");
  return file.json();
}
export async function runAuthoringWork(directory: string, args: string[]) {
  const [command = "list", id, path] = args,
    store = new WorkFileStore(directory),
    bridge = new WorkspaceBridge(directory);
  await bridge.initialize();
  const source = async () => {
    const current = await bridge.read();
    if (!current) throw Error("Initialize the source workspace before authoring work");
    return current.project;
  };
  if (command === "list") return store.list();
  if (command === "exemplars") return authoringExemplars;
  if (command === "recipes") return store.recipes(id);
  if (command === "restore") {
    const work = await store.restore(await request(id));
    return { key: contentKey(work), ...workHandoff(work) };
  }
  if (command === "init" || command === "import") {
    const input = await request(id);
    const work = command === "init" ? createWorkSession(await source(), input) : verifyWorkSession(input);
    const key = await store.put(work, null);
    return { key, ...workHandoff(work) };
  }
  if (command === "plan") return planAuthoringIntent(await source(), await request(id));
  if (command === "instantiate") {
    const input = await request(id);
    return instantiateEditRecipe(await source(), input.recipe, input.parameters, input.targets);
  }
  if (!id) throw Error("A work session ID is required");
  const storePath = join(await store.directory(), "evidence");
  if (command === "toolkit")
    return runAuthoringToolkit(directory, id, await request(path), dirname(resolve(path!)));
  if (command === "hero") {
    const input = await request(path);
    if (input.qualityTarget?.references?.length)
      input.qualityTarget.references = (
        await store.retainEvidence({ evidence: input.qualityTarget.references }, dirname(resolve(path!)))
      ).evidence;
    return authorHeroStudy(
      store,
      id,
      input,
      studyBrowserReviewer(join(storePath, `hero-${crypto.randomUUID()}`)),
    );
  }
  if (command === "iterate")
    return iterateAuthoring(store, id, await request(path), (b, c, w, s) =>
      runAuthoringPacket(b, c, w.constraints, s, join(storePath, `iteration-${crypto.randomUUID()}`)),
    );
  const work = await store.get(id),
    key = contentKey(work);
  if (command === "context") {
    const target =
      path ??
      work.constraints.find((c) => c.kind === "dimensions" || c.kind === "clearance")?.target ??
      work.baseline.entry;
    const doc = work.baseline.documents.find((d) => d.id === target);
    return {
      key,
      brief: work.brief,
      ...authoringTaskContext(
        work.baseline,
        path ??
          work.constraints.find((c) => c.kind === "dimensions" || c.kind === "clearance")?.target ??
          work.baseline.entry,
        work.constraints,
      ),
      branches:
        doc?.kind === "vegetation"
          ? botanicalStructure(doc)
              .branches.filter((b) => b.level <= 2)
              .slice(0, 128)
              .map((b) => ({
                id: b.id,
                parent: b.parent,
                level: b.level,
                start: b.start,
                end: b.end,
                bare: b.bare,
              }))
          : undefined,
    };
  }
  if (command === "evaluate")
    return evaluateWork(store, id, await request(path), (baseline, candidate, w, settings) =>
      runAuthoringPacket(
        baseline,
        candidate,
        w.constraints,
        settings,
        join(storePath, `packet-${crypto.randomUUID()}`),
      ),
    );
  if (command === "job")
    return authoringJob(
      store,
      id,
      await request(path),
      studyBrowserReviewer(join(storePath, `job-${crypto.randomUUID()}`)),
    );
  if (command === "study") {
    const input = await request(path);
    if (input.brief?.quality?.references?.length)
      input.brief.quality.references = (
        await store.retainEvidence({ evidence: input.brief.quality.references }, dirname(resolve(path!)))
      ).evidence;
    return studyAuthoring(
      store,
      id,
      input,
      studyBrowserReviewer(join(storePath, `study-${crypto.randomUUID()}`)),
    );
  }
  if (command === "finish")
    return finishAuthoringWork(directory, id, await request(path), async (input): Promise<unknown> => {
      const file = join(storePath, `adoption-${crypto.randomUUID()}.json`);
      await Bun.write(file, JSON.stringify(input));
      return runAuthoringWork(directory, ["adopt", id, file]);
    });
  if (command === "inspect") return { key, ...workHandoff(work) };
  if (command === "export") return work;
  if (command === "backup") return store.backup(id);
  const input = await request(path);
  if (command !== "adopt" && input.expectedKey !== key)
    throw Error("Work changed or expectedKey missing; inspect before continuing");
  if (command === "remember") {
    const recipe = await store.saveRecipe(
      await promoteStudyRecipe(work, input.proposal, input.recipe, (ref) => Bun.file(ref).json()),
    );
    const evidence = await store.saveArtifact("semantic-recipe.json", recipe);
    const next = appendWorkEvent(work, {
      id: `remember-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      kind: "handoff",
      actor: "author",
      summary: `Retained semantic recipe ${recipe.id}`,
      evidence: [evidence],
    });
    return { recipe, key: await store.put(next, key), evidence };
  }
  let next = work;
  const started = performance.now(),
    output = join(await store.directory(), "evidence", `${id}-${crypto.randomUUID()}`);
  await mkdir(output, { recursive: true });
  if (command === "propose") next = proposeWork(work, input.proposal, input.operations, input.actor);
  else if (command === "review") {
    const p = work.proposals.find((p) => p.id === input.proposal);
    if (!p) throw Error("Unknown proposal");
    const report = await runAuthoringReview(
      work.baseline,
      materializeWorkProposal(work, p.id),
      work.constraints,
      output,
      input.hardware === true,
    );
    await Bun.write(join(output, "review.json"), JSON.stringify(report, null, 2));
    next = attachWorkReview(work, p.id, report);
  } else if (command === "feedback")
    next = appendWorkEvent(work, await store.retainEvidence(input.event, dirname(resolve(path))));
  else if (command === "retain-evidence")
    next = parseWorkSession(await store.retainEvidence(work, dirname(resolve(path))));
  else if (command === "decide") next = decideWork(work, input.proposal, input.decision);
  else if (command === "adopt") {
    return publishWork(
      store,
      {
        async snapshot() {
          const source = await bridge.read();
          if (!source) throw Error("Source workspace is missing; initialize it before adopting work");
          return { key: source.key, revision: 0 };
        },
        async preview(batch) {
          const {
            published: _published,
            workspaceKey: _key,
            ...receipt
          } = await bridge.transact(work.baselineKey, batch, { preview: true, exact: true });
          return receipt;
        },
        async commit(batch, baselineKey) {
          const result = await bridge.transact(baselineKey, batch, { exact: true });
          const { published: _published, workspaceKey: _key, ...receipt } = result;
          return receipt;
        },
        async receipt(id) {
          return (await bridge.transactionReceipt(id))?.result;
        },
      },
      {
        id,
        expectedKey: input.expectedKey,
        proposal: input.proposal,
        requireArtReview: input.requireArtReview === true,
      },
    );
  } else if (command === "experiment") {
    const p = input.proposal ? work.proposals.find((p) => p.id === input.proposal) : undefined;
    if (input.proposal && !p) throw Error("Unknown experiment proposal");
    let trial = 0;
    const result = await experimentAuthoring(
      p ? materializeWorkProposal(work, p.id) : work.baseline,
      work.constraints,
      input.experiment,
      (baseline, candidate, constraints) =>
        runAuthoringReview(
          baseline,
          candidate,
          constraints,
          join(output, `trial-${trial++}`),
          input.hardware === true,
        ),
    );
    await Bun.write(join(output, "experiment.json"), JSON.stringify(result, null, 2));
    next = appendWorkEvent(work, {
      id: `experiment-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      kind: "review",
      actor: input.actor ?? "agent",
      summary: `Measured control experiment; suggestion ${result.suggested ?? "none"}`,
      evidence: [join(output, "experiment.json")],
    });
  } else if (command === "capture") {
    const p = input.proposal ? work.proposals.find((p) => p.id === input.proposal) : undefined;
    if (input.proposal && !p) throw Error("Unknown capture proposal");
    const result = await runAuthoringCapture(
      p ? materializeWorkProposal(work, p.id) : work.baseline,
      input.settings,
      output,
    );
    next = appendWorkEvent(work, {
      id: `capture-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      kind: "review",
      actor: input.actor ?? "agent",
      summary: "Revision-bound hardware capture",
      evidence: [result.image, join(output, "capture.json")],
    });
  } else if (command === "recipe") {
    const recipe = await store.saveRecipe(promoteEditRecipe(work, input.proposal, input.recipe));
    await Bun.write(join(output, "recipe.json"), JSON.stringify(recipe, null, 2));
    next = appendWorkEvent(work, {
      id: `recipe-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      kind: "handoff",
      actor: input.actor ?? "agent",
      summary: `Retained recipe ${recipe.id}`,
      evidence: [join(output, "recipe.json")],
    });
  } else throw Error(`Unknown work command: ${command}`);
  if (command !== "feedback")
    next = appendWorkEvent(next, {
      id: `${command}-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      actor: input.actor ?? "agent",
      kind: command === "propose" ? "propose" : "review",
      summary: `${command} ${input.proposal ?? ""}`.trim(),
      durationMs: performance.now() - started,
      tokens: input.tokens ?? null,
      humanInterventions: input.humanInterventions ?? null,
    });
  return { key: await store.put(next, key), output, ...workHandoff(next) };
}
