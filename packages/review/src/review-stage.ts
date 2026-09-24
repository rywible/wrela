import type { PacketSettings } from "@wrela/authoring";
import { contentKey, type Document, type Project } from "@wrela/model";

/** Immutable review-only fixtures. No source stage fallback and no publication of review lighting. */
export function reviewStage(project: Project, target: string, view: PacketSettings["views"][number]) {
  if (!view.rig) {
    if (view.stage && project.documents.find((d) => d.id === view.stage)?.kind !== "stage")
      throw Error(`Review stage ${view.stage} is missing; implicit fallback is forbidden`);
    const stage = view.stage ? project.documents.find((d) => d.id === view.stage) : undefined;
    return { project, stage: view.stage, lightingKey: stage ? contentKey(stage) : "source-default" };
  }
  const prefix = `review-${view.rig}-${contentKey(target)}`,
    env = `${prefix}-sky`,
    light = `${prefix}-light`,
    stage = `${prefix}-stage`;
  if (project.documents.some((d) => [env, light, stage].includes(d.id)))
    throw Error("Review fixture identity collides with authored source");
  const envelope = { schemaVersion: 1 as const, dependencies: [] };
  const documents: Document[] = [
    {
      ...envelope,
      id: env,
      name: "Fixed review sky",
      kind: "environment",
      model: "analytic-sky",
      sunElevation: view.rig === "neutral" ? 0.7 : 0.45,
      sunAzimuth: view.rig === "neutral" ? -0.65 : -1.42,
      turbidity: 2,
      fogDensity: 0,
      cloudCover: 0,
      skyColor: [0.38, 0.48, 0.6],
      horizonColor: [0.68, 0.71, 0.73],
      groundColor: [0.2, 0.2, 0.2],
      wind: [0, 0, 0],
    },
    {
      ...envelope,
      id: light,
      name: "Fixed review light",
      kind: "lighting",
      ambient: view.rig === "neutral" ? 0.7 : 0.4,
      lights: [
        { id: "review-sun", type: "directional", position: [0, 8, 4], color: [1, 1, 1], intensity: 2.5 },
      ],
    },
    {
      ...envelope,
      id: stage,
      name: `Fixed ${view.rig} review`,
      kind: "stage",
      environment: env,
      lighting: light,
      ground: true,
      exposure: 1,
      camera: view.camera,
      subjects: [target],
    },
  ];
  return {
    project: { ...project, documents: [...project.documents, ...documents] },
    stage,
    lightingKey: contentKey(documents.slice(0, 2)),
  };
}
