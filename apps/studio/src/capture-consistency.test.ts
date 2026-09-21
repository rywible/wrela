import { expect, test } from "bun:test";
import type { WebGPURenderer } from "@wrela/render-webgpu";
import { StudioController } from "./controller";

test("multi-channel capture rejects changed render settings and preserves intervening camera input", async () => {
  const changes: ((controller: StudioController) => void)[] = [
    (controller) => controller.patch({ exposure: 2 }),
    (controller) => controller.patch({ wind: 0.5 }),
    (controller) => controller.patch({ stage: "studio" }),
    (controller) => controller.patch({ stageId: "another-stage" }),
    (controller) => controller.patch({ quality: "review" }),
    (controller) => controller.patch({ grid: true }),
    (controller) => controller.authoring.select("polar-bunny"),
    (controller) => controller.patch({ camera: { position: [40, 20, 30], target: [0, 0, 0], fov: 40 } }),
  ];
  for (const change of changes) {
    const controller = new StudioController();
    controller.patch({ revision: controller.authoring.getSnapshot().revision, status: "current" });
    controller.prepare = async () => ({ revision: controller.authoring.getSnapshot().revision, missing: [] });
    let captures = 0;
    let changedCamera = controller.getSnapshot().camera;
    controller.renderer = {
      completeness: { frame: 1, complete: true, rendered: [], culled: [], uploading: [], rejected: [] },
      async capture() {
        captures++;
        change(controller);
        changedCamera = controller.getSnapshot().camera;
        return new Blob(["first frame"]);
      },
    } as unknown as WebGPURenderer;
    await expect(controller.capture({ channels: ["beauty", "identity"] })).rejects.toThrow(
      "Capture view or clock changed",
    );
    expect(captures).toBe(1);
    expect(controller.getSnapshot().camera).toEqual(changedCamera);
  }
});
