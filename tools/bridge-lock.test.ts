import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdtemp, rm, stat, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { referenceProject } from "@wrela/model";
import { WorkspaceBridge } from "./bridge";
import { withWorkspacePublicationLock } from "./bridge-lock";

interface WorkerMessage {
  event: string;
  status?: string;
  error?: string;
}
function spawnWorker(...args: string[]) {
  const received = new Map<string, WorkerMessage>();
  const waiters = new Map<string, (message: WorkerMessage) => void>();
  const child = Bun.spawn([process.execPath, join(import.meta.dir, "bridge-lock-worker.ts"), ...args], {
    stdout: "ignore",
    stderr: "pipe",
    ipc(message: WorkerMessage) {
      received.set(message.event, message);
      waiters.get(message.event)?.(message);
    },
  });
  return {
    child,
    async wait(event: string, timeoutMs = 5_000): Promise<WorkerMessage> {
      const existing = received.get(event);
      if (existing) return existing;
      let timer: ReturnType<typeof setTimeout> | undefined;
      try {
        return await new Promise<WorkerMessage>((resolve, reject) => {
          waiters.set(event, resolve);
          timer = setTimeout(() => reject(new Error(`Timed out waiting for worker ${event}`)), timeoutMs);
        });
      } finally {
        clearTimeout(timer);
        waiters.delete(event);
      }
    },
  };
}

let root: string, bridge: WorkspaceBridge, lockPath: string;
beforeEach(async () => {
  root = await mkdtemp(join(tmpdir(), "wrela-publication-lock-"));
  bridge = new WorkspaceBridge(root);
  await bridge.initialize();
  lockPath = join(root, ".wrela", "PUBLISH.lock");
});
afterEach(async () => {
  await rm(root, { recursive: true, force: true });
});

test("separate processes cannot both publish against the same source key", async () => {
  const saved = await bridge.save(referenceProject(), null);
  const a = spawnWorker("save", root, "Agent A", saved.key, "pause");
  const b = spawnWorker("probe-save", root, "Agent B", saved.key);
  try {
    await Promise.all([a.wait("ready"), b.wait("ready")]);
    a.child.send("start");
    // A has captured CURRENT in its final read, inside the publication lock.
    await a.wait("final-read");
    b.child.send("start");
    // B probes the same OS lock while A's final read is paused. A remains paused
    // until the probe finishes, so slow worker startup cannot hide missing locks.
    expect((await b.wait("probe")).status).toBe("held");
    b.child.send("continue");
    a.child.send("release");
    expect((await a.wait("result")).status).toBe("fulfilled");
    const loser = await b.wait("result");
    expect(loser.status).toBe("rejected");
    expect(loser.error).toContain("changed externally");
    expect((await bridge.read())?.project.name).toBe("Agent A");
    expect(await a.child.exited).toBe(0);
    expect(await b.child.exited).toBe(0);
  } finally {
    a.child.kill();
    b.child.kill();
    await Promise.all([a.child.exited, b.child.exited]);
  }
});

test("process death releases the lock without deleting its stable inode", async () => {
  const holder = spawnWorker("hold", root);
  try {
    await holder.wait("locked");
    const before = await stat(lockPath);
    await expect(withWorkspacePublicationLock(lockPath, async () => "unexpected", 50)).rejects.toThrow(
      "busy publishing",
    );
    // Timing out closes only our own handle; it must not release the holder's lock.
    await expect(withWorkspacePublicationLock(lockPath, async () => "unexpected", 50)).rejects.toThrow(
      "busy publishing",
    );
    holder.child.kill("SIGKILL");
    await holder.child.exited;
    await bridge.save({ ...referenceProject(), name: "Recovered after crash" }, null);
    expect((await bridge.read())?.project.name).toBe("Recovered after crash");
    const after = await stat(lockPath);
    expect([after.dev, after.ino]).toEqual([before.dev, before.ino]);
  } finally {
    holder.child.kill();
    await holder.child.exited;
  }
});

test("a failed publication releases its lock", async () => {
  await expect(
    withWorkspacePublicationLock(lockPath, async () => {
      throw new Error("publication failed");
    }),
  ).rejects.toThrow("publication failed");
  expect(await withWorkspacePublicationLock(lockPath, async () => "recovered", 50)).toBe("recovered");
});

test("publication refuses a symbolic link at the lock path", async () => {
  const external = join(root, "external.txt");
  await writeFile(external, "unchanged");
  await symlink(external, lockPath);
  await expect(bridge.save(referenceProject(), null)).rejects.toThrow();
  expect(await bridge.read()).toBeNull();
});
