import { expect, test } from "bun:test";
import type { AuthoringTrace } from "../authoring-trace";
import { authoringDeadlines } from "./deadlines";

test("deadlines distinguish an image, a completed review, and delivered source without inferring art approval", () => {
  const start = 100000;
  const row: AuthoringTrace = {
    version: 1,
    id: "job",
    command: "work.job",
    startedAt: start + 1000,
    endedAt: start + 20000,
    durationMs: 19000,
    ok: true,
    firstCandidateImageAt: start + 8000,
  };
  const rows = [
    row,
    {
      ...row,
      id: "finish",
      command: "work.finish",
      startedAt: start + 31000,
      endedAt: start + 40000,
      durationMs: 9000,
      firstCandidateImageAt: undefined,
    },
  ];
  const checks = authoringDeadlines(rows, start);
  expect(
    checks.map((c) => [
      c.candidateImageAvailable,
      c.reviewedOutputAvailable,
      c.deliveryFinished,
      c.artisticAcceptance,
    ]),
  ).toEqual([
    [true, false, false, null],
    [true, true, false, null],
    [true, true, true, null],
  ]);
  expect(authoringDeadlines([{ ...row, ok: false }], start)[2].reviewedOutputAvailable).toBe(false);
  expect(
    authoringDeadlines([{ ...row, startedAt: start - 1, firstCandidateImageAt: start - 1 }], start)[2]
      .reviewedOutputAvailable,
  ).toBe(false);
});
