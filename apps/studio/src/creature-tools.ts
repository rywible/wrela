import { type AuthoringSession, inspectCreatureFields, traceCreatureDependencies } from "@wrela/authoring";
import { type CreatureCaptureMetadata, compareCreatureCaptures } from "./creature-review";

/** Browser and human panels share this facade; no separate UI mutation path. */
export function createCreatureStudioTools(session: AuthoringSession) {
  const character = (target: string) => {
    const document = session.getSnapshot().project.documents.find((item) => item.id === target);
    if (document?.kind !== "character") throw Error("Select a character definition");
    return document;
  };
  return {
    fields: (target: string) => ({
      revision: session.getSnapshot().revision,
      ...inspectCreatureFields(character(target)),
    }),
    dependencies: (
      target: string,
      control: Parameters<typeof traceCreatureDependencies>[1],
      direction?: Parameters<typeof traceCreatureDependencies>[2],
    ) => ({
      revision: session.getSnapshot().revision,
      ...traceCreatureDependencies(character(target), control, direction),
    }),
    inspect: (target: string, region?: string) => session.inspectCreature(target, region),
    select: (target: string, point: [number, number, number], radius?: number) =>
      session.selectCreature(target, point, radius),
    explain: (target: string, region: string) => session.explainCreature(target, region),
    solve: (request: Parameters<AuthoringSession["solveCreature"]>[0]) => session.solveCreature(request),
    repair: (request: Parameters<AuthoringSession["repairCreature"]>[0]) => session.repairCreature(request),
    jobs: {
      start: (request: Parameters<AuthoringSession["startCreatureSolveJob"]>[0]) =>
        session.startCreatureSolveJob(request),
      inspect: (id: string) => session.inspectCreatureSolveJob(id),
      list: () => session.listCreatureSolveJobs(),
      pause: (id: string) => session.pauseCreatureSolveJob(id),
      resume: (id: string) => session.resumeCreatureSolveJob(id),
      cancel: (id: string) => session.cancelCreatureSolveJob(id),
      await: (id: string) => session.awaitCreatureSolveJob(id),
      release: (id: string) => session.releaseCreatureSolveJob(id),
      usage: () => session.creatureSolveJobUsage(),
    },
    candidates: {
      propose: (request: Parameters<AuthoringSession["proposeCandidate"]>[0]) =>
        session.proposeCandidate(request),
      list: () => session.listCandidates(),
      inspect: (id: string) => session.inspectCandidate(id),
      compare: (left: string, right?: string) => session.compareCandidates(left, right),
      adopt: (id: string, options?: Parameters<AuthoringSession["adoptCandidate"]>[1]) =>
        session.adoptCandidate(id, options),
      cancel: (id: string) => session.cancelCandidate(id),
      release: (id: string) => session.releaseCandidate(id),
    },
    compareCaptures: (baseline: CreatureCaptureMetadata, candidate: CreatureCaptureMetadata) =>
      compareCreatureCaptures(baseline, candidate),
  };
}
