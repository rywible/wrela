import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { contentKey } from "@wrela/model";
import { AuthoringSession } from "./session";
import { publishWork, sessionWorkPublisher, type WorkRecordStore } from "./work-publication";
import {
  createWorkSession,
  materializeWorkProposal,
  parseWorkSession,
  proposeWork,
  workHandoff,
} from "./work-session";

function fixture() {
  const project = referenceProject(),
    session = new AuthoringSession(project);
  let work = proposeWork(createWorkSession(project, { id: "publish", brief: "Make coat matte" }), "matte", [
    { kind: "document.set", target: "snow-fur", path: ["roughness"], value: 0.71 },
  ]);
  let writes = 0,
    failAt = 0;
  const store: WorkRecordStore = {
    async get() {
      return parseWorkSession(JSON.parse(JSON.stringify(work)));
    },
    async put(next, key) {
      if (++writes === failAt) throw Error("storage unavailable");
      if (key !== contentKey(work)) throw Error("concurrent work change");
      work = next;
      return contentKey(next);
    },
  };
  return {
    session,
    store,
    input: { id: work.id, expectedKey: contentKey(work), proposal: "matte" },
    fail: (n: number) => {
      failAt = n;
    },
  };
}
test("adoption does not edit source if durable intent cannot be retained", async () => {
  const f = fixture(),
    before = contentKey(f.session.getSnapshot().project);
  f.fail(1);
  await expect(
    publishWork(
      f.store,
      sessionWorkPublisher(() => f.session),
      f.input,
    ),
  ).rejects.toThrow("storage unavailable");
  expect(contentKey(f.session.getSnapshot().project)).toBe(before);
});
test("receipt failure after commit reports success and recovers identical receipt after restart", async () => {
  const f = fixture();
  f.fail(2);
  const first = await publishWork(
    f.store,
    sessionWorkPublisher(() => f.session),
    f.input,
  );
  expect(first.outcome).toBe("committed");
  expect(first.receiptPending).toBe(true);
  const restarted = new AuthoringSession(f.session.getSnapshot().project);
  const retry = await publishWork(
    f.store,
    sessionWorkPublisher(() => restarted),
    f.input,
  );
  expect(retry.receiptPending).toBe(false);
  expect(retry.result).toEqual(first.result);
  expect((await f.store.get(f.input.id)).publication?.state).toBe("committed");
  expect(
    (
      await publishWork(
        f.store,
        sessionWorkPublisher(() => restarted),
        f.input,
      )
    ).result,
  ).toEqual(first.result);
  expect(restarted.getSnapshot().revision).toBe(0);
});
test("pending intent recovers before source commit and concurrent writers cannot adopt twice", async () => {
  const f = fixture(),
    publisher = sessionWorkPublisher(() => f.session);
  await expect(
    publishWork(
      f.store,
      {
        ...publisher,
        async commit() {
          throw Error("interrupted");
        },
      },
      f.input,
    ),
  ).rejects.toThrow("interrupted");
  const outcomes = await Promise.allSettled([
    publishWork(f.store, publisher, f.input),
    publishWork(f.store, publisher, f.input),
  ]);
  expect(outcomes.some((o) => o.status === "fulfilled")).toBe(true);
  expect(f.session.getSnapshot().revision).toBe(1);
});
test("32 alternatives retain one baseline and handoff never evaluates unselected operations", () => {
  let work = createWorkSession(referenceProject(), { id: "alternatives", brief: "Compare coat roughness" });
  const baselineBytes = JSON.stringify(work).length;
  for (let i = 0; i < 32; i++)
    work = proposeWork(work, `p-${i}`, [
      { kind: "document.set", target: "snow-fur", path: ["roughness"], value: (i + 10) / 100 },
    ]);
  expect(JSON.stringify(work).length - baselineBytes).toBeLessThan(24000);
  expect(contentKey(materializeWorkProposal(work, "p-31"))).toBe(work.proposals[31].candidateKey);
  const corrupted = structuredClone(work);
  corrupted.proposals[0].candidateKey = "corrupt";
  expect(workHandoff(corrupted).proposals).toHaveLength(32);
  expect(() => materializeWorkProposal(corrupted, "p-0")).toThrow("does not match");
});

test("source persistence failure retains a pending intent that can replay after losing browser memory", async () => {
  const f = fixture(),
    original = f.session.getSnapshot().project;
  const first = await publishWork(
    f.store,
    {
      ...sessionWorkPublisher(() => f.session),
      async flush() {
        throw Error("disk full");
      },
    },
    f.input,
  );
  expect(first.outcome).toBe("accepted");
  expect(first.receiptPending).toBe(true);
  expect((await f.store.get(f.input.id)).publication?.state).toBe("pending");
  const reopened = new AuthoringSession(original);
  reopened.replace(original);
  let durableKey: string | undefined;
  const retried = await publishWork(
    f.store,
    {
      ...sessionWorkPublisher(() => reopened),
      async flush(key) {
        durableKey = key;
      },
    },
    f.input,
  );
  expect(retried.result).toEqual(first.result);
  expect(durableKey).toBe(first.result.key);
  expect(contentKey(reopened.getSnapshot().project)).toBe(first.result.key);
  expect(retried.outcome).toBe("committed");
});
