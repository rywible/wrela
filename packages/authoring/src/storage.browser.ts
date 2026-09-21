import { contentKey, referenceProject } from "@wrela/model";
import { ProjectStore } from "./storage";
/** Runs against a real browser IndexedDB implementation. Used by the isolated
 * browser harness; no in-memory database substitute hides transaction timing. */
export async function verifyProjectStorage(): Promise<{ checks: string[] }> {
  const checks: string[] = [],
    name = `wrela-storage-verification-${crypto.randomUUID()}`,
    store = new ProjectStore(name);
  const assert = (value: unknown, message: string) => {
    if (!value) throw new Error(message);
  };
  const project = { ...referenceProject(), id: "custom-project", name: "Custom project" };
  try {
    await Promise.all([store.open(), store.open(), store.open()]);
    await store.save(project, 0);
    assert(
      (await store.loadRuntime("_active-project")) === "custom-project",
      "Saved custom project was not made active",
    );
    checks.push("custom project active metadata");
    const later = { ...project, name: "Newer recovery" };
    await store.recover(later, 2);
    await store.save({ ...project, name: "Intermediate save" }, 1);
    const saved = await store.load(project.id),
      recovery = await store.load(project.id, true);
    assert(recovery?.project.name === later.name, "An older save erased newer recovery");
    assert(recovery && saved, "Saved project or recovery disappeared");
    checks.push("newer recovery survives older save");
    await store.recover(project, 0);
    assert((await store.load(project.id, true))?.revision === 2, "Delayed recovery overwrote newer work");
    await store.save(later, 2);
    await store.recover(later, 2);
    assert(!(await store.load(project.id, true)), "Saved content was spuriously recovered as unsaved");
    checks.push("stale and identical recovery ignored");
    let rejected = false;
    try {
      await store.save(project, 1);
    } catch {
      rejected = true;
    }
    assert(rejected, "An older save overwrote a newer saved revision");
    checks.push("stale saved revision rejected");
    store.close();
    const reopened = new ProjectStore(name);
    await reopened.open();
    try {
      assert(
        (await reopened.load(project.id))?.project.name === later.name,
        "Saved project did not survive reopen",
      );
      await reopened.recover({ ...later, name: "New session edit" }, 1);
      assert(
        (await reopened.load(project.id, true))?.project.name === "New session edit",
        "New session revision was compared with old session revision",
      );
      checks.push("fresh session recovery ordering");
      const second = new ProjectStore(name);
      await second.open();
      try {
        const base = await second.load(project.id);
        await second.recover({ ...later, name: "Other window unsaved" }, 2);
        const drafts = await second.listRecoveries(project.id);
        assert(drafts.length === 2, "One writer replaced another writer's unsaved recovery");
        assert(
          new Set(drafts.map((draft) => draft.writer)).size === 2 &&
            drafts.every((draft) => draft.baseKey === base?.key),
          "Recovery drafts did not retain their distinct writers and saved source bases",
        );
        assert(
          (await second.load(project.id, true))?.project.name === "Other window unsaved",
          "Legacy recovery load did not choose the newest divergent draft",
        );
        checks.push("independent writer drafts and source bases");
        await second.save({ ...later, name: "Other window edit" }, 3);
        const remaining = await second.listRecoveries(project.id);
        assert(
          remaining.length === 1 && remaining[0]?.project.name === "New session edit",
          "Saving one writer erased another writer's unsaved recovery",
        );
        const restarted = new ProjectStore(name);
        await restarted.open();
        try {
          const published = await restarted.load(project.id),
            retained = await restarted.load(project.id, true);
          assert(
            retained?.project.name === "New session edit" &&
              published &&
              retained.updatedAt < published.updatedAt,
            "Restart did not expose a divergent draft older than another writer's save",
          );
        } finally {
          restarted.close();
        }
        checks.push("foreign unsaved recovery survives save and restart");
        let conflict = false;
        try {
          await reopened.save({ ...later, name: "Stale window edit" }, 2);
        } catch {
          conflict = true;
        }
        assert(conflict, "A stale window silently overwrote another window");
        checks.push("cross-window save conflicts");
      } finally {
        second.close();
      }
    } finally {
      reopened.close();
    }
    const raw = await new Promise<IDBDatabase>((resolve, reject) => {
      const req = indexedDB.open(name);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    try {
      await new Promise<void>((resolve, reject) => {
        const tx = raw.transaction("projects", "readwrite");
        tx.objectStore("projects").put({ ...saved, key: "corrupted" });
        tx.oncomplete = () => resolve();
        tx.onabort = () => reject(tx.error);
      });
    } finally {
      raw.close();
    }
    await store.open();
    rejected = false;
    try {
      await store.load(project.id);
    } catch {
      rejected = true;
    }
    assert(rejected, "Corrupted persisted content passed its integrity check");
    checks.push("corrupt persisted data rejected");
    await verifyPreservedRecovery();
    checks.push("preserved work survives restored-draft autosave, save and restart");
    await verifyLegacyRecoveryMigration();
    checks.push("legacy recovery migration preserves content and provenance");
  } finally {
    store.close();
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error("Concurrent open leaked an IndexedDB connection")),
        2000,
      );
      const req = indexedDB.deleteDatabase(name);
      req.onsuccess = () => {
        clearTimeout(timer);
        resolve();
      };
      req.onerror = () => {
        clearTimeout(timer);
        reject(req.error);
      };
    });
  }
  checks.push("concurrent opens close cleanly");
  return { checks };
}

