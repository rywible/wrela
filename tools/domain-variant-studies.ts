import {
  type AuthoringDomain,
  AuthoringSession,
  type DomainRecipe,
  type DomainVariantRequest,
} from "@wrela/authoring";
import { buildGeologyObjects } from "@wrela/compiler";
import { createCharacterLookdev } from "@wrela/examples/character-lookdev";
import { ALPINE_LOOKDEV_CAMERAS, createAlpineLookdevProject } from "@wrela/examples/lookdev-scene";
import { createGeologyAuthoringStudy } from "@wrela/examples/terrain-lookdev";
import {
  type Camera,
  type Document,
  type Project,
  parseProject,
  type StageDefinition,
  type WorldDefinition,
} from "@wrela/model";
import { createCreatureFittingLookdev } from "./fixtures/creature-fitting";
import type { LookdevStudy } from "./fixtures/lookdev";

export type DomainVariantStudy = {
  domain: AuthoringDomain;
  lookdev: LookdevStudy;
  request: DomainVariantRequest;
  variationScope: string;
};
export const DOMAIN_VARIANT_DOMAINS: AuthoringDomain[] = [
  "creatures",
  "assemblies",
  "vegetation",
  "geology",
  "materials",
  "world",
  "performance",
  "environment",
];
const camera = (position: Camera["position"], target: Camera["target"], fov: number): Camera => ({
  position,
  target,
  fov,
});
function replace(project: Project, documents: Document[]) {
  const ids = new Set(documents.map((document) => document.id));
  project.documents = [...project.documents.filter((document) => !ids.has(document.id)), ...documents];
}
function make(
  domain: AuthoringDomain,
  lookdev: LookdevStudy,
  recipes: DomainRecipe[],
  variationScope: string,
): DomainVariantStudy {
  lookdev.project = parseProject(lookdev.project);
  return {
    domain,
    lookdev,
    request: {
      id: `${domain}-review`,
      expectedRevision: 0,
      actor: "domain-variant-lookdev",
      intent: "Review three predeclared parameter variants against one unchanged source baseline",
      variants: recipes.map((recipe, index) => ({ id: `variant-${index + 1}`, recipe })),
    },
    variationScope,
  };
}
function stage(project: Project): StageDefinition {
  const value: StageDefinition = {
    id: "domain-variant-stage",
    name: "Matched source variant review",
    kind: "stage",
    schemaVersion: 1,
    dependencies: [],
    environment: "alpine-lookdev-sky",
    lighting: "alpine-lookdev-light",
    ground: true,
    exposure: 1.15,
    subjects: [],
    camera: camera([6, 4.2, -9], [0, 1.75, 0], 38),
  };
  // Resolve the production environment's actual light identity rather than inventing a binding.
  const world = project.documents.find((document) => document.id === project.entry);
  if (world?.kind === "world") {
    value.environment = world.environment;
    value.lighting = world.lighting;
  }
  replace(project, [value]);
  return value;
}

