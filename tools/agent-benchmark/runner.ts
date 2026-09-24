import { mkdir, open, realpath, rename } from "node:fs/promises";
import { isAbsolute, join, relative, resolve } from "node:path";
import { authoringTaskContext, createWorkSession } from "@wrela/authoring";
import { contentKey, type Project, parseProject } from "@wrela/model";

import { z } from "zod";
import { invokeAuthoringAgent } from "../authoring-agent";
import { runAuthoringReview } from "../authoring-review-runner";
import { invokeAuthoringSession } from "../authoring-service";
import { readAuthoringTrace, summarizeAuthoringTrace } from "../authoring-trace";
import { WorkFileStore } from "../authoring-work-store";
import { WorkspaceBridge } from "../bridge";
import { sourceManifest } from "../evidence";
import { authoringDeadlines } from "./deadlines";
import { evaluateNative } from "./native-evaluation";
import { type BenchmarkAttempt, benchmarkAttemptSchema, benchmarkSuiteSchema } from "./protocol";

export const agentRunnerSchema = z.strictObject({
  engine: z.enum(["wrela", "blender", "unreal"]),
  agent: z.string().min(1),
  model: z.string().min(1),
  command: z.array(z.string()).min(1),
  execution: z.enum(["agent", "scripted-smoke"]).default("agent"),
  task: z.string(),
  repetition: z.number().int().nonnegative().default(0),
  nativeEvaluator: z.array(z.string()).min(1).optional(),
  prepareJob: z.boolean().default(false),
  warmSession: z.boolean().default(false),
});
export async function engineAvailability() {
  const exists = async (path: string | null) => (path && (await Bun.file(path).exists()) ? path : null);
  return {
    wrela: { executable: process.execPath, available: true },
    blender: await (async () => {
      const executable = await exists(
        process.env.BLENDER_PATH ??
          Bun.which("blender") ??
          "/Applications/Blender.app/Contents/MacOS/Blender",
      );
      return { executable, available: !!executable };
    })(),
    unreal: await (async () => {
      const executable = await exists(
        process.env.UNREAL_EDITOR_PATH ?? Bun.which("UnrealEditor-Cmd") ?? null,
      );
      return { executable, available: !!executable };
    })(),
    scope: "Executable discovery only. Native adapter execution and render validation remain required.",
  };
}
const submissionSchema = z.strictObject({
  tokens: z.number().int().nonnegative().nullable().default(null),
  humanInterventions: z.number().int().nonnegative().nullable().default(null),
  constraintsPassed: z.boolean().nullable().default(null),
  artifacts: z
    .array(
      z.strictObject({
        path: z.string(),
        kind: z.enum(["source", "image", "review", "transcript", "handoff"]),
      }),
    )
    .min(1),
  stages: z
    .array(z.strictObject({ name: z.string(), durationMs: z.number().finite().nonnegative() }))
    .default([]),
  reason: z.string().optional(),
});
export async function hashBenchmarkArtifact(directory: string, path: string) {
  const root = await realpath(directory),
    target = await realpath(resolve(root, path));
  const rel = relative(root, target);
  if (isAbsolute(rel) || rel === ".." || rel.startsWith("../"))
    throw Error("Submitted artifacts must remain inside the attempt directory");
  const file = Bun.file(target);
  if (file.size > 128 * 1024 * 1024) throw Error("Benchmark artifact exceeds 128 MiB");
  return { path: rel, sha256: new Bun.CryptoHasher("sha256").update(await file.arrayBuffer()).digest("hex") };
}
export async function runBenchmarkAttempt(suiteDirectory: string, raw: z.input<typeof agentRunnerSchema>) {
  const config = agentRunnerSchema.parse(raw),
    root = resolve(suiteDirectory);
  const suite = benchmarkSuiteSchema.parse(await Bun.file(join(root, "suite.json")).json());
  if (config.prepareJob && config.engine !== "wrela")
    throw Error("Prepared dispatch requires the Wrela authoring job API");
  const task = suite.tasks.find((t) => t.id === config.task);
  if (!task || !task.engines.includes(config.engine) || config.repetition >= suite.repetitions)
    throw Error("Invalid benchmark cell");
  if (config.model !== suite.policy.model && config.execution === "agent")
    throw Error("Agent model must match the preregistered suite");
  const id = `${task.id}-${config.engine}-${config.repetition}`,
    directory = join(root, "attempts", id);
  if (await Bun.file(join(directory, "attempt.json")).exists())
    throw Error("Attempt already exists; failed attempts cannot be overwritten");
  await mkdir(directory, { recursive: true });
  const reservation = await open(join(directory, "RUNNING"), "wx");
  try {
    await reservation.writeFile(JSON.stringify({ pid: process.pid, startedAt: new Date().toISOString() }));
  } finally {
    await reservation.close();
  }
  const started = Date.now(),
    source = await sourceManifest("agent-attempt-before"),
    availability = await engineAvailability();
  const attempt: BenchmarkAttempt = {
    version: 1,
    id,
    suiteKey: contentKey(suite),
    task: task.id,
    engine: config.engine,
    repetition: config.repetition,
    agent: config.agent,
    model: config.model,
    execution: config.execution,
    dispatch: config.prepareJob ? "engine-started" : "agent-started",
    status: "running",
    startedAt: new Date(started).toISOString(),
    endedAt: new Date(started).toISOString(),
    elapsedMs: 0,
    tokens: null,
    humanInterventions: null,
    engineEdits: [],
    constraintsPassed: null,
    artifacts: [],
    stages: [],
  };
  await Bun.write(join(directory, "attempt.json"), JSON.stringify(attempt, null, 2));
  let service: ReturnType<typeof Bun.spawn> | undefined;
  const sessionPath = join(directory, "session.json");
  const closeService = async () => {
    if (!service) return;
    try {
      const s = await Bun.file(sessionPath).json();
      await fetch(`${s.url}/close`, {
        method: "POST",
        headers: { authorization: `Bearer ${s.token}` },
        signal: AbortSignal.timeout(1000),
      });
    } catch {
      /* Kill the owned service below if startup failed. */
    }
    const child = service;
    service = undefined;
    const timer = setTimeout(() => child.kill(), 5000);
    try {
      await child.exited;
    } finally {
      clearTimeout(timer);
    }
  };
  try {
    if (source.sourceFingerprint !== suite.sourceFingerprint)
      throw Error(
        "Engine source differs from the registered suite; prepare a new suite against a frozen source snapshot",
      );
    if (!availability[config.engine].available) {
      attempt.status = "unavailable";
      attempt.reason = "Engine executable was not found";
    } else {
      if (config.warmSession) {
        if (config.engine !== "wrela") throw Error("Warm review sessions require Wrela");
        service = Bun.spawn([process.execPath, "tools/authoring-service.ts", sessionPath], {
          cwd: resolve("."),
          stdout: Bun.file(join(directory, "session.log")),
          stderr: Bun.file(join(directory, "session-errors.log")),
        });
        const deadline = Date.now() + 60000;
        while (!(await Bun.file(sessionPath).exists())) {
          if (Date.now() > deadline || service.exitCode !== null)
            throw Error("Warm authoring service did not become ready");
          await Bun.sleep(50);
        }
        attempt.stages.push({
          name: "warm-session-startup",
          durationMs: Date.now() - started,
          origin: "runner",
        });
      }
      let prior: { directory: string; attempt: BenchmarkAttempt } | undefined;
      if (task.prerequisite) {
        const priorDirectory = join(
          root,
          "attempts",
          `${task.prerequisite}-${config.engine}-${config.repetition}`,
        );
        const file = Bun.file(join(priorDirectory, "attempt.json"));
        if (!(await file.exists())) throw Error("Handoff requires the preceding revision attempt");
        const value = benchmarkAttemptSchema.parse(await file.json());
        if (value.status !== "completed") throw Error("Handoff prerequisite did not complete");
        prior = { directory: priorDirectory, attempt: value };
      }
      const workspace = join(directory, "workspace");
      let heldBaseline: Project | undefined;
      let initialWorkKey: string | undefined;
      if (config.engine === "wrela") {
        const baseline = prior
          ? (await new WorkspaceBridge(join(prior.directory, "workspace")).read())?.project
          : await Bun.file(join(root, task.baseline)).json();
        if (!baseline) throw Error("Handoff source is missing");
        const project = parseProject(baseline),
          bridge = new WorkspaceBridge(workspace);
        await bridge.initialize();
        await bridge.save(project, null);
        heldBaseline = structuredClone(project);
        initialWorkKey = await new WorkFileStore(workspace).put(
          createWorkSession(project, { id: task.workId, brief: task.brief, constraints: task.constraints }),
          null,
        );
      }
      const jobInvocation = join(directory, "job-invocation.json");
      if (heldBaseline)
        await Bun.write(
          jobInvocation,
          JSON.stringify(
            {
              workspace,
              work: task.workId,
              action: "job",
              input: {
                target: task.subject ?? heldBaseline.entry,
                expectedKey: initialWorkKey,
                budgetSeconds: 30,
              },
            },
            null,
            2,
          ),
        );
      // Generation begins from the registered task brief, inside the original clock.
      // A fresh agent receives the images, makes the visual choice, and publishes.
      const prepared =
        config.prepareJob && heldBaseline
          ? config.warmSession
            ? await invokeAuthoringSession(sessionPath, jobInvocation)
            : await invokeAuthoringAgent(await Bun.file(jobInvocation).json())
          : undefined;
      const request = {
        version: 1,
        suiteKey: contentKey(suite),
        task,
        policy: suite.policy,
        availability: availability[config.engine],
        workspace,
        baseline: join(root, task.baseline),
        brief: join(root, task.id, "brief.json"),
        neutralStarter: join(root, task.id, "neutral-starter.json"),
        predecessor: prior ?? null,
        output: directory,
        submission: join(directory, "submission.json"),
        initialWorkKey,
        preparedJob: prepared,
        warmSessionPath: config.warmSession ? sessionPath : undefined,
        taskContext:
          heldBaseline && heldBaseline.documents.some((d) => d.id === (task.subject ?? heldBaseline!.entry))
            ? authoringTaskContext(
                heldBaseline,
                task.subject ??
                  (heldBaseline.documents.some((d) => d.id === "benchmark-gate")
                    ? "benchmark-gate"
                    : heldBaseline.entry),
                task.constraints,
              )
            : undefined,
        fastWorkflow: {
          jobInvocation: heldBaseline ? jobInvocation : undefined,
          job: `bun tools/authoring-agent.ts ${jobInvocation}`,
          context: `bun run author ${workspace} work context ${task.workId} ${task.subject ?? "benchmark-gate"}`,
          study: `bun run author ${workspace} work study ${task.workId} request.json`,
          agentTool: config.warmSession
            ? `Use bun tools/authoring-service.ts invoke ${sessionPath} invocation.json for every authoring call. This reuses the already-started renderer. Responses include result, images and service timing. Display the images before choosing.`
            : "bun tools/authoring-agent.ts invocation.json returns {result,images}; display images in the same host tool invocation. Native MCP is available through bun tools/authoring-agent.ts --mcp.",
          evaluate: `bun run author ${workspace} work evaluate ${task.workId} request.json`,
          finish: `bun run author ${workspace} work finish ${task.workId} finish.json`,
          note: "Use task brief cameras in evaluate.views; finish accepts benchmarkRequest pointing to this request file and atomically creates submission.json. Inspect the contact sheet before finish. Never invent model/token counts.",
        },
        instruction:
          "Start a fresh agent context. Use the best programmatic APIs. Respect the declared asset policy, time/token limits and content/tool-development track. Record failed iterations, tokens and human interventions. Inspect final images. Do not invent artistic acceptance or measurements. Finish with benchmarkRequest pointing to this request to create submission.json atomically; no repository documentation is required.",
      };
      await Bun.write(join(directory, "request.json"), JSON.stringify(request, null, 2));
      await Bun.write(
        join(directory, "agent-start.json"),
        JSON.stringify(
          {
            brief: task.brief,
            acceptance: task.acceptance,
            budget: task.budget,
            cwd: resolve("."),
            workspace,
            work: task.workId,
            jobInvocation: heldBaseline ? jobInvocation : undefined,
            benchmarkRequest: join(directory, "request.json"),
            preparedJob: prepared,
            warmSessionPath: config.warmSession ? sessionPath : undefined,
            invocationCommand: config.warmSession
              ? [process.execPath, "tools/authoring-service.ts", "invoke", sessionPath]
              : [process.execPath, "tools/authoring-agent.ts"],
            workflow: prepared
              ? "The registered brief already generated reviewed alternatives inside the benchmark clock. Display preparedJob.images, make your visual selection, then run bun tools/authoring-agent.ts --finish-job <preparedJob.result.dispatch> <chosen-proposal> <benchmarkRequest>. You may refine through the normal job API; no automatic art acceptance is inferred."
              : !request.taskContext
                ? "This is a blank-workspace hero creation task. Author your own component graph with action hero, inspect the returned images, iterate as needed, then finish with benchmarkRequest. Do not use completed starting designs or benchmark fixtures. Generic schema and compiler capabilities are allowed."
                : "Run the supplied job invocation to get matched baseline and candidate images. Display images, inspect them, then use result.finish (or another passing proposal) with action finish and benchmarkRequest. The full request retains constraints and schemas for further edits. Do not infer art approval from constraints or recommendation.",
          },
          null,
          2,
        ),
      );
      if (prepared)
        console.log(
          JSON.stringify({
            reviewReady: join(directory, "agent-start.json"),
            brief: task.brief,
            workspace,
            work: task.workId,
            images: prepared.images,
            dispatch: prepared.result.dispatch,
            candidates: prepared.result.candidates,
            benchmarkRequest: join(directory, "request.json"),
          }),
        );
      const agentProcess = Bun.spawn([...config.command, join(directory, "request.json")], {
        cwd: resolve("."),
        stdout: Bun.file(join(directory, "transcript.log")),
        stderr: Bun.file(join(directory, "stderr.log")),
        env: { ...globalThis.process.env, WRELA_BENCHMARK_REQUEST: join(directory, "request.json") },
      });
      let timedOut = false;
      const timer = setTimeout(
        () => {
          timedOut = true;
          agentProcess.kill("SIGKILL");
        },
        Math.max(1, task.budget.seconds * 1000 - (Date.now() - started)),
      );
      let code: number;
      try {
        code = await agentProcess.exited;
      } finally {
        clearTimeout(timer);
      }
      await closeService();
      if (timedOut) {
        attempt.status = "timeout";
        attempt.reason = "Declared wall-clock budget exceeded";
      } else if (code !== 0) {
        attempt.status = "failed";
        attempt.reason = `Agent runner exited ${code}`;
      } else {
        const submission = submissionSchema.parse(await Bun.file(join(directory, "submission.json")).json());
        attempt.tokens = submission.tokens;
        attempt.humanInterventions = submission.humanInterventions;
        attempt.constraintsPassed = null;
        let submittedSourceKey: string | undefined;
        if (config.engine === "wrela") {
          const published = await new WorkspaceBridge(workspace).read();
          if (!published || !heldBaseline) throw Error("Submitted source workspace is missing");
          if (published.key === contentKey(heldBaseline)) throw Error("Task submitted unchanged source");
          submittedSourceKey = published.key;
          const evaluated = await runAuthoringReview(
            heldBaseline,
            published.project,
            task.constraints,
            join(directory, "independent-review"),
            task.constraints.some((c) => c.kind === "silhouette" || c.kind === "resources"),
          );
          await Bun.write(join(directory, "independent-review.json"), JSON.stringify(evaluated, null, 2));
          attempt.constraintsPassed =
            evaluated.results.length === task.constraints.length &&
            evaluated.results.every((r) => r.status === "passed");
        }
        for (const artifact of submission.artifacts)
          attempt.artifacts.push({
            ...(await hashBenchmarkArtifact(directory, artifact.path)),
            kind: artifact.kind,
          });
        if (
          !attempt.artifacts.some((a) => a.kind === "source") ||
          !attempt.artifacts.some((a) => a.kind === "image")
        )
          throw Error("Completed attempts require editable source and rendered image artifacts");
        if (submittedSourceKey) {
          let matchingSource = false;
          for (const artifact of attempt.artifacts.filter((a) => a.kind === "source")) {
            try {
              if (
                contentKey(parseProject(await Bun.file(join(directory, artifact.path)).json())) ===
                submittedSourceKey
              )
                matchingSource = true;
            } catch {
              /* A source bundle may accompany the canonical project JSON. */
            }
          }
          if (!matchingSource) throw Error("A source artifact must contain the exact published project JSON");
          attempt.artifacts.push({
            ...(await hashBenchmarkArtifact(directory, "independent-review.json")),
            kind: "review",
          });
        }
        if (config.engine !== "wrela" && config.nativeEvaluator) {
          const sources = attempt.artifacts
            .filter((a) => a.kind === "source")
            .map(({ path, sha256 }) => ({ path, sha256 }));
          attempt.constraintsPassed = (
            await evaluateNative(
              config.nativeEvaluator,
              directory,
              contentKey(suite),
              task,
              sources,
              task.budget.seconds,
            )
          ).passed;
          for (const source of sources)
            if ((await hashBenchmarkArtifact(directory, source.path)).sha256 !== source.sha256)
              throw Error("Native evaluator modified the submitted source");
          attempt.artifacts.push({
            ...(await hashBenchmarkArtifact(directory, "native-review.json")),
            kind: "review",
          });
        }
        attempt.stages.push(...submission.stages.map((s) => ({ ...s, origin: "agent-reported" as const })));
        attempt.status = "completed";
        attempt.reason = submission.reason;
        if (attempt.tokens !== null && attempt.tokens > task.budget.tokens) {
          attempt.status = "failed";
          attempt.reason = "Declared token budget exceeded";
        }
      }
    }
  } catch (error) {
    attempt.status = "failed";
    attempt.reason = String(error);
  } finally {
    await closeService();
  }
  const after = await sourceManifest("agent-attempt-after"),
    before = new Map(source.files.map((f) => [f.path, f.sha256])),
    later = new Map(after.files.map((f) => [f.path, f.sha256]));
  attempt.engineEdits = [...new Set([...before.keys(), ...later.keys()])].filter(
    (p) => before.get(p) !== later.get(p),
  );
  if (suite.policy.track === "content" && attempt.engineEdits.length) {
    attempt.status = "failed";
    attempt.reason =
      "Engine source changed during content-only attempt; cannot attribute changes to this agent in a shared checkout";
  }
  attempt.endedAt = new Date().toISOString();
  attempt.elapsedMs = Date.now() - started;
  try {
    const trace = await readAuthoringTrace(join(directory, "workspace"));
    if (trace.length)
      attempt.telemetry = summarizeAuthoringTrace(trace, started, Date.parse(attempt.endedAt));
    attempt.deadlines = authoringDeadlines(trace, started);
  } catch (error) {
    await Bun.write(join(directory, "telemetry-error.txt"), String(error));
  }

  await Bun.write(
    join(directory, "attempt.json.tmp"),
    JSON.stringify(benchmarkAttemptSchema.parse(attempt), null, 2),
  );
  await rename(join(directory, "attempt.json.tmp"), join(directory, "attempt.json"));
  return { directory, attempt };
}
