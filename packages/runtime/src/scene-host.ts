import { RadianceLightingCache } from "./radiance-lighting";
import {
  compileDocument,
  compilerKey,
  compileWaterEffects,
  compileWaterPhases,
  compileWaterReflectionProxies,
  oceanWaterMesh,
  resolveWaterWaves,
  riverWaterMesh,
  sharedMaterialLattice,
  waterMeshSpacing,
} from "@wrela/compiler";
import {
  type Camera,
  type CompiledCharacter,
  contentKey,
  createSurfaceAppearance,
  type Diagnostic,
  type Document,
  type EvaluatedScene,
  type GrowthEvent,
  identityMatrix,
  type MaterialDefinition,
  type MeshData,
  type PersistentVegetationGrowth,
  type Project,
  type Quality,
  type RenderMaterial,
  type RenderSurface,
  type StageDefinition,
  type SurfaceArtifact,
  type TerrainDefinition,
  transformMatrix,
  type Vec3,
  vegetationWindResponse,
  type WaterDefinition,
  type WaterPhaseProduct,
  type WorldDefinition,
  waterGeometryErrorBound,
  worldCompositionReferences,
} from "@wrela/model";

import {
  CELL_SIZE,
  realizeWorldComposition,
  type WorldOptions,
  type WorldSave,
  WorldSession,
  worldPosition,
} from "@wrela/world";
import { poseMatrices, quatFromEuler } from "./animation";
import { artifactBytes } from "./artifact-memory";
import { CreatureDetailSelector, DEFAULT_CREATURE_VIEWPORT_HEIGHT } from "./creature-detail";
import { applyCreatureInspection, creatureInspectionSource } from "./creature-inspection";
import { evaluateEnvironment, evaluateEnvironmentState, evaluateWaterEnvironment } from "./environment";
import { createGeologyPreview } from "./geology-preview";
import { resolveGroomMaterial } from "./groom-material";
import { IndirectLightingCache } from "./indirect-lighting";
import {
  type RuntimeSaveMigration,
  type RuntimeSaveMigrationReport,
  remapRuntimeEntities,
} from "./save-migration";
import { RuntimeSession } from "./session";
import { SkyVisibilityCache } from "./sky-visibility";
import { applySurfaceAppearance } from "./surface-appearance";
import { SurfaceReliefCache } from "./surface-relief";
import {
  prepareVegetationGrowth,
  validateVegetationGrowth,
  vegetationGrowthDocument,
} from "./vegetation-growth";

/** Resolve one authored environment for both rendering and physical water sampling. */
function environmentSources(
  documents: ReadonlyMap<string, Document>,
  subject?: Document,
  world?: WorldDefinition,
  stage?: StageDefinition,
) {
  const environment = documents.get(
    subject?.kind === "environment" ? subject.id : (world?.environment ?? stage?.environment ?? ""),
  );
  const lighting = documents.get(
    subject?.kind === "lighting" ? subject.id : (world?.lighting ?? stage?.lighting ?? ""),
  );
  return {
    environment: environment?.kind === "environment" ? environment : undefined,
    lighting: lighting?.kind === "lighting" ? lighting : undefined,
  };
}
function configureRuntimeWater(
  runtime: RuntimeSession,
  documents: ReadonlyMap<string, Document>,
  subject?: Document,
  world?: WorldDefinition,
  stage?: StageDefinition,
) {
  const ids =
    subject?.kind === "water"
      ? [subject.id]
      : [...new Set([world?.water, ...(world?.waters ?? [])].filter((id): id is string => !!id))];
  const waters = ids
    .map((id) => documents.get(id))
    .filter((doc): doc is WaterDefinition => doc?.kind === "water");
  const sources = environmentSources(documents, subject, world, stage);
  runtime.setWaters(
    waters.map((water) => ({
      definition: water,
      evaluate: water.flow?.weatherResponse
        ? (time) =>
            evaluateWaterEnvironment(
              water,
              evaluateEnvironmentState(sources.environment, sources.lighting, time),
            )
        : undefined,
    })),
  );
}
export type SceneHostOptions = {
  /** Diagnostic static transport solver for bounded lookdev/reference fixtures.
   * Player and Studio use the common renderer lighting; world transport has not
   * met its coverage/performance gates. This is not a product lighting toggle. */
  indirectLighting?: import("./indirect-lighting").IndirectLightingOptions;
  /** Compile a reusable noise lattice for renderer experiments; disabled until it wins on the target workload. */
  materialCache?: boolean;
  compile?: (document: Document, quality: Quality) => Promise<SurfaceArtifact | null>;
  generateTerrain?: WorldOptions["generate"];
  growVegetation?: import("@wrela/compiler").GrowthCompileProvider;
  maxCacheBytes?: number;
  maxInstalledBytes?: number;
};
/** Keep at least four cells across authored terrain feature radii near an interest.
 * Smaller leaves reuse the same bounded patch count and 256 m root coverage. */
function terrainRealization(terrain: TerrainDefinition | undefined, quality: Quality) {
  const resolution = quality === "export" ? 64 : quality === "review" ? 32 : 16;
  const radius = Math.min(
    Infinity,
    ...(terrain?.geology?.landforms.map((item) =>
      item.kind === "cliff" ? Math.max(0.25, item.width * 0.08) : item.width,
    ) ?? []),
    ...(terrain?.geology?.strata.strength ? [terrain.geology.strata.thickness] : []),
    ...(terrain?.geology?.corridors?.flatMap((corridor) =>
      [corridor.halfWidth, corridor.shoulder].filter((width) => width > 0),
    ) ?? []),
    ...(terrain?.interventions
      .filter((item) => item.kind !== "clearing" && item.strength > 0)
      .map((item) => item.radius) ?? []),
  );
  const baseSize = Math.max(8, Math.min(32, 2 ** Math.floor(Math.log2((radius * resolution) / 4))));
  return { resolution, baseSize, levels: Math.log2(256 / baseSize) };
}
const defaultMaterial: RenderMaterial = {
  color: [0.55, 0.63, 0.62],
  secondary: [0.8, 0.84, 0.84],
  roughness: 0.85,
  metallic: 0,
  pattern: 0,
  scale: 1,
  normalStrength: 0,
};
const waterBedAppearance = {
  ...createSurfaceAppearance(),
  detail: { kind: "mineral" as const, scale: 0.35, strength: 0.55 },
};
export function renderMaterial(material?: MaterialDefinition): RenderMaterial {
  return material
    ? applySurfaceAppearance({
        emission: material.emission,
        color: material.color,
        secondary: material.secondary,
        roughness: material.roughness,
        metallic: material.metallic,
        pattern: ["solid", "noise", "stripes", "marble", "weave"].indexOf(material.pattern),
        scale: material.scale,
        normalStrength: material.normalStrength,
        creature: material.creature,
        domain: material.domain,
        layers: material.layers,
        appearance: material.appearance,
      })
    : { ...defaultMaterial };
}
export function gridMesh(size: number, resolution = 1, height = 0): MeshData {
  const positions = new Float32Array((resolution + 1) ** 2 * 3),
    normals = new Float32Array(positions.length),
    indices: number[] = [];
  for (let z = 0; z <= resolution; z++)
    for (let x = 0; x <= resolution; x++) {
      const i = (z * (resolution + 1) + x) * 3;
      positions.set([(x / resolution - 0.5) * size, height, (z / resolution - 0.5) * size], i);
      normals[i + 1] = 1;
      if (x < resolution && z < resolution) {
        const a = z * (resolution + 1) + x,
          b = a + 1,
          c = a + resolution + 1,
          d = c + 1;
        indices.push(a, c, b, b, c, d);
      }
    }
  return {
    positions,
    normals,
    indices: new Uint32Array(indices),
    bounds: { min: [-size / 2, height, -size / 2], max: [size / 2, height, size / 2] },
  };
}
function instanceMatrix(position: Vec3, rotation: Vec3 = [0, 0, 0], scale = 1) {
  const matrix = poseMatrices(
    [
      {
        id: "transform",
        name: "",
        parent: null,
        position: [0, 0, 0],
        rotation: [0, 0, 0],
        radius: 1,
        minimum: -Math.PI,
        maximum: Math.PI,
      },
    ],
    new Map([["transform", { translation: position, rotation: quatFromEuler(rotation) }]]),
  );
  for (let i = 0; i < 12; i++) matrix[i] *= scale;
  return matrix;
}

