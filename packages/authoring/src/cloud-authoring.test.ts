import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { environmentStateSchema } from "@wrela/model";
import { AuthoringSession } from "./session";

const cloud = { development: 0.6, storminess: 0, highCloudCover: 0.2 };
const tower = {
  id: "tower-1",
  kind: "tower",
  center: [0, 8000],
  base: 1200,
  size: [2200, 3800, 1900],
  yaw: 0,
  density: 1,
  erosion: 0.3,
  seed: 17,
};

test("cloud controls extend old documents, round-trip undo, and isolate weather keys", () => {
  const session = new AuthoringSession(referenceProject());
  const edit = (path: (string | number)[], value: unknown) =>
    session.apply({
      expectedRevision: session.getSnapshot().revision,
      operations: [{ kind: "document.set", target: "winter-sky", path, value }],
    });
  edit(["cloudscape"], cloud);
  edit(["cloudCover"], 0.48);
  edit(["cloudscape", "formations"], [tower]);
  edit(["cloudscape", "background"], 0.3);
  edit(["cloudscape", "midCloudCover"], 0.4);
  edit(["cloudscape", "midCloudHeight"], 7000);
  edit(["cloudscape", "midCloudYaw"], -0.8);
  edit(["cloudscape", "highCloudHeight"], 11000);
  edit(["cloudscape", "formations", 0, "maturity"], 0.7);
  edit(["cloudscape", "formations", 0, "shear"], -0.4);
  edit(["cloudscape", "highCloudYaw"], 0.8);
  session.undo();
  expect(session.inspect("winter-sky")).toMatchObject({
    cloudscape: { formations: [{ ...tower, maturity: 0.7, shear: -0.4 }], background: 0.3 },
  });
  session.redo();
  expect(session.inspect("winter-sky")).toMatchObject({ cloudscape: { highCloudYaw: 0.8 } });
  const state = environmentStateSchema.parse({
    sunElevation: 0.2,
    sunAzimuth: 0.5,
    turbidity: 2,
    fogDensity: 0,
    wind: [0, 0, 0],
    ambient: 0.4,
    sunIntensity: 2,
  });
  edit(["sequence"], {
    duration: 10,
    loop: false,
    interpolation: "linear",
    keyframes: [
      { time: 0, state },
      { time: 10, state },
    ],
  });
  edit(["sequence", "keyframes", 0, "state", "cloudscape"], cloud);
  edit(["sequence", "keyframes", 0, "state", "cloudCover"], 0.6);
  edit(["sequence", "keyframes", 0, "state", "cloudscape", "formations"], []);
  expect(session.inspect("winter-sky")).toMatchObject({
    cloudCover: 0.48,
    cloudscape: { formations: [{ ...tower, maturity: 0.7, shear: -0.4 }] },
    sequence: { keyframes: [{ state: { cloudCover: 0.6, cloudscape: { formations: [] } } }, { state }] },
  });
});

test("optional cloud controls reject unknown paths and invalid envelopes atomically", () => {
  const session = new AuthoringSession(referenceProject());
  session.apply({
    expectedRevision: 0,
    operations: [{ kind: "document.set", target: "winter-sky", path: ["cloudscape"], value: cloud }],
  });
  for (const [path, value] of [
    [["cloudscape", "unknown"], true],
    [["cloudscape", "formations"], [{ ...tower, size: [2200, 6000, 1900] }]],
    [["cloudscape", "highCloudYaw"], Number.NaN],
    [["cloudscape", "midCloudHeight"], 5000],
  ] as const) {
    expect(() =>
      session.apply({
        expectedRevision: 1,
        operations: [{ kind: "document.set", target: "winter-sky", path: [...path], value }],
      }),
    ).toThrow();
    expect(session.getSnapshot().revision).toBe(1);
  }
});
