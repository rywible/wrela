import { describe, expect, test } from "bun:test";
import { StudioController } from "./controller";

describe("clay-first Studio inspection", () => {
  test("clay and skeleton are presentation-only and retain camera, pose source and other overlays", () => {
    const controller = new StudioController();
    controller.patch({
      camera: { position: [3, 2, 4], target: [0, 1, 0], fov: 42 },
      overlays: ["colliders"],
    });
    const source = structuredClone(controller.authoring.getSnapshot());
    const camera = structuredClone(controller.getSnapshot().camera);
    const time = controller.getSnapshot().time;
    const result = controller.creature.inspectView({ channel: "clay", skeleton: true, hideGroom: true });
    expect(result).toEqual({
      channel: "clay",
      overlays: ["colliders", "rig"],
      hideGroom: true,
      visualApproval: "not-reviewed",
    });
    expect(controller.authoring.getSnapshot()).toEqual(source);
    expect(controller.getSnapshot().camera).toEqual(camera);
    expect(controller.getSnapshot().time).toBe(time);
    expect(controller.host).toBeNull(); // No hidden runtime rebuild or graphics initialization.

    controller.creature.inspectView({ channel: "beauty", skeleton: false, hideGroom: false });
    expect(controller.getSnapshot().overlays).toEqual(["colliders"]);
    expect(controller.getSnapshot().camera).toEqual(camera);
    expect(controller.authoring.getSnapshot()).toEqual(source);
  });
  test("inspection rejects unsupported channels and unavailable pixel evidence", () => {
    const controller = new StudioController();
    expect(() => controller.creature.inspectView({ channel: "invented" as "clay" })).toThrow();
    expect(() => controller.creature.inspectPixel({ x: 20, y: 20 })).toThrow("finish rendering");
    expect(controller.getSnapshot().mode).toBe("beauty");
  });
});
