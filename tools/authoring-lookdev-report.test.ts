import { expect, test } from "bun:test";
import {
  AUTHORING_STUDIES,
  type AuthoringLookdevReport,
  authoringAcceptance,
  renderAuthoringLookdev,
} from "./authoring-lookdev-report";

const report = (): AuthoringLookdevReport => ({
  sourceFingerprint: "test-source",
  startedAt: "2026-09-22",
  studies: AUTHORING_STUDIES.map((s) => ({
    id: s.id,
    exitCode: 0,
    durationSeconds: 1,
    artifacts: [`${s.id}.png`],
    log: `${s.id}.log`,
    visualAcceptance: "unreviewed",
    observations: [],
  })),
});
test("successful hardware captures never automatically approve visual quality", () => {
  expect(authoringAcceptance(report())).toMatchObject({
    complete: true,
    accepted: false,
    pending: AUTHORING_STUDIES.map((s) => s.id),
  });
});
test("missing categories, failed tools and missing images prevent acceptance", () => {
  const value = report();
  for (const s of value.studies) s.visualAcceptance = "accepted";
  expect(authoringAcceptance(value).accepted).toBe(true);
  value.studies[0].exitCode = 1;
  value.studies[1].artifacts = [];
  value.studies.pop();
  expect(authoringAcceptance(value)).toMatchObject({
    complete: false,
    accepted: false,
    missing: ["environment"],
    failed: ["creatures", "assembly"],
  });
});
test("gallery safely renders reviewer notes and links to inspectable evidence", () => {
  const value = report();
  value.studies[0].observations = ['<script>alert("bad")</script>'];
  value.studies[0].artifacts = ["source/output/face closeup.png"];
  const html = renderAuthoringLookdev(value);
  expect(html).not.toContain("<script>");
  expect(html).toContain("&lt;script&gt;");
  expect(html).toContain("source/output/face%20closeup.png");
  expect(html).toContain("visual review remains open");
});
