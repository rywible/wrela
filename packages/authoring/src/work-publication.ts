import { contentKey } from "@wrela/model";

import type { EditBatch } from "./commands";
import type { AuthoringSession, TransactionResult } from "./session";
import {
  adoptionBatch,
  appendWorkEvent,
  parseWorkSession,
  type WorkSession,
  workHandoff,
} from "./work-session";

export interface WorkRecordStore {
  get(id: string): Promise<WorkSession>;
  put(work: WorkSession, expectedKey: string | null): Promise<string>;
}
export interface WorkPublisher {
  snapshot(): Promise<{ key: string; revision: number }>;
  preview(batch: EditBatch): Promise<TransactionResult>;
  commit(batch: EditBatch, baselineKey: string): Promise<TransactionResult>;
  flush?(sourceKey: string): Promise<void>;
  receipt?(transactionId: string): Promise<TransactionResult | undefined>;
}
export function sessionWorkPublisher(session: () => AuthoringSession): WorkPublisher {
  return {
    async snapshot() {
      const state = session().getSnapshot();
      return { key: contentKey(state.project), revision: state.revision };
    },
    async preview(batch) {
      const current = session(),
        preview = current.preview(batch);
      return {
        revision: preview.revision + (preview.changes.length ? 1 : 0),
        changed: preview.changes.map((c) => c.id),
        // The coordinator fills the verified candidate identity before retaining this receipt.
        key: "",
        transactionId: batch.transactionId!,
        diff: preview.diff,
      };
    },
    async commit(batch, baselineKey) {
      const current = session();
      if (contentKey(current.getSnapshot().project) !== baselineKey)
        throw Error("Accepted source changed; review again");
      return current.apply({ ...batch, expectedRevision: current.getSnapshot().revision });
    },
  };
}
/** A durable intent precedes source mutation. After source commits, receipt storage failure
 * is an explicit successful outcome with recovery pending, never a failed edit response.
 * Retrying the original request key recovers the same transaction after a process restart. */
export async function publishWork(
  store: WorkRecordStore,
  publisher: WorkPublisher,
  input: {
    id: string;
    expectedKey: string;
    proposal: string;
    requireArtReview?: boolean;
  },
) {
  let work = await store.get(input.id),
    key = contentKey(work);
  const prior = work.publication;
  const retry =
    prior?.proposal === input.proposal &&
    (prior.requestKey === input.expectedKey || key === input.expectedKey);
  if (key !== input.expectedKey && !retry) throw Error("Work changed; reopen before adoption");
  if (prior && !retry) throw Error("Another publication is already reserved for this work");
  if (!prior) {
    const source = await publisher.snapshot();
    if (source.key !== work.baselineKey)
      throw Error("Accepted source changed; start a new work session and review again");
    const batch = {
      ...adoptionBatch(work, input.proposal, input.requireArtReview),
      expectedRevision: source.revision,
      transactionId: `adopt-${crypto.randomUUID()}`,
    };
    const receipt = await publisher.preview(batch);
    receipt.key = work.proposals.find((p) => p.id === input.proposal)!.candidateKey;
    work = parseWorkSession({
      ...work,
      publication: {
        requestKey: input.expectedKey,
        proposal: input.proposal,
        expectedRevision: source.revision,
        receipt,
        state: "pending",
      },
    });
    key = await store.put(work, key);
  }
  const publication = work.publication!;
  let result = publication.receipt;
  if (publication.state !== "committed") {
    const existing = await publisher.receipt?.(result.transactionId),
      source = await publisher.snapshot();
    if (existing) result = existing;
    else if (source.key !== result.key) {
      const batch = {
        ...adoptionBatch(work, input.proposal, input.requireArtReview),
        expectedRevision: publication.expectedRevision,
        transactionId: result.transactionId,
      };
      try {
        const committed = await publisher.commit(batch, work.baselineKey);
        if (committed.key !== result.key) throw Error("Publication differs from reviewed source");
      } catch (error) {
        const recovered = await publisher.receipt?.(result.transactionId);
        if (recovered) result = recovered;
        else if ((await publisher.snapshot()).key !== result.key) throw error;
      }
    }
    if (result.key !== publication.receipt.key)
      throw Error("Publication receipt does not match reviewed source");
    // Browser acceptance and durable project storage have different lifetimes. Keep the
    // intent pending until the source save succeeds, so a crash can replay from baseline.
    try {
      await publisher.flush?.(result.key);
    } catch (error) {
      return {
        outcome: "accepted" as const,
        receiptPending: true,
        persistencePending: true,
        message: error instanceof Error ? error.message : String(error),
        result,
        work: { key, ...workHandoff(work) },
      };
    }
    work = parseWorkSession({
      ...work,
      publication: { ...publication, receipt: result, state: "committed" },
      proposals: work.proposals.map((p) => (p.id === input.proposal ? { ...p, status: "adopted" } : p)),
    });
    if (work.events.length < 1024 && !work.events.some((event) => event.id === result.transactionId))
      work = appendWorkEvent(work, {
        id: result.transactionId,
        at: new Date().toISOString(),
        kind: "adopt",
        actor: "publisher",
        summary: `Adopted ${input.proposal}`,
      });
    try {
      key = await store.put(work, key);
    } catch {
      return {
        outcome: "committed" as const,
        receiptPending: true,
        result,
        work: { key, ...workHandoff(work) },
      };
    }
  }
  return {
    outcome: "committed" as const,
    receiptPending: false,
    result,
    work: { key, ...workHandoff(work) },
  };
}
