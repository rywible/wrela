/** Let input/render work run between compiler slices. Where supported, a real
 * background task avoids nested-timer clamping without raising work priority. */
export function yieldCompilation(): Promise<void> {
  const scheduler = (
    globalThis as unknown as {
      scheduler?: { postTask?: (callback: () => void, options: { priority: "background" }) => Promise<void> };
    }
  ).scheduler;
  return scheduler?.postTask
    ? scheduler.postTask(() => {}, { priority: "background" })
    : new Promise<void>((resolve) => setTimeout(resolve, 0));
}
