import { join, resolve } from "node:path";
import { artifactTransfers, clearCompilerCaches, compileDocument } from "@wrela/compiler";
import { type CreatureFixtureId, createCreatureFixture } from "@wrela/examples";
import { type CharacterDefinition, contentKey } from "@wrela/model";
import { RuntimeSession } from "@wrela/runtime";
import { distribution, sourceManifest, writeEvidence } from "./evidence";

/** CPU workload only. These measurements exclude rendering and display pacing. */
export async function benchmarkCreature(id: CreatureFixtureId, actors: number, ticks = 120) {
  if (![1, 4].includes(actors) || !Number.isInteger(ticks) || ticks < 30 || ticks > 600)
    throw Error("Use one or four actors and 30–600 measured ticks");
  const fixture = createCreatureFixture(id);
  const source = fixture.project.documents.find(
    (doc) => doc.id === fixture.characterId,
  ) as CharacterDefinition;
  clearCompilerCaches();
  const compileStart = performance.now();
  const artifact = compileDocument(source, "review");
  const compilationMilliseconds = performance.now() - compileStart;
  if (artifact?.kind !== "character") throw Error("Expected creature artifact");
  const runtime = await RuntimeSession.create();
  try {
    runtime.physics.addGround();
    runtime.setCreatureSecondaryGroundPlane({ height: 0, minX: -1000, maxX: 1000, minZ: -1000, maxZ: 1000 });
    for (let actor = 0; actor < actors; actor++) {
      const instance = `${id}-${actor}`;
      runtime.addCharacter(instance, artifact, source, [actor * 10, 0, 0]);
      runtime.playMotion(instance, "walk", 0);
    }
    const step: number[] = [],
      evaluation: number[] = [],
      total: number[] = [];
    let evaluatedVertices = 0;
    for (let tick = 0; tick < ticks + 30; tick++) {
      const begin = performance.now();
      runtime.advance(1 / 60);
      const posed = performance.now();
      const instances = runtime.evaluatedCharacters(runtime.clock.time, [0, 0, 0]);
      const done = performance.now();
      evaluatedVertices = instances.reduce(
        (sum, instance) => sum + instance.artifact.mesh.positions.length / 3,
        0,
      );
      if (tick >= 30) {
        step.push(posed - begin);
        evaluation.push(done - posed);
        total.push(done - begin);
      }
    }
    const combined = distribution(total)!;
    return {
      fixture: id,
      actors,
      sourceKey: contentKey(source),
      artifactKey: artifact.key,
      quality: "review",
      detail: "hero",
      motion: "walk",
      secondaryGround: runtime.creatureGroundPolicy,
      timestepSeconds: 1 / 60,
      warmupTicks: 30,
      measuredTicks: ticks,
      compilationMilliseconds,
      uniqueTypedArrayBytes: artifactTransfers(artifact).reduce((sum, buffer) => sum + buffer.byteLength, 0),
      verticesPerActor: artifact.mesh.positions.length / 3,
      evaluatedVertices,
      materialDrawGroupsPerActor: artifact.mesh.materialGroups?.length ?? 1,
      groomGuides: artifact.creatureGroom?.guides.length ?? 0,
      milliseconds: { simulation: distribution(step), extraction: distribution(evaluation), combined },
      provisionalCpuOnlyBudget: { milliseconds: 16.667, p95WithinBudget: combined.p95 <= 16.667 },
      limitations: [
        "CPU time on this host, without rendering, shadows, upload, browser overhead, world streaming or display pacing.",
        "Actors share immutable compiled products and use spatially separated flat-ground motion.",
        "The 16.667ms comparison consumes the entire 60Hz frame budget; passing it does not establish room for rendering.",
        "Global geometry/binding caches are cleared before compile; other module/JIT caches may remain warm.",
      ],
    };
  } finally {
    runtime.dispose();
  }
}

if (import.meta.main) {
  const output = resolve(
    process.argv.find((arg) => arg.startsWith("--output="))?.slice(9) ?? "output/creature-cpu-benchmark",
  );
  const before = await writeEvidence(output, "creature-cpu-benchmark");
  const workloads = [];
  for (const id of ["ash-warden", "reed-penitent"] as const)
    for (const actors of [1, 4]) workloads.push(await benchmarkCreature(id, actors));
  const after = await sourceManifest("creature-cpu-benchmark");
  const sourceStable = before.sourceFingerprint === after.sourceFingerprint;
  await Bun.write(join(output, "results.json"), JSON.stringify({ sourceStable, workloads }, null, 2));
  console.log(
    JSON.stringify(
      {
        output,
        sourceStable,
        workloads: workloads.map(({ fixture, actors, milliseconds }) => ({
          fixture,
          actors,
          combined: milliseconds.combined,
        })),
      },
      null,
      2,
    ),
  );
  if (!sourceStable)
    throw Error(
      "Source changed during benchmark; retained development observations are not stable-source evidence",
    );
}
