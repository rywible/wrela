import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { createSurfaceAppearance } from "@wrela/model";
import { AuthoringSession } from "./session";

test("an unknown nested appearance key rejects the whole authoring edit without losing source", () => {
  const session = new AuthoringSession(referenceProject());
  expect(() =>
    session.apply({
      expectedRevision: 0,
      label: "Typo",
      operations: [
        {
          kind: "document.set",
          target: "pine-needles",
          path: ["appearance"],
          value: { ...createSurfaceAppearance(), layres: [] },
        },
      ],
    }),
  ).toThrow("Unknown authored property");
  expect(session.getSnapshot().revision).toBe(0);
  session.apply({
    expectedRevision: 0,
    label: "Appearance",
    operations: [
      { kind: "document.set", target: "pine-needles", path: ["appearance"], value: { family: "foliage" } },
    ],
  });
  const material = session.getSnapshot().project.documents.find((d) => d.id === "pine-needles");
  expect(material?.kind === "material" && material.appearance?.family).toBe("foliage");
  session.undo();
  expect(session.getSnapshot().project.documents.find((d) => d.id === "pine-needles")).toEqual(
    referenceProject().documents.find((d) => d.id === "pine-needles"),
  );
});
