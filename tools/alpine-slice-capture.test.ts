import { expect, test } from "bun:test";
import { runInNewContext } from "node:vm";
import { type AlpineFramePoll, captureAlpineFrame } from "./alpine-slice-capture";
import type { BrowserView } from "./browser";

function fakeView(frame: () => Promise<unknown>) {
  const page = {
    fixture: {
      frame,
      progress: () => ({ cameraIndex: 0, phase: "measurement", completed: 7, total: 48, elapsedMs: 10 }),
    },
  };
  let calls = 0;
  const view: Pick<BrowserView, "evaluate"> = {
    async evaluate<T>(expression: string): Promise<T> {
      calls++;
      const value = runInNewContext(expression, { window: page, setTimeout });
      // An evaluate must return immediately, never the complete frame's promise.
      expect(typeof value?.then).not.toBe("function");
      return structuredClone(value) as T;
    },
  };
  return { view, calls: () => calls };
}

test("alpine frames finish through short polling calls and retain the complete sample window", async () => {
  let runs = 0;
  const result = { id: "camera", measurements: Array.from({ length: 48 }, (_, frame) => ({ frame })) };
  const { view, calls } = fakeView(async () => {
    runs++;
    await new Promise((resolve) => setTimeout(resolve, 30));
    return result;
  });
  const polls: AlpineFramePoll[] = [];
  const captured = await captureAlpineFrame(view, 0, {
    timeoutMs: 1000,
    pollMs: 2,
    onProgress: (poll) => {
      polls.push(poll);
    },
  });
  expect(captured.id).toBe(result.id);
  expect(captured.measurements.map((frame) => frame.frame)).toEqual(
    result.measurements.map((frame) => frame.frame),
  );
  expect(runs).toBe(1);
  expect(calls()).toBeGreaterThan(3);
  expect(polls[0].status).toBe("running");
  expect(polls.at(-1)?.status).toBe("ready");
});

test("a rejected browser task reports its failure instead of becoming an unhandled promise", async () => {
  const { view } = fakeView(async () => {
    throw Error("Incomplete alpine scene");
  });
  const polls: AlpineFramePoll[] = [];
  await expect(
    captureAlpineFrame(view, 0, {
      pollMs: 1,
      onProgress: (poll) => {
        polls.push(poll);
      },
    }),
  ).rejects.toThrow("Incomplete alpine scene");
  expect(polls.at(-1)?.status).toBe("failed");
});

test("a stalled frame leaves progress available for its incomplete report and browser cleanup", async () => {
  const { view } = fakeView(() => new Promise(() => {}));
  const polls: AlpineFramePoll[] = [];
  await expect(
    captureAlpineFrame(view, 0, {
      timeoutMs: 15,
      pollMs: 1,
      onProgress: (poll) => {
        polls.push(poll);
      },
    }),
  ).rejects.toThrow("measurement (7/48); measurement incomplete");
  expect(polls.at(-1)?.progress.completed).toBe(7);
  // The task deadline does not trip the browser's process-killing round-trip deadline.
  expect(await view.evaluate<number>("window.fixture.progress().total")).toBe(48);
});
