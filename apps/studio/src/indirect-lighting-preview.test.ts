import { expect, spyOn, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import type { Camera } from "@wrela/model";
import { BrowserSceneHost } from "@wrela/runtime";
import { StudioController } from "./controller";

test("ordinary preview camera changes preserve common lighting without starting the reference solver", () => {
  const controller = new StudioController(),
    host = new BrowserSceneHost(referenceProject()),
    configure = spyOn(host, "configureIndirectLighting");
  controller.host = host;
  const camera: Camera = { position: [110, 8, 35], target: [100, 4, 20], fov: 48 };
  try {
    controller.patch({ camera });
    controller.orbit(12, 4);
    controller.zoom(30);
    controller.pan(1, 0);
    controller.patch({ camera: { ...camera, target: [130, 4, 20] } });
    expect(controller.getSnapshot().camera.target).toEqual([130, 4, 20]);
    expect(configure).not.toHaveBeenCalled();
    expect(host.indirectLightingProgress).toBeUndefined();
    expect(host.resourceUsage.indirectLightingBytes).toBe(0);
  } finally {
    configure.mockRestore();
    host.dispose();
  }
});
