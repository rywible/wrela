import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { referenceProject } from "@wrela/model";
import { WorkspaceBridge } from "./bridge";

let root: string, bridge: WorkspaceBridge;
beforeEach(async () => {
  root = await mkdtemp(join(tmpdir(), "wrela-bridge-test-"));
  bridge = new WorkspaceBridge(root);
  await bridge.initialize();
});
afterEach(async () => {
  await rm(root, { recursive: true, force: true });
});
describe("workspace publication", () => {
  test("fresh workspace saves and reopens a multi-document generation", async () => {
    expect(await bridge.read()).toBeNull();
    const project = referenceProject(),
      saved = await bridge.save(project, null),
      read = await bridge.read();
    expect(read?.project).toEqual(project);
    expect(read?.key).toBe(saved.key);
    const manifest = JSON.parse(
      await readFile(join(root, ".wrela", "generations", saved.generation, "project.json"), "utf8"),
    );
    expect(manifest.documents).toHaveLength(project.documents.length);
    expect(manifest.documents.every((file: unknown) => typeof file === "string")).toBe(true);
  });
  test("interrupted unpublished generation never exposes a partial edit", async () => {
    const project = referenceProject(),
      saved = await bridge.save(project, null),
      orphan = join(root, ".wrela", "generations", "deadbeef");
    await mkdir(orphan);
    await writeFile(join(orphan, "project.json"), "{truncated");
    await writeFile(join(root, ".wrela", "CURRENT-deadbeef"), "deadbeef");
    expect((await bridge.read())?.project).toEqual(project);
    expect((await bridge.read())?.key).toBe(saved.key);
  });
  test("external source changes refuse overwrite and remain recoverable", async () => {
    const project = referenceProject(),
      saved = await bridge.save(project, null),
      doc = project.documents[0],
      file = join(root, ".wrela", "generations", saved.generation, `${doc.kind}s`, `${doc.id}.json`),
      changed = { ...doc, name: "Changed outside Studio" };
    await writeFile(file, JSON.stringify(changed));
    await expect(bridge.save(project, saved.key)).rejects.toThrow("changed externally");
    const read = await bridge.read();
    expect(read?.project.documents[0].name).toBe(changed.name);
    expect(read?.key).not.toBe(saved.key);
  });
  test("serial saves reject a stale competing revision without poisoning the queue", async () => {
    const project = referenceProject(),
      saved = await bridge.save(project, null),
      a = { ...project, name: "A" },
      b = { ...project, name: "B" },
      results = await Promise.allSettled([bridge.save(a, saved.key), bridge.save(b, saved.key)]);
    expect(results.map((r) => r.status)).toEqual(["fulfilled", "rejected"]);
    const current = await bridge.read();
    expect(current?.project.name).toBe("A");
    if (!current) throw new Error("Expected current project");
    await bridge.save(b, current.key);
    expect((await bridge.read())?.project.name).toBe("B");
  });
  test("independent bridge objects serialize publication to the same workspace", async () => {
    const other = new WorkspaceBridge(root);
    await other.initialize();
    const project = referenceProject(),
      saved = await bridge.save(project, null),
      results = await Promise.allSettled([
        bridge.save({ ...project, name: "A" }, saved.key),
        other.save({ ...project, name: "B" }, saved.key),
      ]);
    expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    expect(results.filter((result) => result.status === "rejected")).toHaveLength(1);
  });
  test("concurrent initialization shares fresh metadata directories", async () => {
    await rm(join(root, ".wrela"), { recursive: true });
    await Promise.all([bridge.initialize(), new WorkspaceBridge(root).initialize()]);
    await bridge.save(referenceProject(), null);
    expect(await bridge.read()).not.toBeNull();
  });
  test("rejects traversal in generation pointers and manifest paths", async () => {
    const saved = await bridge.save(referenceProject(), null);
    await writeFile(join(root, ".wrela", "CURRENT"), "../../outside");
    await expect(bridge.read()).rejects.toThrow("Invalid workspace generation");
    await writeFile(join(root, ".wrela", "CURRENT"), saved.generation);
    const path = join(root, ".wrela", "generations", saved.generation, "project.json"),
      manifest = JSON.parse(await readFile(path, "utf8"));
    manifest.documents = ["../../outside.json"];
    await writeFile(path, JSON.stringify(manifest));
    await expect(bridge.read()).rejects.toThrow("Invalid document path");
  });
  test("rejects invalid source before changing the active pointer", async () => {
    const project = referenceProject(),
      saved = await bridge.save(project, null),
      invalid = structuredClone(project);
    invalid.documents[0].id = "../escape";
    await expect(bridge.save(invalid, saved.key)).rejects.toThrow();
    expect((await bridge.read())?.key).toBe(saved.key);
  });
});

describe("workspace symlink containment", () => {
  test("a document symlink cannot read a file outside the selected root", async () => {
    const project = referenceProject(),
      saved = await bridge.save(project, null),
      doc = project.documents[0];
    const external = await mkdtemp(join(tmpdir(), "wrela-bridge-external-"));
    try {
      const file = join(root, ".wrela", "generations", saved.generation, `${doc.kind}s`, `${doc.id}.json`);
      const target = join(external, "private.json");
      await writeFile(target, JSON.stringify(doc));
      await rm(file);
      await symlink(target, file);
      await expect(bridge.read()).rejects.toThrow(/outside|symlink|root|symbolic/i);
    } finally {
      await rm(external, { recursive: true, force: true });
    }
  });
  test("initialization refuses a linked metadata directory outside the root", async () => {
    const external = await mkdtemp(join(tmpdir(), "wrela-bridge-external-"));
    try {
      await rm(join(root, ".wrela"), { recursive: true });
      await symlink(external, join(root, ".wrela"));
      await expect(new WorkspaceBridge(root).initialize()).rejects.toThrow(/outside|symlink|root|symbolic/i);
    } finally {
      await rm(external, { recursive: true, force: true });
    }
  });
});

test("an external edit during generation publication wins over a stale save", async () => {
  const project = referenceProject(),
    saved = await bridge.save(project, null),
    document = project.documents[0];
  const file = join(
    root,
    ".wrela",
    "generations",
    saved.generation,
    `${document.kind}s`,
    `${document.id}.json`,
  );
  const originalRead = bridge.read.bind(bridge);
  let reads = 0;
  bridge.read = async () => {
    const value = await originalRead();
    if (++reads === 1) await writeFile(file, JSON.stringify({ ...document, name: "Edited during save" }));
    return value;
  };
  await expect(bridge.save({ ...project, name: "Stale incoming save" }, saved.key)).rejects.toThrow(
    "changed externally",
  );
  bridge.read = originalRead;
  expect((await bridge.read())?.project.documents[0].name).toBe("Edited during save");
  expect((await readFile(join(root, ".wrela", "CURRENT"), "utf8")).trim()).toBe(saved.generation);
});
