import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import type { EvaluatedScene } from "@wrela/model";
import { indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import { evaluateEnvironment } from "./environment";
import { BrowserSceneHost } from "./scene-host";

test("disabling preview lighting removes the field from a retained scene and releases its payload", async () => {
  const fixture = indirectBoxFixture({ occluder: false }),
    host = new BrowserSceneHost(referenceProject());
  const scene: EvaluatedScene = {
    surfaces: fixture.surfaces,
    camera: fixture.camera,
    environment: evaluateEnvironment(),
    time: 0,
    mode: "beauty",
    grid: false,
  };
  try {
    host.configureIndirectLighting({
      dimensions: [2, 2, 2],
      lighting: fixture.lighting,
      samples: 16,
      skySamples: 1,
    });
    host.applyIndirectLighting(scene);
    await host.waitForIndirectLighting();
    host.applyIndirectLighting(scene);
    expect(scene.indirectLighting?.report.status).toBe("ready");
    expect(host.resourceUsage.indirectLightingBytes).toBeGreaterThan(0);

    host.configureIndirectLighting(undefined);
    host.applyIndirectLighting(scene);
    expect(scene.indirectLighting).toBeUndefined();
    expect(host.indirectLightingProgress).toBeUndefined();
    expect(host.indirectLightingRevision).toBe("none");
    expect(host.resourceUsage.indirectLightingBytes).toBe(0);
  } finally {
    host.dispose();
  }
});
