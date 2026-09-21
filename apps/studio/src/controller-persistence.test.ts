import { afterEach, expect, spyOn, test } from "bun:test";
import type { RecoveryDraft, SavedProject } from "@wrela/authoring";
import { contentKey, type Project, referenceProject } from "@wrela/model";
import { StudioController } from "./controller";

/** Deterministic persistence scheduling around the actual controller. IndexedDB
 * transaction/migration semantics are independently exercised in storage.browser. */
class MemoryStore {
  saved: SavedProject;
  drafts = new Map<string, RecoveryDraft>();
  actions: string[] = [];
  beforeRecover?: () => Promise<void>;
  beforeCheckpoint?: () => Promise<void>;
  constructor(project: Project) {
    this.saved = this.record(project, 0, "previous-session");
  }
  private record(project: Project, revision: number, writer: string): SavedProject {
    return {
      id: project.id,
      project: structuredClone(project),
      revision,
      writer,
      key: contentKey(project),
      updatedAt: new Date().toISOString(),
    };
  }
  async open() {}
  close() {}
  async loadRuntime() {
    return this.saved.id;
  }
  async load() {
    return structuredClone(this.saved);
  }
  async listRecoveries(id: string) {
    return structuredClone([...this.drafts.values()].filter((draft) => draft.id === id));
  }
  async recover(project: Project, revision: number) {
    this.actions.push(`recover:${revision}`);
    await this.beforeRecover?.();
    const current = this.drafts.get("active");
    if (this.saved.key === contentKey(project) || (current && current.revision > revision)) return;
    this.drafts.set("active", {
      ...this.record(project, revision, "active"),
      writer: "active",
      baseKey: this.saved.key,
    });
  }
  async preserveRecovery(project: Project, revision: number, label: string) {
    this.actions.push("checkpoint");
    await this.beforeCheckpoint?.();
    const writer = `checkpoint-${this.drafts.size}`;
    const draft = { ...this.record(project, revision, writer), writer, label, baseKey: this.saved.key };
    this.drafts.set(writer, draft);
    return structuredClone(draft);
  }
  async save(project: Project, revision: number) {
    this.actions.push(`save:${revision}`);
    this.saved = this.record(project, revision, "active");
    if ((this.drafts.get("active")?.revision ?? -1) <= revision) this.drafts.delete("active");
    return structuredClone(this.saved);
  }
}
function controller(store: MemoryStore, workspace?: Project) {
  const instance = new StudioController();
  Object.defineProperty(instance, "store", { value: store });
  if (workspace) Object.assign(instance, { bridge: true, workspaceKey: contentKey(workspace) });
  return instance;
}
function startWithoutGraphics(instance: StudioController) {
  // Exercise all real startup persistence, then take its existing disposal exit
  // before worker/WebGPU creation; no source-loading code is substituted.
  Object.assign(instance, { disposed: true });
  return instance.start({} as HTMLCanvasElement);
}
let fetchSpy: ReturnType<typeof spyOn<typeof globalThis, "fetch">> | undefined;
afterEach(() => {
  fetchSpy?.mockRestore();
  fetchSpy = undefined;
});
function workspaceFetch(project: Project, put?: (request?: RequestInit) => Promise<Response>) {
  const serve = async (input: RequestInfo | URL, options?: RequestInit) => {
    if (String(input) === "/bridge/session") return Response.json({ available: true });
    if (String(input) !== "/bridge/project") throw Error(`Unexpected fetch ${String(input)}`);
    return options?.method === "PUT" && put
      ? put(options)
      : Response.json({ project, key: contentKey(project) });
  };
  fetchSpy = spyOn(globalThis, "fetch").mockImplementation(serve as typeof fetch);
}
function rename(instance: StudioController, name: string) {
  instance.authoring.apply({
    expectedRevision: instance.authoring.getSnapshot().revision,
    operations: [{ kind: "document.rename", target: "winter-sky", name }],
  });
}

