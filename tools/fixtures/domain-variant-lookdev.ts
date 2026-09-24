import { AuthoringSession } from "@wrela/authoring";
import type { DomainVariantStudy } from "../domain-variant-studies";
import { createLookdevFixture } from "./lookdev";

/** Every capture starts from the same accepted source and uses ordinary candidate adoption. */
export function createDomainVariantLookdevFixture() {
  let current: Awaited<ReturnType<typeof createLookdevFixture>> | undefined;
  return {
    async capture(study: DomainVariantStudy, variantIndex: number) {
      if (
        !Number.isInteger(variantIndex) ||
        variantIndex < -1 ||
        variantIndex >= study.request.variants.length
      )
        throw Error("Choose baseline (-1) or a declared variant");
      current?.dispose();
      current = undefined;
      const requestedAt = performance.now();
      const session = new AuthoringSession(study.lookdev.project);
      const proposal = session.proposeDomainVariants(study.request);
      const proposedAt = performance.now();
      const variant = variantIndex < 0 ? undefined : proposal.variants[variantIndex];
      if (variant) session.adoptCandidate(variant.candidate.id);
      const adoptedAt = performance.now();
      const project = session.getSnapshot().project;
      // New host/renderer each case, but shared in-process compiler caches can be warm.
      current = await createLookdevFixture({ ...study.lookdev, project });
      const preparedAt = performance.now();
      const frames: Awaited<ReturnType<typeof current.frame>>[] = [];
      try {
        for (let index = 0; index < study.lookdev.frames.length; index++)
          frames.push(await current.frame(index));
        return {
          domain: study.domain,
          variant: variant?.id ?? "baseline",
          frames,
          proposal: variant ?? null,
          sourceKey: proposal.sourceKey,
          project,
          variationScope: study.variationScope,
          timing: {
            sourceRequestMs: proposedAt - requestedAt,
            sourcePreparationMs: proposal.sourcePreparationMs,
            adoptionMs: adoptedAt - proposedAt,
            newHostAndRendererPreparationMs: preparedAt - adoptedAt,
            firstFrameMs: frames[0].timing.frameToCompleteMs,
            requestToFirstCompleteFrameMs: frames[0].timing.completeAt - requestedAt,
            allFramesAndReadbackMs: performance.now() - requestedAt,
            scope:
              "Browser-side source request through first complete hardware frame; excludes browser startup, transport and human review. New host/renderer per case; process-level compiler caches may be warm. Capture/readback waiting is included, not a sustained frame-rate measurement.",
          },
          economics: { agentTokenCost: null, manualInterventions: null, artisticAccepted: null },
          visualAcceptance: "unreviewed",
        };
      } finally {
        current.dispose();
        current = undefined;
      }
    },
    dispose() {
      current?.dispose();
      current = undefined;
    },
  };
}
