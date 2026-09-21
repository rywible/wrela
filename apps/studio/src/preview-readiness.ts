import type { RenderCompleteness } from "@wrela/model";

/** A preview is ready only when its requested render products have a declared outcome. */
export function assertRenderAccepted(completeness: RenderCompleteness | undefined) {
  if (!completeness?.rejected.length) return;
  throw new Error(
    `Preview is incomplete: ${completeness.rejected.map((item) => `${item.id}: ${item.reason}`).join("; ")}`,
  );
}
