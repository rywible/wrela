import { contentKey, type Project, parseProject } from "@wrela/model";

export type SavedProject = {
  id: string;
  project: Project;
  revision: number;
  updatedAt: string;
  key: string;
  writer?: string;
};
export type RecoveryDraft = SavedProject & {
  writer: string;
  /** Saved source this writer edited. Missing only for migrated legacy drafts. */
  baseKey?: string | null;
  /** Independent named checkpoint; ordinary per-writer recovery remains mutable. */
  label?: string;
};
function request<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}
export class ProjectStore {
  private db: IDBDatabase | undefined;
  private opening: Promise<void> | undefined;
  private generation = 0;
  private writer = crypto.randomUUID();
  private lastTimestamp = 0;
  private savedKeys = new Map<string, string | null>();
  constructor(private name = "wrela-studio") {}
  async open(): Promise<void> {
    if (this.db) return;
    if (this.opening) return this.opening;
    const generation = this.generation;
    const opening = new Promise<void>((resolve, reject) => {
      const req = indexedDB.open(this.name, 2);
      let settled = false;
      req.onupgradeneeded = (event) => {
        const db = req.result;
        for (const name of ["projects", "runtime"])
          if (!db.objectStoreNames.contains(name)) db.createObjectStore(name, { keyPath: "id" });
        const createRecovery = () => {
          const store = db.createObjectStore("recovery", { keyPath: ["id", "writer"] });
          store.createIndex("project", "id");
          return store;
        };
        if (!db.objectStoreNames.contains("recovery")) createRecovery();
        else if (event.oldVersion < 2) {
          // Rebuild inside the upgrade transaction: either every legacy draft is
          // retained with its original metadata, or the complete upgrade aborts.
          const upgrade = req.transaction;
          if (!upgrade) throw new Error("Storage upgrade transaction is missing");
          const legacy = upgrade.objectStore("recovery").getAll();
          legacy.onsuccess = () => {
            db.deleteObjectStore("recovery");
            const recovery = createRecovery();
            for (const record of legacy.result as SavedProject[])
              recovery.put({ ...record, writer: record.writer ?? "legacy" });
          };
        }
      };
      req.onerror = () => {
        settled = true;
        reject(req.error ?? new Error("Storage could not be opened"));
      };
      req.onblocked = () => {
        settled = true;
        reject(new Error("Storage upgrade is blocked by another open Wrela window. Close it and retry."));
      };
      req.onsuccess = () => {
        if (settled || generation !== this.generation) {
          req.result.close();
          if (!settled) reject(new Error("Storage closed while opening"));
          return;
        }
        settled = true;
        this.db = req.result;
        this.db.onversionchange = () => this.close();
        resolve();
      };
    });
    this.opening = opening;
    try {
      await opening;
    } finally {
      if (this.opening === opening) this.opening = undefined;
    }
  }
  private record(project: Project, revision: number): SavedProject {
    if (!Number.isInteger(revision) || revision < 0)
      throw new Error("Storage revision must be a nonnegative integer");
    const parsed = parseProject(project);
    this.lastTimestamp = Math.max(Date.now(), this.lastTimestamp + 1);
    return {
      id: parsed.id,
      project: parsed,
      revision,
      updatedAt: new Date(this.lastTimestamp).toISOString(),
      key: contentKey(parsed),
      writer: this.writer,
    };
  }
  private validateRecord(record: SavedProject): SavedProject {
    // Check the persisted bytes before schema migration changes their identity.
    if (record.key !== contentKey(record.project))
      throw new Error("Saved project checksum does not match. Import a backup to recover.");
    const project = parseProject(record.project);
    if (
      record.id !== project.id ||
      !Number.isInteger(record.revision) ||
      record.revision < 0 ||
      !Number.isFinite(Date.parse(record.updatedAt))
    )
      throw new Error("Saved project metadata is invalid. Import a backup to recover.");
    return { ...record, project, key: contentKey(project) };
  }
  private transaction(stores: string[], mode: IDBTransactionMode) {
    if (!this.db) throw Error("Storage is not open");
    return this.db.transaction(stores, mode);
  }
  private done(tx: IDBTransaction): Promise<void> {
    return new Promise((resolve, reject) => {
      tx.oncomplete = () => resolve();
      tx.onabort = () => reject(tx.error ?? Error("Storage transaction aborted"));
      tx.onerror = () => reject(tx.error);
    });
  }
  async save(project: Project, revision: number) {
    const record = this.record(project, revision);
    const tx = this.transaction(["projects", "recovery", "runtime"], "readwrite"),
      done = this.done(tx);
    try {
      const saved = await request<SavedProject | undefined>(tx.objectStore("projects").get(record.id));
      if (this.savedKeys.has(record.id) && this.savedKeys.get(record.id) !== (saved?.key ?? null))
        throw new Error(
          "This project was saved by another window. Reopen it before saving over those changes.",
        );
      if (
        saved?.writer === this.writer &&
        (saved.revision > revision || (saved.revision === revision && saved.key !== record.key))
      )
        throw new Error("A newer or different source snapshot has already been saved for this revision");
      const recovery = await request<RecoveryDraft | undefined>(
        tx.objectStore("recovery").get([record.id, this.writer]),
      );
      record.updatedAt = new Date(
        Math.max(Date.parse(record.updatedAt), saved ? Date.parse(saved.updatedAt) + 1 : 0),
      ).toISOString();
      const newer = recovery && recovery.revision > revision && recovery.key !== record.key;
      tx.objectStore("projects").put(record);
      if (!newer) tx.objectStore("recovery").delete([record.id, this.writer]);
      tx.objectStore("runtime").put({ id: "_active-project", value: record.id });
      await done;
      this.savedKeys.set(record.id, record.key);
      return record;
    } catch (error) {
      try {
        tx.abort();
      } catch {}
      await done.catch(() => {});
      throw error;
    }
  }
  async recover(project: Project, revision: number) {
    const record = this.record(project, revision);
    const tx = this.transaction(["projects", "recovery", "runtime"], "readwrite"),
      done = this.done(tx);
    try {
      const saved = await request<SavedProject | undefined>(tx.objectStore("projects").get(record.id));
      const recovery = await request<RecoveryDraft | undefined>(
        tx.objectStore("recovery").get([record.id, this.writer]),
      );
      const stale =
        saved?.key === record.key ||
        (saved?.writer === this.writer && saved.revision >= revision) ||
        (recovery && recovery.revision > revision);
      if (!stale) {
        record.updatedAt = new Date(
          Math.max(
            Date.parse(record.updatedAt),
            saved ? Date.parse(saved.updatedAt) + 1 : 0,
            recovery ? Date.parse(recovery.updatedAt) + 1 : 0,
          ),
        ).toISOString();
        this.lastTimestamp = Math.max(this.lastTimestamp, Date.parse(record.updatedAt));
        const draft: RecoveryDraft = {
          ...record,
          writer: this.writer,
          baseKey: this.savedKeys.has(record.id)
            ? (this.savedKeys.get(record.id) ?? null)
            : (saved?.key ?? null),
        };
        tx.objectStore("recovery").put(draft);
      }
      tx.objectStore("runtime").put({ id: "_active-project", value: record.id });
      await done;
    } catch (error) {
      try {
        tx.abort();
      } catch {}
      await done.catch(() => {});
      throw error;
    }
  }
  /** Preserve work independently of the active writer's next autosave or save. */
  async preserveRecovery(
    project: Project,
    revision: number,
    label = "Preserved work",
  ): Promise<RecoveryDraft> {
    if (!label.trim() || label.length > 120) throw new Error("Recovery label must contain 1–120 characters");
    const record = this.record(project, revision);
    const tx = this.transaction(["projects", "recovery"], "readwrite"),
      done = this.done(tx);
    try {
      const saved = await request<SavedProject | undefined>(tx.objectStore("projects").get(record.id));
      const draft: RecoveryDraft = {
        ...record,
        writer: `checkpoint-${crypto.randomUUID()}`,
        label,
        baseKey: this.savedKeys.has(record.id)
          ? (this.savedKeys.get(record.id) ?? null)
          : (saved?.key ?? null),
      };
      tx.objectStore("recovery").add(draft);
      await done;
      return draft;
    } catch (error) {
      try {
        tx.abort();
      } catch {}
      await done.catch(() => {});
      throw error;
    }
  }
  async load(id: string, recovery = false): Promise<SavedProject | undefined> {
    const tx = this.transaction(recovery ? ["projects", "recovery"] : ["projects"], "readonly");
    const savedRequest = request<SavedProject | undefined>(tx.objectStore("projects").get(id));
    const draftsRequest = recovery
      ? request<RecoveryDraft[]>(tx.objectStore("recovery").index("project").getAll(id))
      : undefined;
    const [record, drafts] = await Promise.all([savedRequest, draftsRequest]);
    const valid = record ? this.validateRecord(record) : undefined;
    if (drafts) {
      // Revisions belong to individual writers. A draft remains recoverable
      // after a different writer saves, even when that save has a later date.
      return this.validateRecoveries(drafts).find((draft) => draft.key !== valid?.key);
    }
    // Save preconditions compare persisted identity, even when callers receive migrated source.
    this.savedKeys.set(id, record?.key ?? null);
    return valid;
  }
  private validateRecoveries(records: RecoveryDraft[]): RecoveryDraft[] {
    return records
      .map((record) => {
        const valid = this.validateRecord(record);
        if (
          typeof record.writer !== "string" ||
          !record.writer ||
          (record.baseKey !== undefined && record.baseKey !== null && typeof record.baseKey !== "string") ||
          (record.label !== undefined &&
            (typeof record.label !== "string" || !record.label.trim() || record.label.length > 120))
        )
          throw new Error("Recovery metadata is invalid. Import a backup to recover.");
        return { ...valid, writer: record.writer, baseKey: record.baseKey, label: record.label };
      })
      .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt) || a.writer.localeCompare(b.writer));
  }
  async listRecoveries(id: string): Promise<RecoveryDraft[]> {
    const tx = this.transaction(["recovery"], "readonly");
    return this.validateRecoveries(
      await request<RecoveryDraft[]>(tx.objectStore("recovery").index("project").getAll(id)),
    );
  }
  async list(): Promise<SavedProject[]> {
    const tx = this.transaction(["projects"], "readonly");
    return (await request<SavedProject[]>(tx.objectStore("projects").getAll())).map((record) =>
      this.validateRecord(record),
    );
  }
  async saveRuntime(id: string, value: unknown) {
    const tx = this.transaction(["runtime"], "readwrite"),
      done = this.done(tx);
    tx.objectStore("runtime").put({ id, value });
    await done;
  }
  async loadRuntime(id: string): Promise<unknown> {
    const tx = this.transaction(["runtime"], "readonly");
    return (await request(tx.objectStore("runtime").get(id)))?.value;
  }
  close() {
    this.generation++;
    this.db?.close();
    this.db = undefined;
  }
}
export function importProject(text: string): Project {
  if (text.length > 8 * 1024 * 1024) throw Error("Project exceeds the 8 MB source limit");
  return parseProject(JSON.parse(text));
}
export function downloadBlob(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob),
    link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
