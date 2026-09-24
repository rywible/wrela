import { constants } from "node:fs";
import { lstat, mkdir, open, realpath, rename } from "node:fs/promises";
import { join, relative, resolve, sep } from "node:path";
import { type EditBatch, parseEditBatch, type TransactionResult } from "@wrela/authoring";
import {
  contentKey,
  type DocumentDescriptor,
  type DocumentRepository,
  describeDocument,
  type Project,
  parseProject,
} from "@wrela/model";
import { withWorkspacePublicationLock } from "./bridge-lock";
import { prepareWorkspaceTransaction, WorkspaceConflict } from "./workspace-transactions";

/** Immutable generations and an atomic CURRENT pointer publish every document
 * together. Internal symbolic links are refused; the user-selected root itself
 * is canonicalized once. An OS file lock serializes the final conflict check and
 * pointer publication across bridge processes. Direct file editors must finish
 * their writes before saving through the bridge; they do not take this lock. */
export class WorkspaceBridge {
  private root: string;
  private serial: Promise<unknown> = Promise.resolve();
  constructor(root: string) {
    this.root = resolve(root);
  }
  private async contained(path: string): Promise<string> {
    const lexical = resolve(path),
      parts = relative(this.root, lexical).split(sep);
    if (parts[0] === ".." || parts.some((part) => part === ".."))
      throw new Error("Path is outside the workspace root");
    let current = this.root;
    for (const part of parts) {
      if (!part) continue;
      current = join(current, part);
      const info = await lstat(current);
      if (info.isSymbolicLink())
        throw new Error("Symbolic links are not permitted inside the workspace root");
    }
    const actual = await realpath(lexical);
    if (actual !== this.root && !actual.startsWith(this.root + sep))
      throw new Error("Path is outside the workspace root");
    return actual;
  }
  private async ensureDirectory(path: string): Promise<void> {
    try {
      await this.contained(path);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      await this.contained(resolve(path, ".."));
      try {
        await mkdir(path);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
      }
      await this.contained(path);
    }
  }
  private async readText(path: string): Promise<string> {
    await this.contained(path);
    const handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    try {
      const info = await handle.stat();
      if (!info.isFile() || info.size > 8_388_608)
        throw new Error("Workspace source file exceeds its size limit");
      return await handle.readFile("utf8");
    } finally {
      await handle.close();
    }
  }
  private async writeFresh(path: string, text: string): Promise<void> {
    await this.contained(resolve(path, ".."));
    const handle = await open(
      path,
      constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW,
      0o600,
    );
    try {
      await handle.writeFile(text);
      await handle.sync();
    } finally {
      await handle.close();
    }
  }
  private async syncDirectory(path: string): Promise<void> {
    const handle = await open(await this.contained(path), constants.O_RDONLY);
    try {
      await handle.sync();
    } finally {
      await handle.close();
    }
  }
  async initialize(): Promise<void> {
    await mkdir(this.root, { recursive: true });
    this.root = await realpath(this.root);
    await this.ensureDirectory(join(this.root, ".wrela"));
    await this.ensureDirectory(join(this.root, ".wrela", "generations"));
  }
  async read(): Promise<{ project: Project; key: string; generation: string } | null> {
    let generation: string;
    try {
      generation = (await this.readText(join(this.root, ".wrela", "CURRENT"))).trim();
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
      throw error;
    }
    return this.readGeneration(generation);
  }
  private async readGeneration(generation: string) {
    if (!/^[a-f0-9-]+$/.test(generation)) throw new Error("Invalid workspace generation");
    const dir = join(this.root, ".wrela", "generations", generation);
    const manifest = JSON.parse(await this.readText(join(dir, "project.json")));
    const files: unknown = manifest.documents;
    if (!Array.isArray(files) || files.length > 100_000) throw new Error("Invalid workspace manifest");
    const documents = await Promise.all(
      files.map(async (file: unknown) => {
        if (typeof file !== "string" || !/^[a-z]+\/[a-zA-Z0-9_-]+\.json$/.test(file))
          throw new Error("Invalid document path");
        return JSON.parse(await this.readText(join(dir, file)));
      }),
    );
    const { index: _index, ...source } = manifest;
    const project = parseProject({ ...source, documents });
    return { project, key: contentKey(project), generation };
  }
  /** Metadata-only lookup; inspecting one definition does not load the catalog's source. */
  async catalog(): Promise<DocumentRepository> {
    const generation = (await this.readText(join(this.root, ".wrela", "CURRENT"))).trim();
    if (!/^[a-f0-9-]+$/.test(generation)) throw Error("Invalid workspace generation");
    const dir = join(this.root, ".wrela", "generations", generation);
    const {
      documents: files,
      index,
      ...project
    } = JSON.parse(await this.readText(join(dir, "project.json")));
    if (!index) {
      const source = await this.readGeneration(generation);
      const documents = new Map(source.project.documents.map((d) => [d.id, d]));
      return {
        key: source.key,
        project,
        index: source.project.documents.map(describeDocument),
        async read(id) {
          const doc = documents.get(id);
          if (!doc) throw Error("Unknown definition");
          return doc;
        },
      };
    }
    if (
      !Array.isArray(files) ||
      !Array.isArray(index) ||
      files.length !== index.length ||
      index.length > 100_000
    )
      throw Error("Invalid catalog index");
    const byId = new Map<string, string>();
    for (let i = 0; i < files.length; i++) {
      if (
        !/^[a-z]+\/[a-zA-Z0-9_-]+\.json$/.test(files[i]) ||
        typeof index[i]?.id !== "string" ||
        byId.has(index[i].id)
      )
        throw Error("Invalid catalog path or identity");
      byId.set(index[i].id, files[i]);
    }
    const publication: Publication = JSON.parse(await this.readText(join(dir, "publication.json")));
    return {
      key: publication.sourceKey,
      project,
      index: index as DocumentDescriptor[],
      read: async (id) => {
        const path = byId.get(id);
        if (!path) throw Error(`Unknown definition ${id}`);
        return JSON.parse(await this.readText(join(dir, path)));
      },
    };
  }
  private async *history() {
    let generation = (await this.read())?.generation;
    const seen = new Set<string>();
    while (generation) {
      if (!/^[a-f0-9-]+$/.test(generation) || seen.has(generation))
        throw Error("Invalid publication history");
      seen.add(generation);
      let publication: Publication;
      try {
        publication = JSON.parse(
          await this.readText(join(this.root, ".wrela", "generations", generation, "publication.json")),
        );
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") return;
        throw error;
      }
      yield { generation, ...publication };
      generation = publication.parent ?? undefined;
    }
  }
  async transactionReceipt(transactionId: string) {
    for await (const entry of this.history())
      if (entry.receipt?.result.transactionId === transactionId) return entry.receipt;
    return undefined;
  }
  private async baseline(key: string) {
    const current = await this.read();
    if (current?.key === key) return current.project;
    for await (const entry of this.history())
      if (entry.sourceKey === key || (entry.parent && entry.parentSourceKey === key)) {
        const baseline = await this.readGeneration(
          entry.sourceKey === key ? entry.generation : entry.parent!,
        );
        if (baseline.key === key) return baseline.project;
        throw new WorkspaceConflict("Inspected source was edited externally; inspect again");
      }
    throw new WorkspaceConflict("Workspace baseline is unavailable; inspect again");
  }
  async transact(
    baselineKey: string,
    input: EditBatch,
    options: { preview?: boolean; exact?: boolean } = {},
  ) {
    const batch = parseEditBatch(input),
      fingerprint = contentKey({ baselineKey, batch, exact: options.exact === true });
    return withWorkspacePublicationLock(join(this.root, ".wrela", "PUBLISH.lock"), async () => {
      if (batch.transactionId) {
        const previous = await this.transactionReceipt(batch.transactionId);
        if (previous) {
          if (previous.fingerprint !== fingerprint)
            throw new WorkspaceConflict("A transaction ID cannot be reused for different work");
          return { ...previous.result, workspaceKey: previous.result.key, published: !options.preview };
        }
      }
      const current = await this.read();
      if (!current) throw Error("Workspace has no published source");
      if (options.exact && current.key !== baselineKey)
        throw new WorkspaceConflict("Reviewed source changed; review again");
      const prepared = prepareWorkspaceTransaction(await this.baseline(baselineKey), current.project, batch);
      if (!options.preview)
        await this.publish(prepared.project, current.key, { fingerprint, result: prepared.result }, true);
      return {
        ...prepared.result,
        workspaceKey: options.preview ? current.key : prepared.result.key,
        published: !options.preview,
      };
    });
  }
  save(project: Project, expectedKey: string | null) {
    const work = this.serial.then(() => this.publish(project, expectedKey));
    this.serial = work.catch(() => undefined);
    return work;
  }
  private async publish(
    input: Project,
    expectedKey: string | null,
    receipt?: WorkspaceReceipt,
    locked = false,
  ) {
    const project = parseProject(input),
      current = await this.read();
    if ((current?.key ?? null) !== expectedKey)
      throw new Error("Workspace changed externally. Reload before saving.");
    const generation = crypto.randomUUID(),
      dir = join(this.root, ".wrela", "generations", generation);
    await this.ensureDirectory(dir);
    const files: string[] = [],
      folders = new Set<string>();
    for (const document of project.documents) {
      const folder = `${document.kind}s`,
        file = `${folder}/${document.id}.json`;
      if (!folders.has(folder)) {
        await this.ensureDirectory(join(dir, folder));
        folders.add(folder);
      }
      await this.writeFresh(join(dir, file), JSON.stringify(document, null, 2));
      files.push(file);
    }
    for (const folder of folders) await this.syncDirectory(join(dir, folder));
    await this.writeFresh(
      join(dir, "project.json"),
      JSON.stringify(
        { ...project, documents: files, index: project.documents.map(describeDocument) },
        null,
        2,
      ),
    );
    await this.writeFresh(
      join(dir, "publication.json"),
      JSON.stringify({
        parent: current?.generation ?? null,
        parentSourceKey: current?.key,
        sourceKey: contentKey(project),
        receipt,
      } satisfies Publication),
    );
    await this.syncDirectory(dir);
    await this.syncDirectory(join(this.root, ".wrela", "generations"));
    const pointer = join(this.root, ".wrela", `CURRENT-${generation}`);
    await this.writeFresh(pointer, generation);
    const metadata = join(this.root, ".wrela");
    await this.contained(metadata);
    const commit = async () => {
      const latest = await this.read();
      if ((latest?.key ?? null) !== expectedKey)
        throw new Error("Workspace changed externally. Reload before saving.");
      await this.contained(metadata);
      await rename(pointer, join(metadata, "CURRENT"));
      await this.syncDirectory(metadata);
      return { key: contentKey(project), generation };
    };
    return locked ? commit() : withWorkspacePublicationLock(join(metadata, "PUBLISH.lock"), commit);
  }
}

type WorkspaceReceipt = { fingerprint: string; result: TransactionResult };
type Publication = {
  parent: string | null;
  parentSourceKey?: string;
  sourceKey: string;
  receipt?: WorkspaceReceipt;
};
