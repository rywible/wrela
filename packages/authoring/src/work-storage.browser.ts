import { contentKey } from "@wrela/model";
import { searchAuthoringToolkit, type ToolkitEntry, toolkitEntrySchema } from "./authoring-toolkit";
import { type EditRecipe, parseEditRecipe } from "./edit-recipe";
import { bundleWork, restoreWorkBundle } from "./work-bundle";
import { parseWorkSession, type WorkSession } from "./work-session";

/** Separate review storage: evidence does not invalidate published source identity. */
export class BrowserWorkStore {
  private async database() {
    return new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open("wrela-authoring-work", 4);
      request.onupgradeneeded = () => {
        if (!request.result.objectStoreNames.contains("work"))
          request.result.createObjectStore("work", { keyPath: "id" });
        if (!request.result.objectStoreNames.contains("artifacts"))
          request.result.createObjectStore("artifacts");
        if (!request.result.objectStoreNames.contains("recipes"))
          request.result.createObjectStore("recipes", { keyPath: "id" });
        if (!request.result.objectStoreNames.contains("toolkit")) request.result.createObjectStore("toolkit");
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  }
  async backup(id: string) {
    return bundleWork(
      await this.get(id),
      (s) => s.startsWith("work-artifact:"),
      (s) => this.artifact(s),
    );
  }
  async toolkit(query = "") {
    const db = await this.database();
    try {
      return await new Promise<ToolkitEntry[]>((resolve, reject) => {
        const request = db.transaction("toolkit").objectStore("toolkit").getAll();
        request.onsuccess = () => {
          try {
            resolve(searchAuthoringToolkit(request.result, query));
          } catch (error) {
            reject(error);
          }
        };
        request.onerror = () => reject(request.error);
      });
    } finally {
      db.close();
    }
  }
  async saveToolkit(raw: ToolkitEntry, expectedKey: string | null = null) {
    const entry = toolkitEntrySchema.parse(raw),
      db = await this.database(),
      id = contentKey([entry.id, entry.revision]);
    try {
      await new Promise<void>((resolve, reject) => {
        const transaction = db.transaction("toolkit", "readwrite"),
          store = transaction.objectStore("toolkit"),
          request = store.get(id);
        let error: unknown;
        request.onsuccess = () => {
          try {
            const old = request.result as ToolkitEntry | undefined;
            if ((old ? contentKey(old) : null) !== expectedKey) throw Error("Toolkit version changed");
            if (old && contentKey(old.implementation) !== contentKey(entry.implementation))
              throw Error("Toolkit implementations are immutable within a revision");
            if (!old && (entry.status !== "experimental" || entry.trials.length))
              throw Error("New capabilities begin experimental");
            store.put(entry, id);
          } catch (e) {
            error = e;
            transaction.abort();
          }
        };
        transaction.oncomplete = () => resolve();
        transaction.onabort = transaction.onerror = () => reject(error ?? transaction.error);
      });
      return entry;
    } finally {
      db.close();
    }
  }
  async restore(input: unknown, expectedKey: string | null = null) {
    const work = await restoreWorkBundle(input, (name, value) => this.saveArtifact(name, value));
    await this.put(work, expectedKey);
    return work;
  }
  async saveRecipe(input: EditRecipe) {
    const recipe = parseEditRecipe(input),
      db = await this.database();
    try {
      await new Promise<void>((resolve, reject) => {
        const transaction = db.transaction("recipes", "readwrite"),
          store = transaction.objectStore("recipes"),
          request = store.get(recipe.id);
        let error: Error | undefined;
        request.onsuccess = () => {
          if (request.result && contentKey(request.result) !== contentKey(recipe)) {
            error = Error("Recipe ID already exists with different content");
            transaction.abort();
          } else store.put(recipe);
        };
        transaction.oncomplete = () => resolve();
        transaction.onabort = transaction.onerror = () => reject(error ?? transaction.error);
      });
      return recipe;
    } finally {
      db.close();
    }
  }
  async recipes(search = "") {
    const db = await this.database();
    try {
      return await new Promise<EditRecipe[]>((resolve, reject) => {
        const request = db.transaction("recipes").objectStore("recipes").getAll();
        request.onsuccess = () =>
          resolve(
            request.result
              .map(parseEditRecipe)
              .filter((r: EditRecipe) =>
                `${r.id} ${r.description}`.toLowerCase().includes(search.toLowerCase()),
              ),
          );
        request.onerror = () => reject(request.error);
      });
    } finally {
      db.close();
    }
  }
  async saveArtifact(name: string, value: Blob | object) {
    const db = await this.database(),
      id = `${crypto.randomUUID()}/${name}`;
    try {
      await new Promise<void>((resolve, reject) => {
        const transaction = db.transaction("artifacts", "readwrite");
        transaction.objectStore("artifacts").add(value, id);
        transaction.oncomplete = () => resolve();
        transaction.onabort = transaction.onerror = () => reject(transaction.error);
      });
      return `work-artifact:${id}`;
    } finally {
      db.close();
    }
  }
  async artifact(reference: string) {
    if (!reference.startsWith("work-artifact:")) throw Error("Unknown artifact reference");
    const db = await this.database();
    try {
      return await new Promise<Blob | object>((resolve, reject) => {
        const request = db.transaction("artifacts").objectStore("artifacts").get(reference.slice(14));
        request.onsuccess = () =>
          request.result === undefined ? reject(Error("Artifact missing")) : resolve(request.result);
        request.onerror = () => reject(request.error);
      });
    } finally {
      db.close();
    }
  }
  async list() {
    const db = await this.database();
    try {
      return await new Promise<{ id: string; brief: string; key: string }[]>((resolve, reject) => {
        const request = db.transaction("work").objectStore("work").getAll();
        request.onsuccess = () =>
          resolve(request.result.map((w) => ({ id: w.id, brief: w.brief, key: contentKey(w) })));
        request.onerror = () => reject(request.error);
      });
    } finally {
      db.close();
    }
  }
  async get(id: string) {
    const db = await this.database();
    try {
      return await new Promise<WorkSession>((resolve, reject) => {
        const request = db.transaction("work").objectStore("work").get(id);
        request.onsuccess = () => {
          try {
            resolve(parseWorkSession(request.result));
          } catch (e) {
            reject(e);
          }
        };
        request.onerror = () => reject(request.error);
      });
    } finally {
      db.close();
    }
  }
  async put(input: WorkSession, expectedKey: string | null) {
    const work = parseWorkSession(input),
      db = await this.database();
    try {
      return await new Promise<string>((resolve, reject) => {
        const transaction = db.transaction("work", "readwrite"),
          store = transaction.objectStore("work");
        let failure: unknown;
        const request = store.get(work.id);
        request.onsuccess = () => {
          if ((request.result ? contentKey(parseWorkSession(request.result)) : null) !== expectedKey) {
            failure = new Error("Work session changed in another writer; reopen before saving");
            transaction.abort();
            return;
          }
          store.put(work);
        };
        transaction.oncomplete = () => resolve(contentKey(work));
        transaction.onabort = transaction.onerror = () =>
          reject(failure ?? transaction.error ?? Error("Work storage failed"));
      });
    } finally {
      db.close();
    }
  }
}