async function verifyLegacyRecoveryMigration() {
  const name = `wrela-storage-v1-migration-${crypto.randomUUID()}`;
  const project = { ...referenceProject(), id: "legacy-project", name: "Legacy unsaved work" };
  const legacy = {
    id: project.id,
    project,
    revision: 7,
    updatedAt: "2026-01-01T00:00:00.000Z",
    key: contentKey(project),
    writer: "legacy-window",
  };
  const anonymousProject = { ...project, id: "anonymous-project" };
  const oldSource = {
    ...project,
    id: "old-source",
    schemaVersion: 0,
    documents: project.documents.map((document) => {
      const { schemaVersion: _version, dependencies: _dependencies, ...fields } = document;
      return fields;
    }),
  };
  const store = new ProjectStore(name);
  try {
    const raw = await new Promise<IDBDatabase>((resolve, reject) => {
      const req = indexedDB.open(name, 1);
      req.onupgradeneeded = () => {
        for (const storeName of ["projects", "recovery", "runtime"])
          req.result.createObjectStore(storeName, { keyPath: "id" });
      };
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    try {
      await new Promise<void>((resolve, reject) => {
        const tx = raw.transaction(["projects", "recovery", "runtime"], "readwrite");
        tx.objectStore("recovery").put(legacy);
        tx.objectStore("projects").put({
          ...legacy,
          id: oldSource.id,
          project: oldSource,
          key: contentKey(oldSource),
        });
        tx.objectStore("recovery").put({
          ...legacy,
          id: anonymousProject.id,
          project: anonymousProject,
          key: contentKey(anonymousProject),
          writer: undefined,
        });
        tx.objectStore("runtime").put({ id: "_active-project", value: project.id });
        tx.oncomplete = () => resolve();
        tx.onabort = () => reject(tx.error);
      });
    } finally {
      raw.close();
    }
    await store.open();
    const oldLoaded = await store.load(oldSource.id);
    if (
      !oldLoaded ||
      oldLoaded.project.schemaVersion !== 1 ||
      oldLoaded.key !== contentKey(oldLoaded.project) ||
      oldLoaded.key === contentKey(oldSource)
    )
      throw new Error("Stored source was not integrity-checked before migration");
    const migrated = await store.listRecoveries(project.id);
    if (
      migrated.length !== 1 ||
      migrated[0]?.key !== legacy.key ||
      migrated[0]?.writer !== legacy.writer ||
      migrated[0]?.revision !== legacy.revision ||
      migrated[0]?.updatedAt !== legacy.updatedAt ||
      migrated[0]?.baseKey !== undefined ||
      (await store.loadRuntime("_active-project")) !== project.id ||
      (await store.load(anonymousProject.id, true))?.project.id !== anonymousProject.id
    )
      throw new Error("IndexedDB upgrade failed to preserve legacy recovery metadata");
    await store.save(oldLoaded.project, 1);
    if ((await store.load(oldSource.id))?.key !== oldLoaded.key)
      throw new Error("Migrated source could not be saved against the persisted source identity");
    await store.recover({ ...project, name: "New writer draft" }, 1);
    await store.save({ ...project, name: "New writer save" }, 2);
    if ((await store.load(project.id, true))?.key !== legacy.key)
      throw new Error("A new writer erased the migrated legacy recovery");
  } finally {
    store.close();
    await new Promise<void>((resolve, reject) => {
      const req = indexedDB.deleteDatabase(name);
      req.onsuccess = () => resolve();
      req.onerror = () => reject(req.error);
    });
  }
}

async function verifyPreservedRecovery() {
  const name = `wrela-storage-checkpoint-${crypto.randomUUID()}`,
    store = new ProjectStore(name);
  const project = { ...referenceProject(), id: "checkpoint-project" },
    current = { ...project, name: "Unsaved work before switching drafts" },
    restored = { ...project, name: "Restored other draft" };
  try {
    await store.open();
    await store.save(project, 0);
    await store.recover(current, 1);
    const checkpoint = await store.preserveRecovery(current, 1, "Before restoring a draft");
    // The real browser timer reproduces the subsequent Studio recovery debounce:
    // this writer's mutable recovery moves on, while its preserved source must survive.
    await new Promise<void>((resolve, reject) => {
      setTimeout(() => {
        void store.recover(restored, 2).then(resolve, reject);
      }, 300);
    });
    const drafts = await store.listRecoveries(project.id);
    if (
      !drafts.some((draft) => draft.writer === checkpoint.writer && draft.key === contentKey(current)) ||
      !drafts.some((draft) => draft.key === contentKey(restored))
    )
      throw new Error("Autosaving the restored draft overwrote the preserved current work");
    await store.save(restored, 2);
    store.close();
    await store.open();
    const retained = (await store.listRecoveries(project.id)).find(
      (draft) => draft.writer === checkpoint.writer,
    );
    if (retained?.project.name !== current.name || retained.label !== "Before restoring a draft")
      throw new Error("Saving or restarting erased the independently preserved source checkpoint");
  } finally {
    store.close();
    await new Promise<void>((resolve, reject) => {
      const request = indexedDB.deleteDatabase(name);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  }
}
