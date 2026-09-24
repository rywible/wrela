import type { BrowserView } from "./browser";
import type { createAlpineSliceFixture } from "./fixtures/alpine-slice-lookdev";

type Fixture = Awaited<ReturnType<typeof createAlpineSliceFixture>>;
export type AlpineFrameResult = Awaited<ReturnType<Fixture["frame"]>>;
export type AlpineFrameProgress = ReturnType<Fixture["progress"]>;
export type AlpineFramePoll = {
  status: "running" | "ready" | "failed";
  progress: AlpineFrameProgress;
  error?: string;
};

/** Keep browser round trips short while one unchanged measurement window runs in the page. */
export async function captureAlpineFrame(
  view: Pick<BrowserView, "evaluate">,
  index: number,
  options: {
    timeoutMs?: number;
    pollMs?: number;
    onProgress?: (poll: AlpineFramePoll) => Promise<void> | void;
  } = {},
): Promise<AlpineFrameResult> {
  if (!Number.isInteger(index) || index < 0) throw Error("Invalid alpine camera index");
  const timeoutMs = options.timeoutMs ?? 120_000;
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw Error("Invalid alpine frame timeout");
  const started = performance.now();
  await view.evaluate(`(() => {
    if (window.alpineFrameJob?.status === "running") throw Error("Alpine frame already running");
    const job = window.alpineFrameJob = { status: "running" };
    setTimeout(() => {
      Promise.resolve().then(() => window.fixture.frame(${index})).then(
        result => { job.result = result; job.status = "ready"; },
        error => { job.error = String(error); job.status = "failed"; }
      );
    }, 0);
    return true;
  })()`);
  while (true) {
    const poll = await view.evaluate<AlpineFramePoll>(`({
      status: window.alpineFrameJob.status,
      error: window.alpineFrameJob.error,
      progress: window.fixture.progress()
    })`);
    await options.onProgress?.(poll);
    if (poll.status === "failed") throw Error(poll.error ?? "Alpine frame failed");
    if (poll.status === "ready")
      return await view.evaluate<AlpineFrameResult>(`(() => {
        const result = window.alpineFrameJob.result;
        delete window.alpineFrameJob;
        return result;
      })()`);
    if (performance.now() - started >= timeoutMs)
      throw Error(
        `Alpine camera ${index} exceeded ${timeoutMs} ms during ${poll.progress.phase} ` +
          `(${poll.progress.completed}/${poll.progress.total}); measurement incomplete`,
      );
    await new Promise((resolve) => setTimeout(resolve, options.pollMs ?? 500));
  }
}
