import { describe, expect, test } from "bun:test";
import { AuthoringSession } from "@wrela/authoring";
import { referenceProject } from "@wrela/examples";
import { contentKey } from "@wrela/model";
import {
  type CandidatePreviewResource,
  candidatePreviewProjects,
  previewCreatureCandidate,
} from "./creature-candidate-preview";

function setup() {
  const session = new AuthoringSession(referenceProject());
  const character = session.getSnapshot().project.documents.find((document) => document.kind === "character");
  if (!character) throw Error("Reference character missing");
  session.proposeCandidate({
    id: "variant",
    batch: {
      expectedRevision: 0,
      operations: [
        { kind: "document.set", target: character.id, path: ["name"], value: "Unaccepted creature" },
      ],
    },
  });
  return {
    session,
    character,
    options: {
      subject: character.id,
      camera: {
        position: [4, 3, 5] as [number, number, number],
        target: [0, 1, 0] as [number, number, number],
        fov: 48,
      },
    },
  };
}

describe("isolated candidate review", () => {
  test("pose sequences synchronize both versions and reuse one bounded resource", async () => {
    const { session, options } = setup();
    const before = structuredClone(session.getSnapshot());
    const ticks: number[] = [];
    let created = 0,
      disposed = 0;
    const result = await previewCreatureCandidate(
      session,
      "variant",
      { ...options, ticks: [0, 20, 60], channel: "binding", overlays: ["rig"], hideGroom: true },
      async () => {
        created++;
        return {
          render: async (_project, settings) => {
            ticks.push(settings.tick);
            expect(settings.channel).toBe("binding");
            expect(settings.overlays).toEqual(["rig"]);
            expect(settings.hideGroom).toBe(true);
            return { blob: new Blob([String(settings.tick)]) };
          },
          dispose: () => {
            disposed++;
          },
        };
      },
    );
    expect(ticks).toEqual([0, 0, 20, 20, 60, 60]);
    expect(result.frames.map((frame) => frame.tick)).toEqual([0, 20, 60]);
    expect(result.metadata.runtimeTicks).toEqual([0, 20, 60]);
    expect(created).toBe(1);
    expect(disposed).toBe(1);
    expect(session.getSnapshot()).toEqual(before);
  });
  test("sequence budget and ordering are enforced before resource allocation", async () => {
    const { session, options } = setup();
    let allocated = false;
    const create = async () => {
      allocated = true;
      throw Error("Unexpected graphics allocation");
    };
    await expect(
      previewCreatureCandidate(session, "variant", { ...options, ticks: [0, 20, 10] }, create),
    ).rejects.toThrow("increasing");
    await expect(
      previewCreatureCandidate(
        session,
        "variant",
        { ...options, ticks: [0, 1, 2, 3, 4, 5, 6, 7], width: 512, height: 512 },
        create,
      ),
    ).rejects.toThrow("image budget");
    expect(allocated).toBe(false);
  });
  test("renders matched independent projects without touching accepted source, revision or selection", async () => {
    const { session, character, options } = setup();
    const before = structuredClone(session.getSnapshot());
    const rendered: string[] = [];
    let disposalCount = 0;
    const result = await previewCreatureCandidate(session, "variant", options, async () => ({
      render: async (project, settings) => {
        rendered.push(project.documents.find((document) => document.id === character.id)?.name ?? "missing");
        expect(settings.width).toBe(320);
        // A renderer may annotate its private input; accepted source remains frozen and independent.
        project.name = "private evaluation";
        return { blob: new Blob(["image"]) };
      },
      dispose: () => {
        disposalCount++;
      },
    }));
    expect(rendered).toEqual([character.name, "Unaccepted creature"]);
    expect(session.getSnapshot()).toEqual(before);
    expect(disposalCount).toBe(1);
    expect(result.metadata.candidateFingerprint).toBe(
      session.inspectCandidate("variant")?.fingerprint ?? "missing",
    );
    expect(result.metadata.baselineKey).toBe(contentKey(before.project));
  });
  test("rejects stale preconditions before allocating graphics resources", async () => {
    const { session, character, options } = setup();
    session.apply({
      expectedRevision: 0,
      operations: [
        { kind: "document.set", target: character.id, path: ["name"], value: "Someone else's work" },
      ],
    });
    let allocated = false;
    await expect(
      previewCreatureCandidate(session, "variant", options, async () => {
        allocated = true;
        throw Error("Must never allocate");
      }),
    ).rejects.toThrow();
    expect(allocated).toBe(false);
    expect(() => candidatePreviewProjects(session, "variant")).toThrow();
  });
  test("failed render disposes resources and keeps the original source", async () => {
    const { session, options } = setup();
    const key = contentKey(session.getSnapshot().project);
    let disposed = false;
    await expect(
      previewCreatureCandidate(session, "variant", options, async () => ({
        render: async () => {
          throw Error("GPU readback failed");
        },
        dispose: () => {
          disposed = true;
        },
      })),
    ).rejects.toThrow("GPU readback failed");
    expect(disposed).toBe(true);
    expect(contentKey(session.getSnapshot().project)).toBe(key);
  });
  test("cancellation closes a stalled render and source edits invalidate completed evidence", async () => {
    const { session, options } = setup();
    const abort = new AbortController();
    let disposed = false;
    await expect(
      previewCreatureCandidate(
        session,
        "variant",
        options,
        async () => ({
          render: async () => {
            abort.abort(new Error("Stop review"));
            return await new Promise<never>(() => {});
          },
          dispose: () => {
            disposed = true;
          },
        }),
        abort.signal,
      ),
    ).rejects.toThrow("Stop review");
    expect(disposed).toBe(true);

    const other = setup();
    await expect(
      previewCreatureCandidate(other.session, "variant", other.options, async () => ({
        render: async () => {
          other.session.apply({
            expectedRevision: 0,
            operations: [
              { kind: "document.set", target: other.character.id, path: ["name"], value: "New source" },
            ],
          });
          return { blob: new Blob() };
        },
        dispose: () => {},
      })),
    ).rejects.toThrow("Accepted source changed");
  });
  test("a timed-out allocation is disposed if the graphics driver resolves later", async () => {
    const { session, options } = setup();
    let finish: ((resource: CandidatePreviewResource) => void) | undefined;
    let disposed = false;
    const pending = new Promise<CandidatePreviewResource>((resolve) => {
      finish = resolve;
    });
    await expect(
      previewCreatureCandidate(session, "variant", { ...options, timeoutMs: 100 }, () => pending),
    ).rejects.toThrow("time budget");
    finish?.({
      render: async () => {
        throw Error("Expired work must never render");
      },
      dispose: () => {
        disposed = true;
      },
    });
    await Promise.resolve();
    expect(disposed).toBe(true);
    expect(session.getSnapshot().revision).toBe(0);
  });
});
