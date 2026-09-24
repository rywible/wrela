import { contentKey } from "@wrela/model";
import { z } from "zod";
import { domainBriefSchema, planDomainAuthoring } from "./authoring-domain";
import { parseEditRecipe, promoteEditRecipe } from "./edit-recipe";
import type { WorkSession } from "./work-session";

/** Promotion requires an existing explicit visual decision; a constraint pass cannot create approval. */
export async function promoteStudyRecipe(
  work: WorkSession,
  proposal: string,
  id: string,
  load: (ref: string) => Promise<unknown>,
) {
  const recipe = promoteEditRecipe(work, proposal, {
    id,
    description: "Accepted semantic authoring conditions",
    parameters: [],
  });
  for (const event of [...work.events].reverse())
    if (event.actor === "study-workflow")
      for (const ref of event.evidence) {
        let value: unknown;
        try {
          value = await load(ref);
        } catch {
          continue;
        }
        const parsed = z
          .object({
            version: z.literal(1),
            baselineKey: z.string(),
            request: z.object({ brief: domainBriefSchema }),
            candidates: z.array(
              z.object({ proposal: z.string(), conditions: domainBriefSchema.shape.conditions }),
            ),
          })
          .safeParse(value);
        if (!parsed.success || parsed.data.baselineKey !== work.baselineKey) continue;
        const candidate = parsed.data.candidates.find((c) => c.proposal === proposal);
        if (!candidate) continue;
        const semantic = { ...parsed.data.request.brief, conditions: candidate.conditions };
        if (
          contentKey(planDomainAuthoring(work.baseline, semantic, proposal).operations) !==
          contentKey(recipe.operations)
        )
          throw Error("Retained study conditions do not reproduce the accepted source edit");
        return parseEditRecipe({
          ...recipe,
          semantic,
          description: `${semantic.domain}: ${semantic.quality.direction}`,
          provenance: {
            ...recipe.provenance,
            reviewKeys: [...recipe.provenance.reviewKeys, contentKey(value)],
          },
        });
      }
  throw Error("No retained study provenance exists for this proposal");
}
