import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { type CharacterDefinition, creatureSchema } from "@wrela/model";
import type { CreatureSolveJobRequest } from "./creature-jobs";
import { AuthoringSession, RevisionConflict } from "./session";

function fixture() {
  const project = referenceProject(),
    character = project.documents.find((doc) => doc.kind === "character") as CharacterDefinition;
  character.creature = creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      {
        id: "body",
        name: "Body",
        nodeIds: ["body"],
        frame: { position: [0, 0.9, 0], rotation: [0, 0, 0] },
        extent: [0.52, 0.68, 0.43],
      },
    ],
  });
  return new AuthoringSession(project);
}
function request(id: string): CreatureSolveJobRequest {
  return {
    request: {
      id,
      target: "polar-bunny",
      expectedRevision: 0,
      controls: [{ kind: "regionScale", region: "body", axis: 0, minimum: 0.8, maximum: 1.6 }],
      objectives: [{ kind: "regionExtent", region: "body", axis: 0, value: 0.7, tolerance: 0.00001 }],
      budget: { evaluations: 32, iterations: 8 },
    },
    deadlineMs: 2000,
    evaluationsPerSlice: 1,
  };
}
test("async solve yields actual evaluation progress and cancellation prevents any candidate or source publication", async () => {
  const session = fixture(),
    before = session.export(),
    progress: number[] = [];
  const stop = session.subscribe(() => {
    const job = session.inspectCreatureSolveJob("cancel-after-work");
    if (job?.status !== "running") return;
    progress.push(job.progress.evaluations);
    if (job.progress.evaluations >= 1) session.cancelCreatureSolveJob(job.id);
  });
  try {
    const started = session.startCreatureSolveJob(request("cancel-after-work"));
    expect(started.status).toBe("queued");
    expect(started.progress.evaluations).toBe(0);
    let eventLoopRan = false;
    setTimeout(() => {
      eventLoopRan = true;
    }, 0);
    const result = await session.awaitCreatureSolveJob(started.id);
    expect(result.status).toBe("cancelled");
    expect(result.progress.evaluations).toBe(1);
    expect(eventLoopRan).toBe(true);
    expect(progress).toContain(0);
    expect(progress).toContain(1);
    expect(session.inspectCandidate(started.id)).toBeUndefined();
    expect(session.export()).toBe(before);
    expect(session.creatureSolveJobUsage().retainedSourceBytes).toBe(0);
  } finally {
    stop();
  }
});
test("a paused continuation resumes from its existing work and matches synchronous solving", async () => {
  const session = fixture(),
    sync = fixture(),
    input = request("resumable");
  let paused = false;
  const stop = session.subscribe(() => {
    const job = session.inspectCreatureSolveJob(input.request.id);
    if (!paused && job?.status === "running" && job.progress.evaluations === 1) {
      paused = true;
      session.pauseCreatureSolveJob(job.id);
      expect(session.inspectCreatureSolveJob(job.id)?.progress.evaluations).toBe(1);
      setTimeout(() => session.resumeCreatureSolveJob(job.id), 0);
    }
  });
  try {
    session.startCreatureSolveJob(input);
    const job = await session.awaitCreatureSolveJob(input.request.id),
      expected = sync.solveCreature(input.request);
    expect(paused).toBe(true);
    expect(job.status).toBe("completed");
    expect(job.candidateId).toBe(input.request.id);
    expect(job.result?.parameters).toEqual(expected.parameters);
    expect(job.result?.evaluations).toBe(expected.evaluations);
    expect(session.getSnapshot().revision).toBe(0);
    expect(session.getSnapshot().jobRevision).toBeGreaterThan(0);
    session.adoptCandidate(job.candidateId ?? "");
    expect(session.inspectCreature("polar-bunny").regions[0].extent[0]).toBeCloseTo(0.7);
  } finally {
    stop();
  }
});
test("deadlines include paused time and terminal jobs cannot restart or publish", async () => {
  const session = fixture(),
    input = { ...request("deadline"), deadlineMs: 15 };
  session.startCreatureSolveJob(input);
  session.pauseCreatureSolveJob(input.request.id);
  const job = await session.awaitCreatureSolveJob(input.request.id);
  expect(job.status).toBe("deadline-exceeded");
  expect(job.progress.evaluations).toBe(0);
  expect(job.finishedAt).toBeDefined();
  expect(session.inspectCandidate(job.id)).toBeUndefined();
  expect(() => session.resumeCreatureSolveJob(job.id)).toThrow("paused");
  expect(session.cancelCreatureSolveJob(job.id)).toEqual(job);
});
test("source edits during yielded work preserve a stale result instead of silently rebasing a candidate", async () => {
  const session = fixture(),
    input = request("stale"),
    sourceKey = session.getSnapshot().project;
  let changed = false;
  const stop = session.subscribe(() => {
    const job = session.inspectCreatureSolveJob(input.request.id);
    if (!changed && job?.status === "running" && job.progress.evaluations >= 1) {
      changed = true;
      session.apply({
        expectedRevision: 0,
        operations: [{ kind: "document.rename", target: "polar-bunny", name: "Changed during solve" }],
      });
    }
  });
  try {
    session.startCreatureSolveJob(input);
    const job = await session.awaitCreatureSolveJob(input.request.id);
    expect(changed).toBe(true);
    expect(job.status).toBe("stale");
    expect(job.sourceRevision).toBe(0);
    expect(job.result?.status).toBe("converged");
    expect(job.result?.batch.expectedRevision).toBe(0);
    expect(session.inspectCandidate(job.id)).toBeUndefined();
    expect(session.getSnapshot().project).not.toBe(sourceKey);
    expect(session.startCreatureSolveJob(input)).toEqual(job); // retry works even though live revision changed
    expect(() => session.startCreatureSolveJob({ ...input, deadlineMs: 100 })).toThrow("different work");
    if (!job.result) throw Error("Missing retained solve result");
    const batch = job.result.batch;
    expect(() => session.proposeCandidate({ id: "stale-copy", batch })).toThrow(RevisionConflict);
  } finally {
    stop();
  }
});
test("completed async candidates retain source preconditions through later adoption", async () => {
  const session = fixture(),
    input = request("reviewable");
  session.startCreatureSolveJob(input);
  const job = await session.awaitCreatureSolveJob(input.request.id);
  expect(job.status).toBe("completed");
  session.apply({
    expectedRevision: 0,
    operations: [{ kind: "document.rename", target: "snow-fur", name: "New surface" }],
  });
  expect(() => session.adoptCandidate(job.candidateId ?? "")).toThrow(RevisionConflict);
  expect(session.startCreatureSolveJob(input)).toEqual(job);
});
test("concurrent work and retained identities are bounded while all queued jobs receive fair progress", async () => {
  const session = fixture();
  let maximumRunning = 0;
  const stop = session.subscribe(() => {
    maximumRunning = Math.max(maximumRunning, session.creatureSolveJobUsage().runningJobs);
  });
  try {
    for (let i = 0; i < 16; i++) session.startCreatureSolveJob(request(`queued-${i}`));
    expect(() => session.startCreatureSolveJob(request("too-many"))).toThrow("retention budget");
    expect(() => session.releaseCreatureSolveJob("queued-0")).toThrow("Cancel or complete");
    const jobs = await Promise.all(
      Array.from({ length: 16 }, (_, i) => session.awaitCreatureSolveJob(`queued-${i}`)),
    );
    expect(jobs.every((job) => job.status === "completed")).toBe(true);
    expect(maximumRunning).toBe(4);
    expect(session.creatureSolveJobUsage()).toMatchObject({
      retainedJobs: 16,
      runningJobs: 0,
      retainedSourceBytes: 0,
    });
    session.releaseCreatureSolveJob("queued-0");
    expect(() => session.startCreatureSolveJob(request("queued-0"))).toThrow("released");
    session.startCreatureSolveJob(request("replacement"));
    expect((await session.awaitCreatureSolveJob("replacement")).status).toBe("completed");
  } finally {
    stop();
  }
});
test("replacing a project cancels retained continuations and invalid controls finish as failed jobs", async () => {
  const session = fixture();
  session.startCreatureSolveJob(request("replace"));
  session.replace(referenceProject());
  expect((await session.awaitCreatureSolveJob("replace")).status).toBe("cancelled");
  const bad = fixture(),
    input = request("bad-controls");
  input.request.controls[0] = { kind: "regionScale", region: "missing", axis: 0, minimum: 0.8, maximum: 1.6 };
  bad.startCreatureSolveJob(input);
  const job = await bad.awaitCreatureSolveJob(input.request.id);
  expect(job.status).toBe("failed");
  expect(job.error).toContain("Unknown region");
  expect(bad.listCandidates()).toHaveLength(0);
});