/** Values are declared before capture. They are workflow hold-outs, never preapproved artistic variants. */
export function createDomainVariantStudy(domain: AuthoringDomain): DomainVariantStudy {
  if (domain === "creatures") {
    const fitting = createCreatureFittingLookdev();
    const character = fitting.project.documents.find((document) => document.id === fitting.characterId);
    if (character?.kind !== "character" || !character.creature) throw Error("Missing fitted character");
    const shoulder = character.creature.regions.find((region) => region.id === "shoulder");
    if (!shoulder) throw Error("Missing common shoulder fitting scope");
    // The mantle fitting child shares spine controls with its shoulder parent.
    // Measure/edit the common anatomy explicitly, so the usual overlap guard stays active.
    const [first, second] = [-1, 1].map((sign, index) => ({
      id: `variant-shoulder-${index === 0 ? "left" : "right"}`,
      region: shoulder.id,
      position: [sign * shoulder.extent[0], 0, 0] as [number, number, number],
    }));
    const source = new AuthoringSession(fitting.project);
    source.apply({
      expectedRevision: 0,
      operations: [first, second].map((value) => ({
        kind: "creature.landmark" as const,
        target: character.id,
        value,
      })),
    });
    fitting.project = source.getSnapshot().project;
    const span = Math.hypot(...first.position.map((value, index) => value - second.position[index]));
    const view = camera([4, 2.7, -5], [0, 1.65, -0.05], 39);
    return make(
      domain,
      {
        project: fitting.project,
        subject: character.id,
        stage: fitting.project.entry,
        frames: [
          { id: "neutral", camera: view },
          { id: "silhouette", camera: view, mode: "silhouette" },
          {
            id: "motion",
            camera: camera([1.4, 2.3, -3], [0, 1.65, -0.25], 34),
            time: 0.6,
            motion: { definition: character.id, id: "walk" },
          },
        ],
      },
      [0.85, 1.12, 1.24].map((factor) => ({
        kind: "creature.landmarkSpan",
        target: character.id,
        first: first.id,
        second: second.id,
        distance: span * factor,
        descendants: true,
      })),
      "Uniform shoulder span with descendant coherence and existing fitted garment; three parameters, not new anatomical species.",
    );
  }
  if (domain === "performance") {
    const character = createCharacterLookdev();
    const document = character.documents.find((value) => value.id === character.character);
    if (document?.kind !== "character") throw Error("Missing motion character");
    const motion = document.motions.find((value) => value.id === "walk");
    if (!motion) throw Error("Missing review walk");
    const project = parseProject({
      schemaVersion: 1,
      id: "domain-performance-review",
      name: "Clip timing variants",
      documents: character.documents,
      entry: "ash-warden-stage",
    });
    const view = camera([5.9, 1.95, 0.65], [0, 1.22, 0.65], 41);
    return make(
      domain,
      {
        project,
        subject: document.id,
        stage: project.entry,
        frames: [0.15, 0.45, 0.75].map((time, index) => ({
          id: `motion-${index + 1}`,
          camera: view,
          time,
          motion: { definition: document.id, id: motion.id },
        })),
      },
      [0.8, 1.2, 1.45].map((factor) => ({
        kind: "performance.retime",
        target: document.id,
        motion: motion.id,
        duration: motion.duration * factor,
      })),
      "Matched wall-clock samples expose faster/slower clip timing; preserve pose/event/contact alignment, not equal phase or gait approval.",
    );
  }
  const project = createAlpineLookdevProject(),
    reviewStage = stage(project);
  if (domain === "assemblies" || domain === "materials") {
    const view = camera([6, 4.2, -9], [0, 1.75, 0], 38);
    return make(
      domain,
      {
        project,
        subject: "alpine-lookdev-gateway",
        stage: reviewStage.id,
        frames: [
          { id: "neutral", camera: view },
          { id: "detail", camera: camera([3, 2.8, -4], [0.7, 2, 0], 40) },
          { id: "motion", camera: view, time: 1.5 },
        ],
      },
      domain === "assemblies"
        ? [0.12, 0.55, 0.9].map((amount) => ({
            kind: "assembly.weather",
            target: "alpine-lookdev-gateway",
            amount,
          }))
        : [0.15, 0.55, 0.9].map((weathering) => ({
            kind: "material.history",
            target: "alpine-lookdev-masonry",
            weathering,
            dirt: weathering * 0.5,
            wetness: weathering * 0.3,
          })),
      domain === "assemblies"
        ? "Existing per-part optical wear only; dimensions, sockets, articulation and authored clearances remain identical."
        : "Shared masonry surface history on real gateway surfaces; geometry and relief remain identical.",
    );
  }
  if (domain === "vegetation") {
    const target = "alpine-lookdev-pine-young";
    const view = camera([8, 5, 14], [0, 3.7, 0], 42);
    return make(
      domain,
      {
        project,
        subject: target,
        stage: reviewStage.id,
        frames: [
          { id: "neutral", camera: view },
          { id: "gameplay", camera: camera([14, 3.2, 20], [0, 3.7, 0], 48) },
          { id: "wind-backlight", camera: view, time: 1.2, sunDirection: [-0.5, 0.22, -0.84] },
        ],
      },
      [0.3, 0.6, 0.82].map((density) => ({ kind: "vegetation.canopy", target, density })),
      "One existing conifer's canopy openness; unchanged seed, dimensions and woody controls. Needle coverage needs visual and temporal review.",
    );
  }
  const world = project.documents.find(
    (document): document is WorldDefinition => document.id === project.entry && document.kind === "world",
  );
  if (!world) throw Error("Missing shared review world");
  if (domain === "geology") {
    const terrain = createGeologyAuthoringStudy(),
      objects = buildGeologyObjects(terrain);
    const studyWorld: WorldDefinition = {
      ...world,
      id: "domain-geology-world",
      name: "Protected route erosion variants",
      terrain: terrain.id,
      water: undefined,
      composition: undefined,
      populations: [],
      instances: objects.map((object, index) => ({
        id: `placed-${object.id}`,
        definition: object.id,
        position: terrain.geology?.formations[index].position ?? [0, 0, 0],
        rotation: [0, 0, 0],
        scale: 1,
      })),
    };
    replace(project, [terrain, ...objects, studyWorld]);
    return make(
      domain,
      {
        project,
        subject: studyWorld.id,
        frames: [
          { id: "landform", camera: camera([15, 8, -22], [-1, 3.7, 1], 48) },
          { id: "corridor", camera: camera([-6.8, 2.2, -10], [-5, 2.1, 6], 58) },
          { id: "gameplay", camera: camera([-16, 13, 28], [-5, 1, 12], 52) },
        ],
      },
      [0.05, 0.55, 0.9].map((strength) => ({ kind: "geology.erosion", target: terrain.id, strength })),
      "Talus erosion strength on directed geology with unchanged authored corridor profiles. Runtime traversal still needs independent verification.",
    );
  }
  if (domain === "world") {
    world.populations.push({
      id: "variant-understory",
      definition: "alpine-lookdev-sedge",
      spacing: 3,
      density: 0.4,
      seed: 921,
      minHeight: -0.4,
      maxHeight: 20,
      maxSlope: 0.6,
    });
    // Confine this population to one explicit biome; existing source routes clear it normally.
    world.composition?.biomes.push({
      id: "variant-verge",
      center: [-8, 0, -8],
      radius: 16,
      transition: 3,
      populations: ["variant-understory"],
      density: 1,
    });
    return make(
      domain,
      { project, subject: world.id, frames: ALPINE_LOOKDEV_CAMERAS },
      [0.1, 0.65, 0.95].map((density) => ({
        kind: "world.population",
        target: world.id,
        rule: "variant-understory",
        density,
      })),
      "A bounded sedge community at three densities in the same authored biome; routes, landmarks, explicit instances and placement exceptions remain unchanged.",
    );
  }
  return make(
    domain,
    { project, subject: world.id, frames: ALPINE_LOOKDEV_CAMERAS },
    [-0.75, 0.35, 1].map((exposureCompensation) => ({
      kind: "environment.grade",
      target: world.environment,
      grade: { exposureCompensation, tint: [1, 1, 1] },
    })),
    "Exposure on the composed world with all weather/source timing preserved; this is a grade workflow test, not atmospheric transport research.",
  );
}