export { artifactBytes } from "./artifact-memory";
export class BrowserSceneHost {
  world?: WorldSession;
  runtime?: RuntimeSession;
  private documents = new Map<string, Document>();
  private artifacts = new Map<string, SurfaceArtifact>();
  private surfaceRelief = new SurfaceReliefCache();
  private indirectLighting = new IndirectLightingCache();
  private skyVisibility = new SkyVisibilityCache();
  private radianceLighting = new RadianceLightingCache();
  get radianceLightingError() { return this.radianceLighting.error; }
  get radianceLightingReport() { return this.radianceLighting.report; }
  get radianceLightingBuilds() { return this.radianceLighting.builds; }
  async waitForRadianceLighting() { await this.radianceLighting.waitReady(); }
  get skyVisibilityError() {
    return this.skyVisibility.error;
  }
  get skyVisibilityBuilds() {
    return this.skyVisibility.builds;
  }
  get skyVisibilityReport() {
    return this.skyVisibility.report;
  }
  async waitForSkyVisibility() {
    await this.skyVisibility.waitReady();
  }
  private indirectLightingOptions?: SceneHostOptions["indirectLighting"];
  get indirectLightingReport() {
    return this.indirectLighting.report;
  }
  get indirectLightingRevision() {
    const field = this.indirectLighting.field;
    return field ? `${field.key}:${field.revision}` : "none";
  }
  get indirectLightingProgress() {
    const field = this.indirectLighting.field;
    return field
      ? { completed: field.completedProbes, total: field.totalProbes, status: field.report.status }
      : undefined;
  }
  configureIndirectLighting(options?: SceneHostOptions["indirectLighting"]): void {
    this.indirectLightingOptions = options;
    this.indirectLighting.invalidate();
  }
  async waitForIndirectLighting(): Promise<void> {
    await this.indirectLighting.waitReady();
  }
  get surfaceReliefReviews() {
    return this.surfaceRelief.reviews;
  }
  private displayedCreatureArtifacts = new WeakMap<RenderSurface, Readonly<CompiledCharacter>>();
  private cache = new Map<string, SurfaceArtifact>();
  private subjectId = "";
  private stageId?: string;
  private stage?: StageDefinition;
  private referencePositions = new Map<string, Vec3>();
  private generation = 0;
  private disposed = false;
  private lastTime = 0;
  private worldCamera: Vec3 = [0, 0, 0];
  private viewportHeight = DEFAULT_CREATURE_VIEWPORT_HEIGHT;
  private creatureDetailSelector = new CreatureDetailSelector();
  private cameraInterest: Vec3 = [Number.POSITIVE_INFINITY, 0, 0];
  private populationRevision = -1;
  private populationCenter = "";
  private populationSurfaces: RenderSurface[] = [];
  private ground = gridMesh(2000);
  private waterPhaseProducts = new WeakMap<WaterDefinition, WaterPhaseProduct>();
  private waterMeshes = new Map<string, MeshData>();
  private selectedMaterial?: string;
  private renderOrigin: Vec3 = [0, 0, 0];
  private seekEpoch = 0;
  private requestedSubjectId = "";
  private requestedStageId?: string;
  private requestedQuality: Quality = "interactive";
  private installedSignature = "";
  private growthArtifacts = new Set<string>();
  private installedTerrainKey = "";
  constructor(
    private project: Project,
    readonly options: SceneHostOptions = {},
  ) {
    for (const budget of [options.maxCacheBytes, options.maxInstalledBytes])
      if (budget !== undefined && (!Number.isFinite(budget) || budget < 0 || budget > 256 * 1024 * 1024))
        throw new RangeError("Invalid scene memory budget");
    this.documents = new Map(project.documents.map((d) => [d.id, d]));
    this.indirectLightingOptions = options.indirectLighting;
  }
  /** Internal inspector bridge: only surfaces from the current extraction resolve.
   * The callback sees the installed selected-LOD artifact; its result is detached
   * before crossing the public inspector boundary. Never expose the artifact itself. */
  inspectDisplayedCreature<T>(
    surface: RenderSurface,
    inspect: (artifact: Readonly<CompiledCharacter>) => T,
  ): T | undefined {
    const artifact = this.displayedCreatureArtifacts.get(surface);
    if (!artifact || artifact.id !== surface.source || artifact.mesh !== surface.mesh) return;
    return structuredClone(inspect(artifact));
  }
  /** Physical framebuffer pixels; unconfigured hosts explicitly use a 720 px estimate. */
  setViewportHeight(height: number): void {
    if (!Number.isFinite(height) || height < 1 || height > 32768)
      throw new RangeError("Viewport height must be in [1,32768] framebuffer pixels");
    this.viewportHeight = height;
  }
  get resourceUsage() {
    const assemblyBytes = this.runtime?.assemblyBytes ?? 0;
    const waterBytes = this.runtime?.waterBytes ?? 0;
    const relief = this.surfaceRelief.artifacts;
    return {
      cacheBytes: artifactBytes([...this.cache.values(), ...relief]),
      installedBytes:
        artifactBytes([...this.artifacts.values(), ...relief]) +
        assemblyBytes +
        waterBytes +
        this.indirectLighting.byteLength +
        this.skyVisibility.byteLength + this.radianceLighting.byteLength,
      // Shared coarse buffers count once. Worker products and JS engine overhead are excluded.
      liveBytes:
        artifactBytes([...this.cache.values(), ...this.artifacts.values(), ...relief]) +
        assemblyBytes +
        waterBytes +
        this.indirectLighting.byteLength +
        this.skyVisibility.byteLength + this.radianceLighting.byteLength,
      reliefBytes: this.surfaceRelief.byteLength,
      indirectLightingBytes: this.indirectLighting.byteLength,
      skyVisibilityBytes: this.skyVisibility.byteLength,
      radianceLightingBytes: this.radianceLighting.byteLength,
      assemblyBytes,
      waterBytes,
      cacheEntries: this.cache.size,
    };
  }
  get diagnostics(): Diagnostic[] {
    return [...this.artifacts.values()]
      .flatMap((artifact) => artifact.diagnostics)
      .concat(
        this.world?.metrics.failures.map((message) => ({
          severity: "error" as const,
          code: "world.generation",
          message,
          document: this.world?.world.terrain,
        })) ?? [],
      )
      .concat(this.surfaceRelief.diagnostics);
  }
  async setProject(project: Project): Promise<void> {
    this.project = project;
    this.documents = new Map(project.documents.map((d) => [d.id, d]));
    if (this.requestedSubjectId || this.subjectId)
      await this.prepare(
        this.requestedSubjectId || this.subjectId,
        this.requestedStageId ?? this.stageId,
        this.requestedQuality,
      );
  }
  private material(id: string, source?: string) {
    const artifact = source ? this.artifacts.get(source) : undefined;
    const generated =
      !this.selectedMaterial && artifact?.kind === "character"
        ? artifact.creatureMaterials?.find((material) => material.id === id)
        : undefined;
    if (generated) {
      const sourceId =
        artifact?.kind === "character" ? artifact.creatureGroomMaterialSources?.[id] : undefined;
      const opticalSource = sourceId ? this.documents.get(sourceId) : undefined;
      return resolveGroomMaterial(
        renderMaterial(generated),
        opticalSource?.kind === "material" ? renderMaterial(opticalSource) : undefined,
      );
    }
    const document = this.documents.get(this.selectedMaterial ?? id);
    return renderMaterial(document?.kind === "material" ? document : undefined);
  }
  private async artifact(id: string, quality: Quality) {
    const generation = this.generation;
    const doc = this.documents.get(id);
    if (!doc) throw new Error(`Missing scene definition ${id}`);
    const key = compilerKey(doc, quality);
    let artifact = this.cache.get(key);
    if (!artifact) {
      artifact =
        (this.options.compile
          ? await this.options.compile(doc, quality)
          : await new Promise<SurfaceArtifact | null>((resolve, reject) =>
              setTimeout(() => {
                try {
                  resolve(compileDocument(doc, quality));
                } catch (e) {
                  reject(e);
                }
              }, 0),
            )) ?? undefined;
      if (artifact) {
        this.cache.set(key, artifact);
        while (
          this.cache.size > 32 ||
          artifactBytes(this.cache.values()) > (this.options.maxCacheBytes ?? 32 * 1024 * 1024)
        ) {
          const oldest = this.cache.keys().next().value;
          if (!oldest) break;
          this.cache.delete(oldest);
        }
      }
    }
    if (artifact && generation === this.generation) {
      this.artifacts.set(id, artifact);
      if (artifactBytes(this.artifacts.values()) > (this.options.maxInstalledBytes ?? 64 * 1024 * 1024))
        throw new Error("Prepared scene exceeds the installed CPU artifact budget");
    }
    return artifact;
  }
  private referenceIds(stage?: StageDefinition) {
    return [
      ...new Set(
        stage?.subjects.length
          ? stage.subjects
          : this.project.documents
              .filter((document) => document.kind === "object" || document.kind === "character")
              .slice(0, 2)
              .map((document) => document.id),
      ),
    ];
  }
  private sceneSignature(subjectId: string, stageId: string | undefined, quality: Quality) {
    const subject = this.documents.get(subjectId);
    if (!subject) throw new Error(`Unknown subject ${subjectId}`);
    const stageDoc =
      subject.kind === "stage"
        ? subject
        : (this.documents.get(stageId ?? "") ?? this.project.documents.find((d) => d.kind === "stage"));
    const stage = stageDoc?.kind === "stage" ? stageDoc : undefined;
    const world =
      subject.kind === "world"
        ? subject
        : subject.kind === "terrain"
          ? this.project.documents.find(
              (d): d is WorldDefinition => d.kind === "world" && d.terrain === subject.id,
            )
          : undefined;
    const terrain = this.documents.get(world?.terrain ?? (subject.kind === "terrain" ? subject.id : ""));
    const ids = world
      ? [
          ...world.instances.map((i) => i.definition),
          ...world.populations.map((p) => p.definition),
          ...worldCompositionReferences(world.composition),
        ]
      : ["object", "character", "vegetation"].includes(subject.kind)
        ? [subject.id]
        : subject.kind === "water"
          ? []
          : this.referenceIds(stage);
    const sources = [...new Set(ids)].map((id) => {
      const doc = this.documents.get(id);
      return doc
        ? {
            id,
            key: compilerKey(doc, quality),
            material: "material" in doc ? doc.material : undefined,
            physics: doc.kind === "character" ? doc.physics.colliders : undefined,
            collision: doc.kind === "object" ? { mode: doc.collision, colliders: doc.colliders } : undefined,
          }
        : { id };
    });
    return {
      signature: contentKey({
        subjectId,
        stage: stage?.id,
        ground: stage?.ground,
        quality,
        sources,
        world: world?.composition
          ? { ...world, composition: { ...world.composition, review: undefined } }
          : world,
        geologyPreview:
          subject.kind === "terrain"
            ? { material: subject.material, formations: subject.geology?.formations ?? [] }
            : undefined,
        terrainRealization: terrainRealization(terrain?.kind === "terrain" ? terrain : undefined, quality),
      }),
      terrainKey: terrain?.kind === "terrain" ? contentKey(terrain) : "",
      terrain,
      stage,
      world,
    };
  }
  async prepare(
    subjectId: string,
    stageId?: string,
    quality: Quality = "interactive",
    resetRuntime = false,
  ): Promise<void> {
    if (this.disposed) throw new Error("Scene host disposed");
    this.displayedCreatureArtifacts = new WeakMap();
    const generation = ++this.generation;
    this.requestedSubjectId = subjectId;
    this.requestedStageId = stageId;
    this.requestedQuality = quality;
    const source = this.sceneSignature(subjectId, stageId, quality);
    if (!resetRuntime && source.signature === this.installedSignature && this.runtime) {
      this.stage = source.stage;
      this.stageId = stageId;
      this.populationRevision = -1;
      for (const doc of this.documents.values())
        if (doc.kind === "character") this.runtime.updateCharacterParameters(doc);
      configureRuntimeWater(
        this.runtime,
        this.documents,
        this.documents.get(subjectId),
        this.world?.world,
        this.stage,
      );
      if (
        this.world &&
        source.terrain?.kind === "terrain" &&
        source.terrainKey !== this.installedTerrainKey
      ) {
        const sourceWorld = source.world;
        const terrain =
          sourceWorld?.kind === "world"
            ? realizeWorldComposition(sourceWorld, source.terrain).terrain
            : source.terrain;
        this.world.replaceTerrain(terrain);
        const ready = await this.world.prepare();
        if (!ready.ready && generation === this.generation)
          throw new Error(`Terrain preparation incomplete: ${ready.missing.join(", ")}`);
        if (generation !== this.generation) return;
      }
      this.installedTerrainKey = source.terrainKey;
      return;
    }
    const candidate = new BrowserSceneHost(this.project, this.options);
    candidate.cache = new Map(this.cache);
    candidate.worldCamera = [...this.worldCamera];
    try {
      await candidate.prepareInPlace(subjectId, stageId, quality);
    } catch (error) {
      candidate.dispose();
      throw error;
    }
    if (this.disposed || generation !== this.generation) {
      candidate.dispose();
      return;
    }
    if (!resetRuntime && candidate.world && this.world?.world.id === candidate.world.world.id) {
      const save = this.world.save();
      if (save.overrides.length || save.dormant.length) {
        try {
          const vegetation = await candidate.prepareSavedVegetation(save);
          candidate.world.loadSave(save);
          candidate.installGrowthProducts(vegetation);
          if (this.disposed || generation !== this.generation) {
            candidate.dispose();
            return;
          }
        } catch (error) {
          candidate.dispose();
          throw error;
        }
      }
    }
    if (!resetRuntime && subjectId === this.subjectId && this.runtime && candidate.runtime)
      candidate.runtime.retainCompatibleAssemblyJoints(this.runtime.snapshotEntities());
    const oldRuntime = this.runtime,
      oldWorld = this.world;
    this.runtime = candidate.runtime;
    this.world = candidate.world;
    this.creatureDetailSelector.clear();
    this.artifacts = candidate.artifacts;
    this.documents = candidate.documents;
    this.growthArtifacts = candidate.growthArtifacts;
    this.cache = candidate.cache;
    this.subjectId = subjectId;
    this.stageId = stageId;
    this.stage = candidate.stage;
    this.referencePositions = candidate.referencePositions;
    this.selectedMaterial = candidate.selectedMaterial;
    this.populationSurfaces = [];
    this.populationRevision = -1;
    this.populationCenter = "";
    this.lastTime = 0;
    this.renderOrigin = [0, 0, 0];
    this.cameraInterest = [Number.POSITIVE_INFINITY, 0, 0];
    this.installedSignature = source.signature;
    this.installedTerrainKey = source.terrainKey;
    oldRuntime?.dispose();
    oldWorld?.dispose();
  }
  private instanceIds(definitionId: string): string[] {
    const ids = this.world
      ? this.world.world.instances
          .filter((i) => i.definition === definitionId || i.id === definitionId)
          .map((i) => i.id)
      : [definitionId];
    if (!ids.length || !this.runtime) throw new Error(`No live instance for ${definitionId}`);
    return ids;
  }
  setLocomotionSpeed(definitionId: string, speed: number) {
    for (const id of this.instanceIds(definitionId)) this.runtime?.setLocomotionSpeed(id, speed);
  }
  playMotion(definitionId: string, motion: string, blendSeconds?: number) {
    for (const id of this.instanceIds(definitionId)) this.runtime?.playMotion(id, motion, blendSeconds);
  }
  setPose(definitionId: string, jointId: string, rotation: Vec3, translation: Vec3 = [0, 0, 0]) {
    for (const id of this.instanceIds(definitionId))
      this.runtime?.setPose(id, jointId, rotation, translation);
  }
  clearPose(definitionId?: string) {
    if (definitionId) for (const id of this.instanceIds(definitionId)) this.runtime?.clearPose(id);
    else this.runtime?.clearPose();
  }
  getPose(definitionId: string, jointId: string) {
    return this.runtime?.getPose(this.instanceIds(definitionId)[0], jointId);
  }
  characterPosition(definitionId: string): Vec3 {
    const id = this.instanceIds(definitionId)[0];
    const instance = this.runtime?.evaluatedCharacters().find((i) => i.id === id);
    if (!instance || !this.runtime) throw new Error(`Unknown character ${definitionId}`);
    const position = this.runtime.bodyState(id).position;
    return [
      position[0],
      position[1] -
        ((instance.artifact.mesh.bounds.min[1] + instance.artifact.mesh.bounds.max[1]) / 2) * instance.scale,
      position[2],
    ];
  }
  async moveCharacter(definitionId: string, position: Vec3) {
    const ids = this.instanceIds(definitionId),
      runtime = this.runtime;
    if (!runtime) throw new Error("Preview is not prepared");
    let height = position[1];
    if (this.world) {
      this.world.setInterest({ id: "player", position, visualRadius: 48, collisionRadius: 40, priority: 2 });
      const ready = await this.world.prepare({ scope: "collision" });
      if (!ready.ready) throw new Error(`Destination collision is not ready: ${ready.missing.join(", ")}`);
      const ground = this.world.queryGround(position[0], position[2]);
      if (ground.status !== "ready") throw new Error(ground.reason);
      height = ground.height;
    }
    runtime.synchronizeLifecycle();
    for (const id of ids) {
      const instance = runtime.evaluatedCharacters().find((i) => i.id === id);
      if (!instance) throw new Error(`Unknown character ${id}`);
      const target: Vec3 = [
        position[0],
        height +
          ((instance.artifact.mesh.bounds.min[1] + instance.artifact.mesh.bounds.max[1]) / 2) *
            instance.scale,
        position[2],
      ];
      const previous = runtime.bodyState(id).position;
      if (Math.hypot(previous[0] - target[0], previous[2] - target[2]) > 32)
        await runtime.teleport(id, target);
      else runtime.setTarget(id, target);
    }
    return { position: [position[0], height, position[2]] as Vec3 };
  }
  private async prepareInPlace(
    subjectId: string,
    stageId?: string,
    quality: Quality = "interactive",
  ): Promise<void> {
    if (this.disposed) throw new Error("Scene host disposed");
    const generation = ++this.generation;
    this.subjectId = subjectId;
    this.stageId = stageId;
    const subject = this.documents.get(subjectId);
    if (!subject) throw new Error(`Unknown subject ${subjectId}`);
    const stage =
      subject.kind === "stage"
        ? subject
        : (this.documents.get(stageId ?? "") ?? this.project.documents.find((d) => d.kind === "stage"));
    this.stage = stage?.kind === "stage" ? stage : undefined;
    this.selectedMaterial = subject.kind === "material" ? subject.id : undefined;
    const previousRuntime = this.runtime;
    const runtime = await RuntimeSession.create();
    if (generation !== this.generation) {
      runtime.dispose();
      return;
    }
    this.runtime = runtime;
    previousRuntime?.dispose();
    this.artifacts.clear();
    this.populationSurfaces = [];
    this.populationRevision = -1;
    this.lastTime = 0;
    let world: WorldDefinition | undefined;
    if (subject.kind === "world") world = subject;
    else if (subject.kind === "terrain") {
      const baseWorld = this.project.documents.find(
        (d): d is WorldDefinition => d.kind === "world" && d.terrain === subject.id,
      ) ?? {
        id: `${subject.id}-preview`,
        name: "Terrain preview",
        kind: "world",
        schemaVersion: 1,
        dependencies: [],
        generatorVersion: "wrela-world-1",
        terrain: subject.id,
        environment: this.stage?.environment ?? "",
        lighting: this.stage?.lighting ?? "",
        populations: [],
        instances: [],
      };
      const preview = createGeologyPreview(this.project, subject, baseWorld);
      world = preview.world;
      this.documents = preview.documents;
    }
    if (world) {
      const sourceTerrain = this.documents.get(world.terrain);
      if (sourceTerrain?.kind !== "terrain") throw new Error("World terrain reference is unavailable");
      const realization = realizeWorldComposition(world, sourceTerrain);
      world = realization.world;
      const terrain = realization.terrain;
      if (this.world?.world.id !== world.id || contentKey(this.world.world) !== contentKey(world)) {
        const save = this.world?.world.id === world.id ? this.world.save() : undefined;
        this.world?.dispose();
        this.world = new WorldSession(world, terrain, {
          generate: this.options.generateTerrain,
          ...terrainRealization(terrain, quality),
        });
        if (save && (save.overrides.length || save.dormant.length)) this.world.loadSave(save);
      } else this.world.replaceTerrain(terrain);
      this.world.setInterest({
        id: "camera",
        position: this.worldCamera,
        visualRadius: 240,
        collisionRadius: 0,
      });
      this.world.setInterest({
        id: "player",
        position: world.instances[0]?.position ?? [0, 0, 0],
        visualRadius: 48,
        collisionRadius: 40,
        priority: 2,
      });
      const readiness = await this.world.prepare();
      if (!readiness.ready) throw new Error(`World preparation incomplete: ${readiness.missing.join(", ")}`);
      if (generation !== this.generation) return;
      const physicalOrigin = worldPosition(world.instances[0]?.position ?? [0, 0, 0]).cell.map(
        (v) => v * CELL_SIZE,
      ) as Vec3;
      runtime.physics.rebase(physicalOrigin);
      runtime.attachWorld(this.world);
      const ids = new Set([
        ...world.instances.map((i) => i.definition),
        ...world.populations.map((p) => p.definition),
      ]);
      for (const id of ids) {
        await this.artifact(id, quality);
        if (generation !== this.generation) return;
      }
      for (const instance of world.instances) {
        const doc = this.documents.get(instance.definition),
          artifact = this.artifacts.get(instance.definition);
        if (doc?.kind === "object" && artifact?.kind === "surface" && doc.assembly)
          runtime.addAssembly(
            instance.id,
            doc.assembly,
            artifact.mesh,
            instanceMatrix(instance.position, instance.rotation, instance.scale),
            doc.collision !== "none",
            doc.id,
          );
        else if (doc?.kind === "object" && artifact?.kind === "surface" && doc.collision !== "none")
          runtime.addStaticObject(
            instance.id,
            doc,
            artifact.mesh,
            instance.position,
            quatFromEuler(instance.rotation),
            instance.scale,
          );
        if (doc?.kind === "character" && artifact?.kind === "character")
          runtime.addCharacter(
            instance.id,
            artifact,
            doc,
            instance.position,
            instance.rotation,
            instance.scale,
          );
      }
    } else {
      this.world?.dispose();
      this.world = undefined;
      if (this.stage?.ground !== false) runtime.physics.addGround();
      // A single-character studio contains only its own collision proxies and
      // this authored plane. Secondary ground queries can be specialized exactly;
      // mixed stages and worlds retain the general support query.
      if (this.stage?.ground !== false && subject.kind === "character")
        runtime.setCreatureSecondaryGroundPlane({
          height: 0,
          minX: -1000,
          maxX: 1000,
          minZ: -1000,
          maxZ: 1000,
        });
      if (subject.kind === "character" || subject.kind === "object" || subject.kind === "vegetation") {
        const artifact = await this.artifact(subject.id, quality);
        if (generation !== this.generation) return;
        if (subject.kind === "character" && artifact?.kind === "character")
          runtime.addCharacter(subject.id, artifact, subject);
        if (subject.kind === "object" && artifact?.kind === "surface" && subject.assembly)
          runtime.addAssembly(
            subject.id,
            subject.assembly,
            artifact.mesh,
            identityMatrix(),
            subject.collision !== "none",
            subject.id,
          );
        else if (subject.kind === "object" && artifact?.kind === "surface" && subject.collision !== "none")
          runtime.addStaticObject(subject.id, subject, artifact.mesh, [0, 0, 0], [0, 0, 0, 1]);
      } else if (subject.kind !== "water") {
        const references = this.referenceIds(this.stage);
        const prepared: { id: string; artifact: SurfaceArtifact; bounds: MeshData["bounds"] }[] = [];
        for (const id of references) {
          const artifact = await this.artifact(id, quality);
          if (generation !== this.generation) return;
          if (artifact)
            prepared.push({
              id,
              artifact,
              bounds: artifact.kind === "vegetation" ? artifact.bounds : artifact.mesh.bounds,
            });
        }
        const gap = 1.5,
          totalWidth =
            prepared.reduce((total, item) => total + item.bounds.max[0] - item.bounds.min[0], 0) +
            Math.max(0, prepared.length - 1) * gap;
        let cursor = -totalWidth / 2;
        for (const { id, artifact, bounds } of prepared) {
          const position: Vec3 = [cursor - bounds.min[0], 0, 0];
          cursor += bounds.max[0] - bounds.min[0] + gap;
          this.referencePositions.set(id, position);
          const doc = this.documents.get(id);
          if (doc?.kind === "character" && artifact.kind === "character")
            runtime.addCharacter(id, artifact, doc, position);
          if (doc?.kind === "object" && artifact.kind === "surface" && doc.assembly)
            runtime.addAssembly(
              id,
              doc.assembly,
              artifact.mesh,
              transformMatrix(position),
              doc.collision !== "none",
              doc.id,
            );
          else if (doc?.kind === "object" && artifact.kind === "surface" && doc.collision !== "none")
            runtime.addStaticObject(id, doc, artifact.mesh, position, [0, 0, 0, 1]);
        }
      }
    }
    configureRuntimeWater(runtime, this.documents, subject, world, this.stage);
    if (this.resourceUsage.installedBytes > (this.options.maxInstalledBytes ?? 64 * 1024 * 1024))
      throw new Error("Prepared scene exceeds the installed CPU artifact budget");
    runtime.resetReplay();
  }
  private appendArtifact(
    surfaces: RenderSurface[],
    id: string,
    artifact: SurfaceArtifact,
    matrix: Float32Array,
    source: string,
  ) {
    const assemblyParts = this.runtime?.assemblyParts(id, matrix);
    if (assemblyParts && artifact.kind === "surface") {
      for (const part of assemblyParts)
        surfaces.push({
          id: `${id}/assembly/${part.id}`,
          instanceId: id,
          source,
          mesh: part.mesh,
          lightingMobility: part.dynamic ? "dynamic" : undefined,
          matrix: part.matrix,
          material: this.material(part.material ?? artifact.material),
        });
      return;
    }
    if (artifact.kind === "vegetation")
      for (const part of artifact.surfaces) {
        if (part.mesh.indices.length === 0) continue;
        surfaces.push({
          id: `${id}/${part.id}`,
          instanceId: id,
          source,
          mesh: part.mesh,
          details: part.details,
          renderProducts: part.renderProducts,
          matrix,
          material: this.material(part.material),
          wind:
            this.documents.get(source)?.kind === "vegetation"
              ? vegetationWindResponse(
                  this.documents.get(source) as Extract<Document, { kind: "vegetation" }>,
                )
              : artifact.windResponse,
        });
      }
    else if (artifact.kind === "surface")
      surfaces.push({
        id,
        instanceId: id,
        source,
        mesh: artifact.mesh,
        renderProducts: artifact.renderProducts,
        matrix,
        material: this.material(artifact.material),
      });
  }
  private materialSurfaces(surfaces: RenderSurface[]): RenderSurface[] {
    this.surfaceRelief.beginFrame();
    return surfaces.flatMap((original) => {
      const definition = this.documents.get(original.source);
      let surface =
        definition?.kind === "vegetation"
          ? { ...original, wind: vegetationWindResponse(definition) }
          : original;
      const groups = [...new Set(surface.mesh.materialGroups?.map((group) => group.material) ?? [])];
      const artifact = this.artifacts.get(surface.source);
      const part =
        definition?.kind === "object"
          ? definition.assembly?.parts.find((entry) => entry.id === surface.mesh.sourceIds?.[0])
          : undefined;
      const artifactMaterial =
        artifact?.kind === "vegetation"
          ? artifact.surfaces.find((entry) => entry.mesh === surface.mesh)?.material
          : artifact?.material;
      const materialId =
        this.selectedMaterial ??
        groups[0] ??
        part?.material ??
        artifactMaterial ??
        (definition && "material" in definition ? definition.material : "unbound");
      if (groups.length > 1) {
        for (const id of groups) {
          const relief = this.material(id, surface.source).appearance?.relief;
          if (relief?.amplitude)
            this.surfaceRelief.skip(
              surface,
              this.selectedMaterial ?? id,
              relief,
              "mixed-material-unsupported",
            );
        }
      } else {
        if (groups.length) surface = { ...surface, material: this.material(groups[0], surface.source) };
        surface = this.surfaceRelief.apply(surface, materialId);
      }
      if (!surface.mesh.materialGroups?.length) return [surface];
      return surface.mesh.materialGroups.map((group) => ({
        ...surface,
        id: `${surface.id}/material/${group.material}`,
        drawRange: { start: group.start, count: group.count },
        details: surface.details?.flatMap((detail) => {
          const range = detail.mesh.materialGroups?.find(
            (candidate) => candidate.material === group.material,
          );
          return range ? [{ ...detail, drawRange: { start: range.start, count: range.count } }] : [];
        }),
        material: this.material(group.material, surface.source),
        creatureInspection: surface.creatureInspection
          ? { ...surface.creatureInspection, materialId: group.material }
          : undefined,
      }));
    });
  }
  /** Gameplay advances simulation explicitly; presentation never changes physics. */
  advance(seconds: number, camera?: Camera): number {
    if (!Number.isFinite(seconds) || seconds < 0) throw new RangeError("Invalid frame duration");
    if (camera) this.updateView(camera);
    const steps = this.runtime?.advance(seconds) ?? 0;
    this.lastTime = this.runtime?.clock.time ?? this.lastTime;
    return steps;
  }
  updateView(camera: Camera): void {
    this.worldCamera = [...camera.position];
    if (
      this.world &&
      Math.hypot(camera.position[0] - this.cameraInterest[0], camera.position[2] - this.cameraInterest[2]) >
        12
    ) {
      this.cameraInterest = [...camera.position];
      this.world.setInterest({
        id: "camera",
        position: camera.position,
        visualRadius: 240,
        collisionRadius: 0,
      });
      this.world.update();
    }
  }
  /** Backward-compatible editor playhead. Games use advance + extract. */
  evaluate(time: number, camera: Camera, mode: EvaluatedScene["mode"] = "beauty"): EvaluatedScene {
    if (!Number.isFinite(time) || time < 0) throw new RangeError("Invalid preview time");
    if (this.runtime && !this.runtime.isSeeking) {
      if (time < this.lastTime || time - this.lastTime > 0.25) this.runtime.seek(Math.round(time * 60));
      else this.runtime.advance(time - this.lastTime);
      this.lastTime = time;
    }
    this.updateView(camera);
    return this.extract(camera, mode);
  }
  /** Extract a render snapshot without advancing simulation or changing residency. */
  extract(camera: Camera, mode: EvaluatedScene["mode"] = "beauty", applyIndirect = true): EvaluatedScene {
    this.displayedCreatureArtifacts = new WeakMap();
    const displayedCharacters = new Map<string, CompiledCharacter>();
    const subject = this.documents.get(this.subjectId);
    const world = this.world;
    const surfaces: RenderSurface[] = [];
    const origin: Vec3 = world
      ? (worldPosition(camera.position).cell.map((v, i) => (i === 1 ? 0 : v * CELL_SIZE)) as Vec3)
      : [0, 0, 0];
    const relative = (position: Vec3): Vec3 => position.map((v, i) => v - origin[i]) as Vec3;
    if (origin.some((v, i) => v !== this.renderOrigin[i])) {
      this.renderOrigin = origin;
      this.populationRevision = -1;
    }

    const effectiveTime = this.runtime?.clock.time ?? this.lastTime;
    if (world) {
      const terrain = this.documents.get(world.world.terrain);
      for (const patch of world.residentPatches()) {
        const material = this.material(terrain?.kind === "terrain" ? terrain.material : "");
        material.domain ??= "world";
        surfaces.push({
          id: `terrain/${patch.key}`,
          source: world.world.terrain,
          mesh: patch.mesh,
          matrix: transformMatrix(relative([patch.x, 0, patch.z])),
          material:
            mode === "lod"
              ? {
                  ...material,
                  color: [
                    [0.25, 0.65, 0.8],
                    [0.5, 0.8, 0.3],
                    [0.95, 0.6, 0.2],
                    [0.8, 0.25, 0.4],
                  ][patch.level % 4] as Vec3,
                  pattern: 0,
                }
              : material,
        });
      }
      for (const instance of world.world.instances) {
        if (world.persistence.overrides.get(instance.id)?.removed) continue;
        const definition = world.persistence.overrides.get(instance.id)?.definition ?? instance.definition;
        const artifact = this.artifacts.get(definition);
        if (artifact)
          this.appendArtifact(
            surfaces,
            instance.id,
            artifact,
            instanceMatrix(
              relative(world.persistence.overrides.get(instance.id)?.position ?? instance.position),
              world.persistence.overrides.get(instance.id)?.rotation ?? instance.rotation,
              world.persistence.overrides.get(instance.id)?.scale ?? instance.scale,
            ),
            definition,
          );
      }
      const populationCenter = `${Math.floor(camera.position[0] / 24)}:${Math.floor(camera.position[2] / 24)}:${contentKey([...world.persistence.overrides])}`;
      if (this.populationRevision !== world.metrics.revision || populationCenter !== this.populationCenter) {
        this.populationCenter = populationCenter;
        this.populationRevision = world.metrics.revision;
        this.populationSurfaces = [];
        for (const placement of world.placements(
          {
            minX: camera.position[0] - 115,
            minZ: camera.position[2] - 115,
            maxX: camera.position[0] + 115,
            maxZ: camera.position[2] + 115,
          },
          200,
        )) {
          const artifact = this.artifacts.get(placement.definition);
          if (artifact)
            this.appendArtifact(
              this.populationSurfaces,
              placement.id,
              artifact,
              instanceMatrix(relative(placement.position), [0, placement.rotation, 0], placement.scale),
              placement.definition,
            );
        }
      }
      surfaces.push(...this.populationSurfaces);
    } else {
      if (this.stage?.ground !== false && subject?.kind !== "water")
        surfaces.push({
          id: "stage-ground",
          source: this.stage?.id ?? "stage",
          mesh: this.ground,
          matrix: identityMatrix(),
          material: {
            ...defaultMaterial,
            color: [0.2, 0.24, 0.25],
            secondary: [0.25, 0.29, 0.3],
            pattern: 1,
            scale: 0.25,
          },
        });
      for (const [id, artifact] of this.artifacts)
        this.appendArtifact(
          surfaces,
          id,
          artifact,
          transformMatrix(this.referencePositions.get(id) ?? [0, 0, 0]),
          id,
        );
    }
    if (this.runtime)
      for (const instance of this.runtime.evaluatedCharacters(
        effectiveTime,
        origin,
        (artifact, position, scale, instanceId, skinMatrices) =>
          this.creatureDetailSelector.select(
            instanceId,
            artifact,
            position,
            scale,
            camera,
            this.viewportHeight,
            skinMatrices,
          ).detail,
      )) {
        displayedCharacters.set(instance.id, instance.artifact);
        surfaces.push({
          id: instance.id,
          instanceId: instance.id,
          source: instance.artifact.id,
          mesh: instance.artifact.mesh,
          matrix: instance.matrix,
          material: this.material(instance.artifact.material),
          skin: {
            jointIndices: instance.artifact.jointIndices,
            weights: instance.artifact.weights,
            matrices: instance.skinMatrices,
          },
          deformation: instance.deformation,
          creatureInspection: creatureInspectionSource(instance.artifact),
          creatureDetail: instance.artifact.creature
            ? this.creatureDetailSelector.inspect(instance.id)?.metadata
            : undefined,
        });
      }
    const sources = environmentSources(this.documents, subject, world?.world, this.stage);
    const environment = evaluateEnvironment(
      sources.environment,
      sources.lighting,
      this.stage?.exposure ?? 1,
      0,
      { time: effectiveTime, position: camera.position, grade: this.stage?.grade },
    );
    const waterIds =
      subject?.kind === "water"
        ? [subject.id]
        : [
            ...new Set(
              [world?.world.water, ...(world?.world.waters ?? [])].filter((id): id is string => !!id),
            ),
          ];
    for (const waterId of waterIds) {
      const waterSource = waterId ? this.documents.get(waterId) : undefined;
      const water =
        waterSource?.kind === "water" ? evaluateWaterEnvironment(waterSource, environment) : undefined;
      if (water?.kind === "water") {
        const waterState = this.runtime?.waterBodies.get(water.id)?.renderState(water);
        const key = contentKey({
          level: water.level,
          world: !!world,
          river: water.flow?.river,
          domain: waterState?.domain?.key,
          spectrum: !!water.spectrum,
        });
        let mesh = this.waterMeshes.get(key);
        if (!mesh) {
          mesh =
            waterState?.domain?.surface ??
            (water.spectrum ? oceanWaterMesh() : undefined) ??
            riverWaterMesh(water, world ? 640 / 256 : 30 / 64) ??
            gridMesh(world ? 640 : 30, world ? 256 : 64, water.level);
          if (this.waterMeshes.size >= 16) this.waterMeshes.clear();
          this.waterMeshes.set(key, mesh);
        }
        const waterSpacing = waterMeshSpacing(mesh, world ? 640 / 256 : 30 / 64);
        let waterPhases = this.waterPhaseProducts.get(water);
        if (!waterPhases) {
          waterPhases = compileWaterPhases(water);
          this.waterPhaseProducts.set(water, waterPhases);
        }
        surfaces.push({
          id: water.id,
          source: water.id,
          mesh,
          waterState,
          matrix: transformMatrix(
            relative(
              (world || water.spectrum) && !water.flow?.river && !water.domain
                ? water.spectrum
                  ? [camera.position[0], 0, camera.position[2]]
                  : [Math.floor(camera.position[0] / 32) * 32, 0, Math.floor(camera.position[2] / 32) * 32]
                : waterState?.domain
                  ? [waterState.domain.min[0], 0, waterState.domain.min[1]]
                  : [0, 0, 0],
            ),
          ),
          material: {
            ...defaultMaterial,
            color: water.color,
            secondary: water.color,
            roughness: water.roughness,
            metallic: 0.25,
          },
          waterPhases,
          waterApproximation: {
            spacing: waterSpacing,
            maxHeightError: waterGeometryErrorBound(resolveWaterWaves(water), waterSpacing),
          },
          water: {
            ...water,
            flow: water.flow,
            waves: resolveWaterWaves(water).map((wave) => ({
              ...wave,
              phase:
                (wave.phase +
                  ((Math.cos(wave.direction) * origin[0] + Math.sin(wave.direction) * origin[2]) *
                    2 *
                    Math.PI) /
                    wave.wavelength) %
                (2 * Math.PI),
            })),
          },
        });
        if (waterState && water.effects?.length) {
          const parent = surfaces[surfaces.length - 1];
          for (const [index, effectMesh] of compileWaterEffects(water).entries())
            surfaces.push({
              ...parent,
              id: `${water.id}-effect-${water.effects[index].id}`,
              mesh: effectMesh,
              matrix: transformMatrix(relative([0, 0, 0])),
              waterEffect: index,
            });
        }
        if (waterState?.domain)
          surfaces.push({
            id: `${water.id}-bed`,
            source: water.id,
            mesh: waterState.domain.bed,
            waterContact: { water, state: waterState },
            matrix: transformMatrix(relative([waterState.domain.min[0], 0, waterState.domain.min[1]])),
            material: {
              ...defaultMaterial,
              color: [0.12, 0.135, 0.12],
              secondary: [0.37, 0.33, 0.245],
              roughness: 0.86,
              metallic: 0,
              appearance: waterBedAppearance,
              pattern: 0,
              scale: 1.2,
              normalStrength: 0,
            },
          });
      }
    }
    if (surfaces.some((s) => s.waterState)) {
      const reflections = compileWaterReflectionProxies(surfaces);
      for (const surface of surfaces)
        if (surface.waterState || surface.waterContact) surface.waterReflections = reflections;
    }
    environment.windPhase = (origin[0] * 0.17 + origin[2] * 0.23) % (2 * Math.PI);
    environment.pointLights = environment.pointLights?.map((light) => ({
      ...light,
      position: relative(light.position),
    }));
    const scene = applyCreatureInspection({
      materialLattice: this.options.materialCache ? sharedMaterialLattice() : undefined,
      surfaces: this.materialSurfaces(surfaces).map((surface) => {
        if (!environment.wetness || surface.water) return surface;
        const appearance = surface.material.appearance ?? createSurfaceAppearance();
        return {
          ...surface,
          material: {
            ...surface.material,
            appearance: { ...appearance, wetness: Math.max(appearance.wetness, environment.wetness) },
          },
        };
      }),
      camera: { ...camera, position: relative(camera.position), target: relative(camera.target) },
      environment,
      time: effectiveTime,
      origin,
      ...(world ? { shadowRadius: 240 } : {}),
      mode,
      grid: !world && subject?.kind !== "water",
    });
    for (const surface of scene.surfaces) {
      const artifact = surface.instanceId ? displayedCharacters.get(surface.instanceId) : undefined;
      if (
        artifact &&
        surface.source === artifact.id &&
        surface.mesh.positions === artifact.mesh.positions &&
        surface.mesh.indices === artifact.mesh.indices
      )
        this.displayedCreatureArtifacts.set(surface, {
          ...artifact,
          mesh: surface.mesh,
          creatureRegions: surface.creatureInspection?.regions ?? artifact.creatureRegions,
          creatureCoordinates: surface.creatureInspection?.coordinates ?? artifact.creatureCoordinates,
        });
    }
    if (applyIndirect) this.applyIndirectLighting(scene);
    return scene;
  }
  /** Call after review lighting overrides, so the cache observes the displayed lighting. */
  applyIndirectLighting(scene: EvaluatedScene): void {
    this.radianceLighting.strip(scene);
    if (this.indirectLightingOptions)
      scene.indirectLighting = this.indirectLighting.update(scene, this.indirectLightingOptions);
    else delete scene.indirectLighting;
    this.skyVisibility.apply(scene);
    this.radianceLighting.apply(scene);
  }
  /** Growth compilation is transactional and never runs inside extract/render. */
  async growVegetation(
    instanceId: string,
    definitionId: string,
    steps: number,
    events: GrowthEvent[] = [],
  ): Promise<PersistentVegetationGrowth> {
    const world = this.world;
    const source = this.documents.get(definitionId);
    if (!world || source?.kind !== "vegetation") throw Error("A prepared world tree is required");
    const instance = world.world.instances.find((instance) => instance.id === instanceId);
    const visible = this.populationSurfaces.some((surface) => surface.instanceId === instanceId);
    const previous = world.persistence.overrides.get(instanceId);
    if (!instance && !visible && !previous?.growth) throw Error("Unknown resident tree identity");
    if (instance && (previous?.growth?.definition ?? instance.definition) !== definitionId)
      throw Error("Tree source mismatch");
    const prepared = this.options.growVegetation
      ? await this.options.growVegetation(
          source,
          instanceId,
          steps,
          previous?.growth,
          events,
          this.requestedQuality,
        )
      : undefined;
    const growth = prepared?.growth ?? prepareVegetationGrowth(source, steps, previous?.growth, events);
    const document = prepared?.document ?? vegetationGrowthDocument(source, growth, instanceId);
    if (this.project.documents.some((source) => source.id === document.id))
      throw Error("Growth realization conflicts with an authored document identity");
    const artifact =
      prepared?.artifact ??
      (this.options.compile
        ? await this.options.compile(document, this.requestedQuality)
        : compileDocument(document, this.requestedQuality));
    if (!artifact || artifact.kind !== "vegetation") throw Error("Tree compilation failed");
    if (
      world !== this.world ||
      this.documents.get(definitionId) !== source ||
      world.persistence.overrides.get(instanceId) !== previous
    )
      throw new DOMException("Growth edit superseded", "AbortError");
    const proposed = new Map(world.persistence.overrides);
    proposed.set(instanceId, { ...previous, growth, definition: document.id });
    world.persistence.validate({ ...world.save(), overrides: [...proposed] });
    const installed = new Map(this.artifacts);
    installed.set(document.id, artifact);
    if (artifactBytes(installed.values()) > (this.options.maxInstalledBytes ?? 64 * 1024 * 1024))
      throw Error("Growth product exceeds installed memory budget");
    this.documents.set(document.id, document);
    this.artifacts.set(document.id, artifact);
    this.growthArtifacts.add(document.id);
    world.persistence.overrides.set(instanceId, { ...previous, growth, definition: document.id });
    this.populationRevision = -1;
    return structuredClone(growth);
  }
  private async prepareSavedVegetation(save: WorldSave) {
    const prepared: { document: Document; artifact: SurfaceArtifact }[] = [];
    for (const [id, override] of save.overrides) {
      if (!override.growth) continue;
      const source = this.documents.get(override.growth.definition);
      if (source?.kind !== "vegetation") throw Error("Saved tree source is unavailable");
      validateVegetationGrowth(source, override.growth);
      const document = vegetationGrowthDocument(source, override.growth, id);
      if (this.project.documents.some((source) => source.id === document.id))
        throw Error("Saved growth realization conflicts with an authored document identity");
      if (override.definition !== document.id) throw Error("Saved tree realization identity mismatch");
      const artifact = this.options.compile
        ? await this.options.compile(document, this.requestedQuality)
        : compileDocument(document, this.requestedQuality);
      if (!artifact || artifact.kind !== "vegetation") throw Error("Saved tree compilation failed");
      prepared.push({ document, artifact });
    }
    const installed = new Map([...this.artifacts].filter(([id]) => !this.growthArtifacts.has(id)));
    for (const entry of prepared) installed.set(entry.document.id, entry.artifact);
    if (artifactBytes(installed.values()) > (this.options.maxInstalledBytes ?? 64 * 1024 * 1024))
      throw Error("Saved growth products exceed installed memory budget");
    return prepared;
  }
  private installGrowthProducts(records: { document: Document; artifact: SurfaceArtifact }[]) {
    const desired = new Set(records.map((record) => record.document.id));
    for (const id of this.growthArtifacts)
      if (!desired.has(id)) {
        this.documents.delete(id);
        this.artifacts.delete(id);
      }
    for (const { document, artifact } of records) {
      this.documents.set(document.id, document);
      this.artifacts.set(document.id, artifact);
    }
    this.growthArtifacts = desired;
  }
  saveRuntime(): WorldSave {
    if (!this.world || !this.runtime)
      throw new Error("A world preview must be prepared before saving runtime state");
    const save = this.world.save(),
      entities = new Map(save.dormant.map((entity) => [entity.id, entity]));
    for (const entity of this.runtime.snapshotEntities()) entities.set(entity.id, entity);
    return { ...save, dormant: [...entities.values()], waters: this.runtime.snapshotWaters() };
  }
  async resetRuntime(): Promise<void> {
    if (!this.subjectId) throw new Error("Preview is not prepared");
    await this.prepare(this.subjectId, this.stageId, this.requestedQuality, true);
  }
  /** Validate/remap a copy for review without changing the running world. */
  prepareRuntimeSave(
    input: WorldSave,
    migration?: RuntimeSaveMigration,
  ): { save: WorldSave; migration?: RuntimeSaveMigrationReport } {
    if (!this.world || !this.runtime)
      throw new Error("A world preview must be prepared before restoring runtime state");
    const save = this.world.persistence.validate(input);
    let report: RuntimeSaveMigrationReport | undefined;
    if (migration) {
      const remapped = remapRuntimeEntities(save.dormant, migration);
      save.dormant = remapped.entities;
      report = remapped.report;
    }
    // A rule cannot bypass live identity, quaternion, mode, motion or scale validation.
    this.runtime.validateEntityStates(save.dormant);
    this.runtime.validateWaterStates(save.waters, save.dormant);
    return { save, migration: report };
  }
  async loadRuntime(
    input: WorldSave,
    options: { migration?: RuntimeSaveMigration } = {},
  ): Promise<{ time: number; migration?: RuntimeSaveMigrationReport }> {
    const world = this.world,
      runtime = this.runtime;
    if (!world || !runtime)
      throw new Error("A world preview must be prepared before restoring runtime state");
    const preparedSave = this.prepareRuntimeSave(input, options.migration),
      save = preparedSave.save;
    const characters = runtime.validateEntityStates(save.dormant);
    const vegetation = await this.prepareSavedVegetation(save);
    runtime.cancelSeek();
    const paused = runtime.paused,
      previousInterests = world.interestSources();
    runtime.paused = true;
    try {
      // A load is a physical transition. Keep simulation paused until every
      // restored body has prepared collision around its saved world position.
      const centres: Vec3[] = [];
      for (const character of characters)
        if (
          character.active &&
          !centres.some(
            (position) =>
              Math.hypot(position[0] - character.body.position[0], position[2] - character.body.position[2]) <
              24,
          )
        )
          centres.push(character.body.position);
      if (centres.length > 6)
        throw new Error("Runtime save requires more than six independent physical regions");
      if (centres.length)
        world.setInterests([
          ...previousInterests.filter(
            (interest) =>
              interest.id !== "player" &&
              !interest.id.startsWith("restore-") &&
              !interest.id.startsWith("runtime:"),
          ),
          ...centres.map((position, index) => ({
            id: index === 0 ? "player" : `restore-${index}`,
            position,
            visualRadius: 48,
            collisionRadius: 40,
            priority: 2,
          })),
        ]);
      const prepared = await world.prepare({ scope: "collision" });
      if (!prepared.ready)
        throw new Error(`Saved destination collision is not ready: ${prepared.missing.join(", ")}`);
      if (world !== this.world || runtime !== this.runtime)
        throw new DOMException("Runtime restore superseded", "AbortError");
      world.loadSave(save);
      this.installGrowthProducts(vegetation);
      this.populationRevision = -1;
      const time = runtime.restoreEntityStates(save.dormant, save.waters);
      this.lastTime = time;
      return { time, migration: preparedSave.migration };
    } catch (error) {
      if (world === this.world) {
        world.setInterests(previousInterests);
        await world.prepare({ scope: "collision" });
      }
      throw error;
    } finally {
      if (runtime === this.runtime) runtime.paused = paused;
    }
  }
  async seek(time: number, onProgress?: (fraction: number) => void): Promise<void> {
    if (!Number.isFinite(time) || time < 0) throw new RangeError("Invalid preview time");
    const runtime = this.runtime;
    if (!runtime) throw new Error("Preview is not prepared");
    const epoch = ++this.seekEpoch;
    await runtime.seekAsync(Math.round(time * 60), { onProgress });
    if (epoch !== this.seekEpoch || runtime !== this.runtime)
      throw new DOMException("Preview seek superseded", "AbortError");
    this.lastTime = time;
  }
  async teleport(position: Vec3) {
    this.worldCamera = [...position];
    this.cameraInterest = [...position];
    if (!this.world) return { ready: true, missing: [] };
    const result = await this.world.teleport(position, {
      id: "camera",
      visualRadius: 240,
      collisionRadius: 0,
    });
    return { ready: result.ready, missing: result.missing };
  }
  dispose() {
    if (this.disposed) return;
    this.displayedCreatureArtifacts = new WeakMap();
    this.creatureDetailSelector.clear();
    this.disposed = true;
    this.generation++;
    this.runtime?.dispose();
    this.world?.dispose();
    this.artifacts.clear();
    this.growthArtifacts.clear();
    this.cache.clear();
    this.waterMeshes.clear();
    this.surfaceRelief.clear();
    this.indirectLighting.dispose();
    this.skyVisibility.dispose();
    this.radianceLighting.dispose();
  }
}
