import { describe, expect, test } from "bun:test";
import { AuthoringSession } from "@wrela/authoring";
import { referenceProject } from "@wrela/examples";
import { creatureSchema } from "@wrela/model";
import { createCreatureStudioTools } from "./creature-tools";

function subject() {
  const project = referenceProject();
  const character = project.documents.find((document) => document.kind === "character");
  if (character?.kind !== "character") throw Error("Reference character missing");
  character.creature = creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      {
        id: "body",
        name: "Body",
        nodeIds: [character.field.root],
        jointIds: [],
        frame: { position: [0, 0, 0], rotation: [0, 0, 0] },
        extent: [1, 1, 1],
      },
    ],
  });
  const session = new AuthoringSession(project);
  return { session, tools: createCreatureStudioTools(session), character };
}

describe("Studio creature authoring parity", () => {
  test("agent selection and inspection resolve the same source used by the panel", () => {
    const { session, tools, character } = subject();
    expect(tools.inspect(character.id, "body")).toEqual(session.inspectCreature(character.id, "body"));
    expect(tools.select(character.id, [0, 0, 0])).toEqual(session.selectCreature(character.id, [0, 0, 0]));
    expect(tools.explain(character.id, "body")).toEqual(session.explainCreature(character.id, "body"));
  });
  test("candidate controls isolate changes, enforce original preconditions, and use normal undo", () => {
    const { session, tools, character } = subject();
    const originalName = character.name;
    tools.candidates.propose({
      id: "name-study",
      batch: {
        expectedRevision: 0,
        operations: [{ kind: "document.set", target: character.id, path: ["name"], value: "Variation" }],
      },
    });
    expect(session.getSnapshot().revision).toBe(0);
    expect(session.getSnapshot().project.documents.find((item) => item.id === character.id)?.name).toBe(
      originalName,
    );
    expect(tools.candidates.list()[0].status).toBe("proposed");
    tools.candidates.adopt("name-study");
    expect(session.getSnapshot().project.documents.find((item) => item.id === character.id)?.name).toBe(
      "Variation",
    );
    session.undo();
    expect(session.getSnapshot().project.documents.find((item) => item.id === character.id)?.name).toBe(
      originalName,
    );

    tools.candidates.propose({
      id: "stale-study",
      batch: {
        expectedRevision: session.getSnapshot().revision,
        operations: [
          { kind: "document.set", target: character.id, path: ["name"], value: "Stale variation" },
        ],
      },
    });
    session.apply({
      expectedRevision: session.getSnapshot().revision,
      operations: [
        { kind: "document.set", target: character.id, path: ["name"], value: "New accepted work" },
      ],
    });
    expect(() => tools.candidates.adopt("stale-study")).toThrow();
    expect(session.getSnapshot().project.documents.find((item) => item.id === character.id)?.name).toBe(
      "New accepted work",
    );
  });
});
