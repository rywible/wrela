import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { contentKey } from "@wrela/model";
import {
  compileDocument,
  cookProject,
  cookProjectAsync,
  createCookedCompiler,
  deserializeArtifact,
  loadCookedProject,
  serializeArtifact,
} from "./index";

test("cooked JSON restores typed products without recompilation and checks source/compiler identities", async () => {
  const project = referenceProject(),
    bundle = cookProject(project, "interactive", "test-compiler-source");
  const reopened = JSON.parse(JSON.stringify(bundle));
  const provider = createCookedCompiler(reopened, project, { compilerSource: "test-compiler-source" });
  const character = project.documents.find((document) => document.kind === "character");
  if (!character) throw new Error("Missing fixture character");
  const artifact = await provider(character, "interactive");
  expect(artifact?.kind).toBe("character");
  if (!artifact || artifact.kind !== "character") throw new Error("Missing character product");
  expect(artifact.mesh.positions).toBeInstanceOf(Float32Array);
  expect(artifact.mesh.indices).toBeInstanceOf(Uint32Array);
  expect(artifact.jointIndices).toBeInstanceOf(Uint16Array);
  const direct = compileDocument(character, "interactive");
  if (!direct) throw new Error("Missing direct character product");
  expect(serializeArtifact(artifact)).toEqual(serializeArtifact(direct));
  await expect(provider(character, "export")).rejects.toThrow("compatible cooked");
  expect(() => createCookedCompiler(reopened, project, { compilerSource: "different-build" })).toThrow(
    "incompatible",
  );
  expect(() => loadCookedProject({ ...reopened, compiler: "future" }, project)).toThrow();
  expect(() => loadCookedProject(reopened, { ...project, name: "Changed source" })).toThrow("incompatible");
});
test("corrupted cooked payloads fail before installation, including rehashed invalid indices", () => {
  const project = referenceProject(),
    bundle = cookProject(project, "interactive");
  const entry = bundle.entries.find((entry) => entry.document === "river-stone");
  if (!entry) throw new Error("Missing fixture stone");
  const payload = bundle.artifacts[entry.artifact] as { mesh: { indices: number[] } };
  payload.mesh.indices[0] = 249999;
  expect(() => loadCookedProject(bundle, project)).toThrow("identity");
  entry.artifact = contentKey(payload);
  bundle.artifacts[entry.artifact] = payload;
  expect(() => loadCookedProject(bundle, project)).toThrow("layout");
  expect(() => deserializeArtifact({ kind: "character" })).toThrow();
});
test("worker-provider cooking shares manifest semantics and missing products cannot claim completion", async () => {
  const project = referenceProject();
  const asyncBundle = await cookProjectAsync(
    project,
    "interactive",
    async (document, quality) => compileDocument(document, quality),
    "worker-build",
  );
  expect(asyncBundle).toEqual(cookProject(project, "interactive", "worker-build"));
  await expect(cookProjectAsync(project, "interactive", async () => null)).rejects.toThrow("did not produce");
});
