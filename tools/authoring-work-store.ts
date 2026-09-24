import { constants } from "node:fs";
import { lstat, mkdir, open, realpath, rename } from "node:fs/promises";
import { extname, isAbsolute, join, resolve } from "node:path";
import {
  bundleWork,
  type EditRecipe,
  parseEditRecipe,
  parseWorkSession,
  restoreWorkBundle,
  type WorkSession,
} from "@wrela/authoring";
import { contentKey, idSchema } from "@wrela/model";

import { withWorkspacePublicationLock } from "./bridge-lock";

/** Atomic, cross-process CAS over portable review records; source publication keeps its existing lock. */
export class WorkFileStore {
  private root: string;
  constructor(workspace: string) {
    this.root = resolve(workspace);
  }
  async directory() {
    await mkdir(this.root, { recursive: true });
    this.root = await realpath(this.root);
    let path = this.root;
    for (const part of [".wrela", "work"]) {
      path = join(path, part);
      await mkdir(path, { recursive: true });
      if ((await lstat(path)).isSymbolicLink()) throw Error("Work directories cannot be symlinks");
    }
    return path;
  }
  async get(id: string) {
    const path = join(await this.directory(), `${idSchema.parse(id)}.json`);
    const handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    try {
      const stat = await handle.stat();
      if (!stat.isFile() || stat.size > 64 * 1024 * 1024) throw Error("Work record exceeds 64 MiB");
      return parseWorkSession(JSON.parse(await handle.readFile("utf8")));
    } finally {
      await handle.close();
    }
  }
  async list() {
    const dir = await this.directory(),
      results = [];
    for (const file of new Bun.Glob("*.json").scanSync(dir)) {
      const w = await this.get(file.slice(0, -5));
      results.push({ id: w.id, brief: w.brief, key: contentKey(w) });
    }
    return results;
  }
  async saveArtifact(name: string, value: Blob | object) {
    if (!/^[a-zA-Z0-9_.-]+$/.test(name)) throw Error("Invalid artifact name");
    const directory = join(await this.directory(), "evidence", crypto.randomUUID());
    await mkdir(directory, { recursive: true });
    const path = join(directory, name);
    await Bun.write(path, value instanceof Blob ? value : JSON.stringify(value, null, 2));
    return path;
  }
  /** Explicit feedback attachments are confined to the request directory and managed evidence. */
  async retainEvidence<T>(input: T, requestDirectory: string): Promise<T> {
    const allowed = await realpath(requestDirectory),
      requestedRoot = resolve(requestDirectory),
      owned = join(await this.directory(), "evidence");
    const mapping = new Map<string, string>(),
      visiting = new Set<string>();
    let total = 0;
    const attach = async (reference: string): Promise<string> => {
      if (/^https?:\/\//.test(reference)) return reference;
      if (mapping.has(reference)) return mapping.get(reference) as string;
      const actual = await realpath(resolve(allowed, reference));
      if (!actual.startsWith(`${allowed}/`) && !actual.startsWith(`${owned}/`))
        throw Error("Feedback attachment is outside the request directory; copy it beside the request first");
      if (actual.startsWith(`${owned}/`)) return actual;
      if (visiting.has(actual)) throw Error("Cyclic evidence attachments");
      visiting.add(actual);
      const file = Bun.file(actual);
      total += file.size;
      if (total > 64 * 1024 * 1024) throw Error("Feedback attachments exceed 64 MiB");
      const name = (actual.split("/").at(-1) ?? "attachment").replace(/[^a-zA-Z0-9_.-]/g, "-");
      const value =
        extname(actual) === ".json"
          ? await walk(await file.json())
          : new Blob([await file.arrayBuffer()], { type: file.type });
      const stored = await this.saveArtifact(name, value as object | Blob);
      mapping.set(reference, stored);
      visiting.delete(actual);
      return stored;
    };
    const walk = async (value: unknown, key?: string): Promise<unknown> => {
      if (typeof value === "string" && mapping.has(value)) return mapping.get(value);
      if (key === "evidence" && Array.isArray(value)) {
        const refs: string[] = [];
        for (const r of value) {
          if (typeof r !== "string") throw Error("Evidence references must be strings");
          refs.push(await attach(r));
        }
        return refs;
      }
      if (
        typeof value === "string" &&
        isAbsolute(value) &&
        (value.startsWith(`${allowed}/`) || value.startsWith(`${requestedRoot}/`)) &&
        /\.(png|jpe?g|webp|json)$/.test(value)
      )
        return attach(value);
      if (Array.isArray(value)) {
        const values: unknown[] = [];
        for (const child of value) values.push(await walk(child));
        return values;
      }
      if (value && typeof value === "object") {
        const result: Record<string, unknown> = {};
        for (const [k, v] of Object.entries(value))
          Object.defineProperty(result, k, {
            value: await walk(v, k),
            enumerable: true,
            writable: true,
            configurable: true,
          });
        return result;
      }
      return value;
    };
    return (await walk(input)) as T;
  }
  async backup(id: string) {
    const root = join(await this.directory(), "evidence");
    return bundleWork(
      await this.get(id),
      (s) => s.startsWith(`${root}/`) && !!extname(s),
      async (s) => {
        const actual = await realpath(s);
        if (!actual.startsWith(`${root}/`)) throw Error("Evidence escaped the work directory");
        const file = Bun.file(actual);
        if (file.size > 64 * 1024 * 1024) throw Error("Evidence exceeds 64 MiB");
        return extname(actual) === ".json"
          ? file.json()
          : new Blob([await file.arrayBuffer()], { type: file.type });
      },
    );
  }
  async restore(input: unknown, expectedKey: string | null = null) {
    const work = await restoreWorkBundle(input, (name, value) => this.saveArtifact(name, value));
    await this.put(work, expectedKey);
    return work;
  }
  async saveRecipe(input: EditRecipe) {
    const recipe = parseEditRecipe(input),
      directory = join(await this.directory(), "recipes");
    await mkdir(directory, { recursive: true });
    if ((await lstat(directory)).isSymbolicLink()) throw Error("Recipe directory cannot be a symlink");
    return withWorkspacePublicationLock(join(directory, "PUBLISH.lock"), async () => {
      const path = join(directory, `${recipe.id}.json`),
        file = Bun.file(path);
      if (await file.exists()) {
        if (contentKey(await file.json()) !== contentKey(recipe))
          throw Error("Recipe ID already exists with different content");
      } else {
        const temp = join(directory, `${recipe.id}-${crypto.randomUUID()}.tmp`);
        await Bun.write(temp, JSON.stringify(recipe, null, 2));
        await rename(temp, path);
      }
      return recipe;
    });
  }
  async recipes(search = "") {
    const directory = join(await this.directory(), "recipes"),
      recipes: EditRecipe[] = [];
    await mkdir(directory, { recursive: true });
    for (const path of new Bun.Glob("*.json").scanSync(directory)) {
      const recipe = parseEditRecipe(await Bun.file(join(directory, path)).json());
      if (`${recipe.id} ${recipe.description}`.toLowerCase().includes(search.toLowerCase()))
        recipes.push(recipe);
    }
    return recipes;
  }
  async put(input: WorkSession, expectedKey: string | null) {
    const work = parseWorkSession(input),
      dir = await this.directory();
    return withWorkspacePublicationLock(join(dir, "PUBLISH.lock"), async () => {
      const current = await this.get(work.id).catch((e) => {
        if (e.code === "ENOENT") return null;
        throw e;
      });
      if ((current ? contentKey(current) : null) !== expectedKey)
        throw Error("Work changed; inspect and retry with its current key");
      const text = JSON.stringify(work, null, 2);
      if (Buffer.byteLength(text) > 64 * 1024 * 1024) throw Error("Work record exceeds 64 MiB");
      const temp = join(dir, `${work.id}-${crypto.randomUUID()}.tmp`);
      const handle = await open(
        temp,
        constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY | constants.O_NOFOLLOW,
        0o600,
      );
      try {
        await handle.writeFile(text);
        await handle.sync();
      } finally {
        await handle.close();
      }
      await rename(temp, join(dir, `${work.id}.json`));
      const folder = await open(dir, constants.O_RDONLY);
      try {
        await folder.sync();
      } finally {
        await folder.close();
      }
      return contentKey(work);
    });
  }
}
