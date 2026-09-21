import { constants } from "node:fs";
import { lstat, mkdir, open, realpath, rename } from "node:fs/promises";
import { join, relative, resolve, sep } from "node:path";
import { contentKey, type Project, parseProject } from "@wrela/model";
import { withWorkspacePublicationLock } from "./bridge-lock";

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
  async read(): Promise<{ project: Project; key: string } | null> {
    let generation: string;
    try {
      generation = (await this.readText(join(this.root, ".wrela", "CURRENT"))).trim();
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
      throw error;
    }
    if (!/^[a-f0-9-]+$/.test(generation)) throw new Error("Invalid workspace generation");
    const dir = join(this.root, ".wrela", "generations", generation);
    const manifest = JSON.parse(await this.readText(join(dir, "project.json")));
    const files: unknown = manifest.documents;
    if (!Array.isArray(files) || files.length > 256) throw new Error("Invalid workspace manifest");
    const documents = await Promise.all(
      files.map(async (file: unknown) => {
        if (typeof file !== "string" || !/^[a-z]+\/[a-zA-Z0-9_-]+\.json$/.test(file))
          throw new Error("Invalid document path");
        return JSON.parse(await this.readText(join(dir, file)));
      }),
    );
    const project = parseProject({ ...manifest, documents });
    return { project, key: contentKey(project) };
  }
  save(project: Project, expectedKey: string | null) {
    const work = this.serial.then(() => this.publish(project, expectedKey));
    this.serial = work.catch(() => undefined);
    return work;
  }
  private async publish(input: Project, expectedKey: string | null) {
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
      JSON.stringify({ ...project, documents: files }, null, 2),
    );
    await this.syncDirectory(dir);
    await this.syncDirectory(join(this.root, ".wrela", "generations"));
    const pointer = join(this.root, ".wrela", `CURRENT-${generation}`);
    await this.writeFresh(pointer, generation);
    const metadata = join(this.root, ".wrela");
    await this.contained(metadata);
    return withWorkspacePublicationLock(join(metadata, "PUBLISH.lock"), async () => {
      const latest = await this.read();
      if ((latest?.key ?? null) !== expectedKey)
        throw new Error("Workspace changed externally. Reload before saving.");
      await this.contained(metadata);
      await rename(pointer, join(metadata, "CURRENT"));
      await this.syncDirectory(metadata);
      return { key: contentKey(project), generation };
    });
  }
}
