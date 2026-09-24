import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { createLookdevFixture } from "./fixtures/lookdev";

test("lookdev rejects a later frame sun override with GI before creating any browser or renderer resources", async () => {
  const camera = { position: [1, 1, 1], target: [0, 0, 0], fov: 45 } as const;
  await expect(
    createLookdevFixture({
      project: referenceProject(),
      subject: "unused-preflight-subject",
      indirectLighting: {},
      frames: [
        {
          id: "authored-sun",
          camera: { ...camera, position: [...camera.position], target: [...camera.target] },
        },
        {
          id: "backlight",
          camera: { ...camera, position: [...camera.position], target: [...camera.target] },
          sunDirection: [0, 1, 0],
        },
      ],
    }),
  ).rejects.toThrow("backlight combines indirect lighting with an unsupported sunDirection override");
});