test("failed workspace publication leaves durable unsaved work recoverable across startup", async () => {
  const workspace = referenceProject(),
    store = new MemoryStore(workspace),
    active = controller(store, workspace);
  rename(active, "Unsaved sky");
  const edited = active.authoring.getSnapshot().project;
  workspaceFetch(workspace, async () => {
    store.actions.push("publish");
    return Response.json({ error: "Workspace changed externally" }, { status: 409 });
  });
  await expect(active.save()).rejects.toThrow("changed externally");
  expect(store.actions).toEqual(["recover:1", "publish"]);
  expect(store.saved.key).toBe(contentKey(workspace));
  expect(store.drafts.get("active")?.key).toBe(contentKey(edited));
  const reopened = controller(store);
  await startWithoutGraphics(reopened);
  expect(reopened.authoring.getSnapshot().project).toEqual(workspace);
  expect(reopened.getSnapshot().recoveryDrafts.some((draft) => draft.key === contentKey(edited))).toBe(true);
});

test("startup checkpoints differing browser-saved work before opening workspace and deduplicates repeated opens", async () => {
  const workspace = referenceProject(),
    browser = { ...workspace, name: "Browser work from failed old save" },
    store = new MemoryStore(browser);
  workspaceFetch(workspace);
  const first = controller(store);
  await startWithoutGraphics(first);
  expect(first.authoring.getSnapshot().project).toEqual(workspace);
  expect([...store.drafts.values()].map((draft) => draft.key)).toEqual([contentKey(browser)]);
  expect(first.getSnapshot().recoveryDrafts[0].name).toContain("Browser work before opening workspace");
  const second = controller(store);
  await startWithoutGraphics(second);
  expect(store.actions.filter((action) => action === "checkpoint")).toHaveLength(1);
  expect(second.getSnapshot().recoveryDrafts).toHaveLength(1);
});

test("edits during publication remain recovered when only the submitted revision is saved", async () => {
  const workspace = referenceProject(),
    store = new MemoryStore(workspace),
    active = controller(store, workspace);
  rename(active, "Submitted source");
  const submitted = active.authoring.getSnapshot().project;
  workspaceFetch(workspace, async () => {
    store.actions.push("publish");
    rename(active, "New edit while saving");
    return Response.json({ key: contentKey(submitted) });
  });
  const result = await active.save();
  expect(result.revision).toBe(1);
  expect(store.actions).toEqual(["recover:1", "publish", "recover:2", "save:1"]);
  expect(store.saved.key).toBe(contentKey(submitted));
  expect(store.drafts.get("active")?.revision).toBe(2);
  expect(store.drafts.get("active")?.key).toBe(contentKey(active.authoring.getSnapshot().project));
  expect(active.authoring.getSnapshot().savedRevision).toBe(1);
  expect(active.authoring.getSnapshot().revision).toBe(2);
});

test("source changes during prepublication recovery cancel a stale publication", async () => {
  const workspace = referenceProject(),
    store = new MemoryStore(workspace),
    active = controller(store, workspace);
  rename(active, "First edit");
  let once = false;
  store.beforeRecover = async () => {
    if (once) return;
    once = true;
    rename(active, "Newer edit");
    await store.recover(active.authoring.getSnapshot().project, 2);
  };
  workspaceFetch(workspace, async () => {
    throw Error("Stale source must not publish");
  });
  await expect(active.save()).rejects.toThrow("Source changed while preparing");
  expect(fetchSpy).not.toHaveBeenCalled();
  expect(store.drafts.get("active")?.revision).toBe(2);
  expect(store.saved.key).toBe(contentKey(workspace));
});

test("startup refuses to replace source edited during an independent checkpoint", async () => {
  const workspace = referenceProject(),
    store = new MemoryStore({ ...workspace, name: "Local saved" }),
    active = controller(store);
  store.beforeCheckpoint = async () => {
    rename(active, "Edit while opening");
  };
  workspaceFetch(workspace);
  await startWithoutGraphics(active);
  expect(active.getSnapshot().status).toBe("failed");
  expect(active.getSnapshot().message).toContain("Source changed while opening");
  expect(active.authoring.inspect("winter-sky")).toMatchObject({ name: "Edit while opening" });
  expect([...store.drafts.values()].some((draft) => draft.project.name === "Local saved")).toBe(true);
});
