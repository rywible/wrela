import { expect, test } from "bun:test";
import { AuthoringSession } from "@wrela/authoring";
import { canonical } from "@wrela/model";

import { createDomainVariantStudy, DOMAIN_VARIANT_DOMAINS } from "./domain-variant-studies";

test("production studies generate three ordinary source variants in every domain and keep matched cameras", () => {
  for (const domain of DOMAIN_VARIANT_DOMAINS) {
    const study = createDomainVariantStudy(domain),
      baseline = canonical(study.lookdev.project);
    const session = new AuthoringSession(study.lookdev.project);
    const result = session.proposeDomainVariants(study.request);
    expect(result.variants).toHaveLength(3);
    expect(new Set(result.variants.map((variant) => variant.review.candidateKey)).size).toBe(3);
    expect(study.lookdev.frames.length).toBeGreaterThanOrEqual(3);
    for (const variant of result.variants) {
      expect(variant.constraints.every((constraint) => constraint.passed)).toBe(true);
      const isolated = new AuthoringSession(study.lookdev.project);
      isolated.apply(variant.candidate.batch);
      const saved = isolated.export();
      expect(new AuthoringSession(JSON.parse(saved)).export()).toBe(saved);
      expect(variant.review.status).toBe("unreviewed");
    }
    expect(canonical(study.lookdev.project)).toBe(baseline);
    if (domain === "performance")
      expect(study.lookdev.frames.every((frame) => frame.motion?.id === "walk")).toBe(true);
  }
});

test("creature fitting variants select the common shoulder scope without weakening shared-control guards", () => {
  const study = createDomainVariantStudy("creatures"),
    session = new AuthoringSession(study.lookdev.project);
  const character = study.lookdev.project.documents.find((document) => document.id === study.lookdev.subject);
  if (character?.kind !== "character" || !character.creature) throw Error("Missing fitted anatomy");
  const mantle = character.creature.cloth.find((cloth) => cloth.id === "pilgrim-mantle");
  if (!mantle?.fittingLandmarks) throw Error("Missing live mantle fitting landmarks");
  const fittingLandmarks = mantle.fittingLandmarks;
  expect(() =>
    session.proposeDomainVariants({
      id: "child-only",
      expectedRevision: 0,
      variants: [
        {
          id: "widen",
          recipe: {
            kind: "creature.landmarkSpan",
            target: character.id,
            first: fittingLandmarks[0],
            second: fittingLandmarks[1],
            distance: 0.8,
            descendants: true,
          },
        },
      ],
    }),
  ).toThrow("Shared controls overlap unselected region shoulder");
  expect(session.listCandidates()).toHaveLength(0);
  const variants = session.proposeDomainVariants(study.request).variants;
  for (const variant of variants) {
    const operation = variant.candidate.batch.operations[0];
    expect(operation.kind).toBe("creature.proportion");
    if (operation.kind !== "creature.proportion") throw Error("Missing common-scope proportion edit");
    expect(operation.region).toBe("shoulder");
    expect(operation.propagate).toBe("descendants");
  }
});
