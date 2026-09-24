import {
  AuthoringSession,
  authorCreatureMotion,
  authoringError,
  creatureClipSourceSchema,
  creatureRepairRequestSchema,
  creatureRetargetSchema,
  creatureSolveJobRequestSchema,
  creatureSolveRequestSchema,
  discoverAuthoring,
  domainVariantRequestSchema,
  type EditBatch,
  explainAuthoringSource,
  inspectAuthoring,
  retargetCreatureMotion,
} from "@wrela/authoring";
import {
  type CharacterDefinition,
  contentKey,
  idSchema,
  loadWorkingSet,
  searchDocuments,
  vec3Schema,
} from "@wrela/model";

import { z } from "zod";
/** Local, inspectable authoring over the same transactions used by Studio. */
import { readAuthoringTrace, summarizeAuthoringTrace, traceAuthoring } from "./authoring-trace";
import { runAuthoringWork } from "./authoring-work";
import { WorkspaceBridge } from "./bridge";
export async function runAuthoring(args: string[]) {
  if (!args.length || args[0] === "--help" || args[0] === "-h" || args[1] === "--help")
    return {
      usage: "bun tools/author.ts <workspace> <command>",
      work: [
        "init <request.json>",
        "context <work-id> <target-id>",
        "exemplars",
        "job <work-id> <request.json>",
        "hero <work-id> <request.json>",
        "iterate <work-id> <request.json>",
        "toolkit <work-id> <request.json>",
        "study <work-id> <request.json>",
        "finish <work-id> <request.json>",
        "export <work-id>",
        "backup <work-id>",
        "restore <bundle.json>",
      ],
      example: {
        workspace: "/absolute/workspace",
        work: "asset",
        action: "job",
        input: { budgetSeconds: 30 },
      },
      bridge:
        "bun tools/authoring-agent.ts invocation.json returns result and image paths. Display the gallery before selecting a passing proposal and finishing.",
      portability:
        "bun tools/authoring-portability-check.ts <work-record-or-bundle.json> <new-output-directory>",
    };
  return traceAuthoring(args, () => runAuthoringCommand(args));
}
async function runAuthoringCommand(args: string[]) {
  const [directory, command = "inspect", argument, extra] = args;
  if (directory && command === "work") return { work: await runAuthoringWork(directory, args.slice(2)) };
  if (!directory)
    throw new Error(
      "Usage: bun run author <workspace> inspect [id] | discover | preview/apply <proposal.json> | variants/propose <request.json> | creature-inspect <id> [region] | creature-explain <id> <region> | creature-select/solve/propose/retarget/motion <request.json>",
    );
  if (command === "timing") {
    const rows = await readAuthoringTrace(directory);
    return { rows, summary: summarizeAuthoringTrace(rows, rows[0]?.startedAt ?? Date.now(), Date.now()) };
  }
  const bridge = new WorkspaceBridge(directory);
  await bridge.initialize();
  if (command === "catalog")
    return searchDocuments(await bridge.catalog(), argument ? await Bun.file(argument).json() : {});
  if (command === "inspect" && argument) {
    const repository = await bridge.catalog(),
      entry = repository.index.find((d) => d.id === argument);
    if (!entry) throw Error(`Unknown definition ${argument}`);
    const document = await repository.read(argument),
      sourceKey = contentKey(document);
    return {
      workspaceKey: sourceKey === entry.key ? repository.key : (await bridge.read())?.key,
      sourceKey,
      result: document,
    };
  }
  if (command === "working-set")
    return loadWorkingSet(await bridge.catalog(), argument ? (await Bun.file(argument).json()).roots : []);
  if (command === "apply" || command === "preview") {
    if (!argument) throw new Error("A proposal JSON path is required");
    const proposal = z
      .strictObject({
        workspaceKey: z.string(),
        batch: z.unknown(),
        // Read-only evidence travels with proposals emitted by the CLI.
        report: z.unknown().optional(),
        preview: z.unknown().optional(),
        published: z.boolean().optional(),
      })
      .parse(await Bun.file(argument).json());
    return bridge.transact(proposal.workspaceKey, proposal.batch as EditBatch, {
      preview: command === "preview",
    });
  }
  const current = await bridge.read();
  if (!current) throw new Error("Workspace has no published project. Save a project from Studio first.");
  if (command === "discover")
    return {
      ...discoverAuthoring(current.project, 0, argument ? await Bun.file(argument).json() : {}),
      workspaceKey: current.key,
      headless: {
        commands: [
          "variants",
          "propose",
          "creature-inspect",
          "creature-select",
          "creature-explain",
          "creature-solve",
          "creature-repair",
          "creature-solve-async",
          "creature-propose",
          "creature-retarget",
          "creature-motion",
        ],
        requests:
          "Mutation proposals use JSON {workspaceKey,...request fields}; solver uses request, retarget uses source/target/retarget, motion uses target/clip. Only apply publishes.",
        candidates:
          "Legacy variant candidates are ephemeral. Use work init/propose/review/adopt for persistent proposals, feedback and evidence across processes.",
        solver:
          "creature-solve runs synchronous bounded work. creature-solve-async runs a yielding in-process job to a terminal state with a deadline; jobs are not persisted across CLI processes.",
      },
      transport:
        "A proposal contains {workspaceKey,batch}. Revisions describe this inspected snapshot; workspaceKey identifies the inspected baseline; publication checks document read/write identities under a process lock. transactionId retries return durable receipts.",
    };
  if (command === "explain")
    return { workspaceKey: current.key, result: explainAuthoringSource(current.project, argument, extra) };
  if (command === "fields")
    return {
      workspaceKey: current.key,
      result: { revision: 0, ...inspectAuthoring(current.project, await Bun.file(argument).json()) },
    };
  const session = new AuthoringSession(current.project);
  if (command === "inspect") return { workspaceKey: current.key, result: session.inspect(argument) };
  if (command === "impact")
    return { workspaceKey: current.key, result: session.impact((await Bun.file(argument).json()).batch) };
  if (command === "variants" || command === "propose") {
    if (!argument) throw Error("A request JSON path is required");
    const file = Bun.file(argument);
    if (file.size > 4 * 1024 * 1024) throw Error("Request exceeds 4 MiB input budget");
    const input: unknown = JSON.parse(await file.text());
    if (
      !input ||
      typeof input !== "object" ||
      !("workspaceKey" in input) ||
      input.workspaceKey !== current.key
    )
      throw Error("Workspace changed or workspaceKey missing; inspect it and revise the proposal");
    if (command === "variants") {
      const parsed = z
        .strictObject({ workspaceKey: z.string(), request: domainVariantRequestSchema })
        .parse(input);
      const result = session.proposeDomainVariants(parsed.request);
      return {
        published: false,
        workspaceKey: current.key,
        ...result,
        proposals: result.variants.map((variant) => ({
          workspaceKey: current.key,
          batch: variant.candidate.batch,
          report: { ...variant, candidate: undefined },
        })),
        lifecycle:
          "Candidates are ephemeral; save a proposals entry and use preview/apply. Applying one invalidates siblings' workspace baseline.",
      };
    }
    const parsed = z
      .strictObject({ workspaceKey: z.string(), id: idSchema, batch: z.unknown() })
      .parse(input);
    const candidate = session.proposeCandidate({ id: parsed.id, batch: parsed.batch as EditBatch });
    return {
      published: false,
      workspaceKey: current.key,
      batch: candidate.batch,
      report: {
        candidateId: candidate.id,
        ephemeral: true,
        comparison: session.compareCandidates(candidate.id),
      },
      preview: session.preview(candidate.batch),
    };
  }
  if (command === "creature-inspect") {
    if (!argument) throw Error("A character ID is required");
    return { workspaceKey: current.key, result: session.inspectCreature(argument, extra) };
  }
  if (command === "creature-explain") {
    if (!argument || !extra) throw Error("Character and region IDs are required");
    return { workspaceKey: current.key, result: session.explainCreature(argument, extra) };
  }
  if (command.startsWith("creature-")) {
    if (!argument) throw Error("A request JSON path is required");
    const file = Bun.file(argument);
    if (file.size > 4 * 1024 * 1024) throw Error("Request exceeds 4 MiB input budget");
    const input: unknown = JSON.parse(await file.text());
    if (
      !input ||
      typeof input !== "object" ||
      !("workspaceKey" in input) ||
      input.workspaceKey !== current.key
    )
      throw Error("Workspace changed or workspaceKey missing; inspect it and revise the proposal");
    const character = (id: string): CharacterDefinition => {
      const document = session.inspect(id);
      if (!document || !("kind" in document) || document.kind !== "character")
        throw Error(`Character ${id} does not exist`);
      return document as CharacterDefinition;
    };
    const proposal = (batch: EditBatch, report: unknown) => {
      const preview = session.preview(batch);
      return { published: false, workspaceKey: current.key, batch, report, preview };
    };
    const key = z.string();
    if (command === "creature-select") {
      const request = z
        .strictObject({
          workspaceKey: key,
          target: idSchema,
          point: vec3Schema,
          radius: z.number().finite().min(0).optional(),
        })
        .parse(input);
      return {
        workspaceKey: current.key,
        result: session.selectCreature(request.target, request.point, request.radius),
      };
    }
    if (command === "creature-solve-async") {
      const request = z.strictObject({ workspaceKey: key, job: creatureSolveJobRequestSchema }).parse(input);
      const started = session.startCreatureSolveJob(request.job);
      const job = await session.awaitCreatureSolveJob(started.id);
      if (job.status !== "completed" || !job.result)
        return { published: false, workspaceKey: current.key, job };
      return proposal(job.result.batch, { job, ephemeral: true });
    }
    if (command === "creature-solve") {
      const request = z.strictObject({ workspaceKey: key, request: creatureSolveRequestSchema }).parse(input);
      const solved = session.solveCreature(request.request);
      const { batch, candidate, ...report } = solved;
      return proposal(batch, { ...report, candidateId: candidate.id, ephemeral: true });
    }
    if (command === "creature-repair") {
      const parsed = z.strictObject({ workspaceKey: key, request: creatureRepairRequestSchema }).parse(input);
      const result = session.repairCreature(parsed.request);
      return proposal(result.batch, result);
    }
    if (command === "creature-propose") {
      const request = z.strictObject({ workspaceKey: key, id: idSchema, batch: z.unknown() }).parse(input);
      const candidate = session.proposeCandidate({ id: request.id, batch: request.batch as EditBatch });
      return proposal(candidate.batch, {
        candidateId: candidate.id,
        ephemeral: true,
        comparison: session.compareCandidates(candidate.id),
      });
    }
    if (command === "creature-retarget") {
      const request = z
        .strictObject({
          workspaceKey: key,
          source: idSchema,
          target: idSchema,
          retarget: creatureRetargetSchema,
        })
        .parse(input);
      const source = character(request.source),
        target = character(request.target);
      const result = retargetCreatureMotion(source, target, request.retarget);
      const motions = target.motions.filter((motion) => motion.id !== result.motion.id);
      motions.push(result.motion);
      const operations: EditBatch["operations"] = [
        { kind: "document.set", target: target.id, path: ["motions"], value: motions },
      ];
      if (result.contacts.length && !target.creature)
        throw Error("Initialize target creature source before retargeting contacts");
      for (const value of result.contacts)
        operations.push({ kind: "creature.contact", target: target.id, value });
      return proposal(
        {
          expectedRevision: 0,
          actor: "creature-retarget",
          intent: `Retarget ${source.id}/${request.retarget.motion} to ${target.id}`,
          operations,
        },
        result.provenance,
      );
    }
    if (command === "creature-motion") {
      const request = z
        .strictObject({ workspaceKey: key, target: idSchema, clip: creatureClipSourceSchema })
        .parse(input);
      const target = character(request.target),
        motion = authorCreatureMotion(target, request.clip);
      const motions = target.motions.filter((value) => value.id !== motion.id);
      motions.push(motion);
      return proposal(
        {
          expectedRevision: 0,
          actor: "creature-motion",
          intent: `Author ${motion.id} from named poses`,
          operations: [{ kind: "document.set", target: target.id, path: ["motions"], value: motions }],
        },
        { poses: request.clip.poses.map((pose) => pose.id), motion: motion.id, keys: motion.keys.length },
      );
    }
    throw Error(`Unknown authoring command: ${command}`);
  }
  throw new Error(`Unknown authoring command: ${command}`);
}
if (import.meta.main) {
  try {
    console.log(JSON.stringify(await runAuthoring(process.argv.slice(2)), null, 2));
  } catch (error) {
    console.error(JSON.stringify(authoringError(error)));
    process.exitCode = 1;
  }
}
