import { expect, spyOn, test } from "bun:test";
import { compileCharacter, queryWater, resolveWaterWaves } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import { type Camera, environmentStateSchema, parseProject } from "@wrela/model";
import { evaluateEnvironmentState, evaluateWaterEnvironment } from "./environment";
import { BrowserSceneHost } from "./scene-host";

test("weather water agrees in extraction, fixed-step buoyancy, seeking and source roundtrip", async () => {
  const project = referenceProject();
  const water = project.documents.find((d) => d.kind === "water");
  const environment = project.documents.find((d) => d.kind === "environment");
  const stage = project.documents.find((d) => d.kind === "stage");
  const character = project.documents.find((d) => d.kind === "character");
  if (
    water?.kind !== "water" ||
    environment?.kind !== "environment" ||
    stage?.kind !== "stage" ||
    character?.kind !== "character"
  )
    throw new Error("Missing fixtures");
  stage.environment = environment.id;
  water.flow = {
    velocity: [0.2, 0.1],
    weatherResponse: { referenceWindSpeed: 0, waveGain: 0.3, roughnessGain: 0.02 },
  };
  const calm = environmentStateSchema.parse({
    sunElevation: 0.5,
    sunAzimuth: 1,
    turbidity: 2,
    fogDensity: 0,
    wind: [0, 0, 0],
    ambient: 1,
    sunIntensity: 2,
  });
  environment.sequence = {
    duration: 1,
    loop: false,
    interpolation: "linear",
    keyframes: [
      { time: 0, state: calm },
      { time: 1, state: { ...calm, wind: [5, 0, 0], wetness: 1 } },
    ],
  };
  const saved = JSON.stringify(project);
  const host = new BrowserSceneHost(project);
  const camera: Camera = { position: [8, 5, 12], target: [0, 0, 0], fov: 45 };
  try {
    await host.prepare(water.id, stage.id);
    const runtime = host.runtime;
    if (!runtime) throw new Error("Missing runtime");
    const bodyDefinition = { ...character, physics: { ...character.physics, mode: "kinematic" as const } };
    runtime.addCharacter("water-probe", compileCharacter(bodyDefinition, "interactive"), bodyDefinition, [
      1,
      water.level,
      2,
    ]);
    const buoyancy = spyOn(runtime.physics, "buoyancy");
    const first = host.evaluate(0, camera).surfaces.find((surface) => surface.water);
    const middleScene = host.evaluate(0.5, camera);
    const middle = middleScene.surfaces.find((surface) => surface.water);
    const expected = evaluateWaterEnvironment(water, evaluateEnvironmentState(environment, undefined, 0.5));
    expect(middle?.water?.waves).toEqual(resolveWaterWaves(expected));
    expect(middle?.material.roughness).toBeCloseTo(expected.roughness);
    expect(middle?.water?.waves[0].amplitude).toBeGreaterThan(first?.water?.waves[0].amplitude ?? Infinity);
    expect(middle?.mesh).toBe(first?.mesh);
    expect(middleScene.time).toBe(0.5);
    expect(buoyancy.mock.calls.length).toBe(30);
    const lastCall = buoyancy.mock.calls.at(-1);
    const position = runtime.bodyState("water-probe").position;
    const sample = queryWater(expected, position[0], position[2], 0.5);
    expect(lastCall?.[1]).toBeCloseTo(sample.height, 7);
    expect(lastCall?.[2]).toEqual(sample.velocity);
    const extracted = host.extract(camera).surfaces.find((surface) => surface.water);
    expect(extracted?.water).toEqual(middle?.water);
    await host.seek(0);
    await host.seek(0.5);
    expect(host.extract(camera).surfaces.find((surface) => surface.water)?.water).toEqual(middle?.water);
    buoyancy.mockRestore();
    expect(JSON.stringify(project)).toBe(saved);
    expect(parseProject(JSON.parse(saved)).documents.find((document) => document.id === water.id)).toEqual(
      water,
    );
    // Parameter publication reconfigures the resolver without resetting the clock.
    const edited = structuredClone(project);
    const editedEnvironment = edited.documents.find((document) => document.id === environment.id);
    if (editedEnvironment?.kind !== "environment" || !editedEnvironment.sequence)
      throw new Error("Missing editable weather");
    editedEnvironment.sequence.keyframes[1].state.wind = [10, 0, 0];
    await host.setProject(edited);
    expect(host.runtime).toBe(runtime);
    expect(
      host.extract(camera).surfaces.find((surface) => surface.water)?.water?.waves[0].amplitude,
    ).toBeGreaterThan(middle?.water?.waves[0].amplitude ?? Infinity);
    const updatedBuoyancy = spyOn(runtime.physics, "buoyancy");
    host.advance(1 / 60);
    const updated = evaluateWaterEnvironment(
      water,
      evaluateEnvironmentState(editedEnvironment, undefined, runtime.clock.time),
    );
    const updatedPosition = runtime.bodyState("water-probe").position;
    expect(updatedBuoyancy.mock.calls.at(-1)?.[1]).toBeCloseTo(
      queryWater(updated, updatedPosition[0], updatedPosition[2], runtime.clock.time).height,
      7,
    );
    updatedBuoyancy.mockRestore();
  } finally {
    host.dispose();
  }
});
