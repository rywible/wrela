import { compileDocument, compilerKey } from "@wrela/compiler";
import {
  type Camera,
  contentKey,
  type Diagnostic,
  type Document,
  type EvaluatedScene,
  identityMatrix,
  type MaterialDefinition,
  type MeshData,
  type Project,
  type Quality,
  type RenderMaterial,
  type RenderSurface,
  type StageDefinition,
  type SurfaceArtifact,
  transformMatrix,
  type Vec3,
  type WorldDefinition,
  waterGeometryErrorBound,
} from "@wrela/model";
import { CELL_SIZE, type WorldOptions, type WorldSave, WorldSession, worldPosition } from "@wrela/world";
import { poseMatrices, quatFromEuler } from "./animation";
import { installObjectCollision } from "./collision-realization";
import { evaluateEnvironment } from "./environment";
import {
  type RuntimeSaveMigration,
  type RuntimeSaveMigrationReport,
  remapRuntimeEntities,
} from "./save-migration";
import { RuntimeSession } from "./session";
export type SceneHostOptions = {
  compile?: (document: Document, quality: Quality) => Promise<SurfaceArtifact | null>;
  generateTerrain?: WorldOptions["generate"];
  maxCacheBytes?: number;
  maxInstalledBytes?: number;
};
const defaultMaterial: RenderMaterial = {
  color: [0.55, 0.63, 0.62],
  secondary: [0.8, 0.84, 0.84],
  roughness: 0.85,
  metallic: 0,
  pattern: 0,
  scale: 1,
  normalStrength: 0,
};
export function renderMaterial(material?: MaterialDefinition): RenderMaterial {
  return material
    ? {
        color: material.color,
        secondary: material.secondary,
        roughness: material.roughness,
        metallic: material.metallic,
        pattern: ["solid", "noise", "stripes", "marble"].indexOf(material.pattern),
        scale: material.scale,
        normalStrength: material.normalStrength,
        domain: material.domain,
        layers: material.layers,
      }
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
export function artifactBytes(artifacts: Iterable<SurfaceArtifact>): number {
  const buffers = new Set<ArrayBufferLike>(),
    meshes = new Set<MeshData>();
  const metadataArrays = new Set<object>();
  let metadata = 0;
  for (const artifact of new Set(artifacts)) {
    const surfaces = artifact.kind === "vegetation" ? artifact.surfaces : [artifact];
    for (const mesh of surfaces.flatMap((surface) => [
      surface.mesh,
      ...("details" in surface ? (surface.details?.map((detail) => detail.mesh) ?? []) : []),
    ])) {
      if (meshes.has(mesh)) continue;
      meshes.add(mesh);
      buffers.add(mesh.positions.buffer);
      buffers.add(mesh.normals.buffer);
      buffers.add(mesh.indices.buffer);
      if (mesh.colors) buffers.add(mesh.colors.buffer);
      if (mesh.sourceIds && !metadataArrays.has(mesh.sourceIds)) {
        metadataArrays.add(mesh.sourceIds);
        metadata +=
          mesh.sourceIds.length * 8 +
          [...new Set(mesh.sourceIds)].reduce((bytes, id) => bytes + id.length * 2, 0);
      }
      if (mesh.materialGroups && !metadataArrays.has(mesh.materialGroups)) {
        metadataArrays.add(mesh.materialGroups);
        metadata += mesh.materialGroups.reduce((bytes, group) => bytes + 32 + group.material.length * 2, 0);
      }
    }
    if (artifact.kind === "character") {
      buffers.add(artifact.jointIndices.buffer);
      buffers.add(artifact.weights.buffer);
      metadata += JSON.stringify({ joints: artifact.joints, motions: artifact.motions }).length * 2;
    }
  }
  return [...buffers].reduce((sum, buffer) => sum + buffer.byteLength, metadata);
}
export class BrowserSceneHost {
  world?: WorldSession;
  runtime?: RuntimeSession;
  private documents = new Map<string, Document>();
  private artifacts = new Map<string, SurfaceArtifact>();
  private cache = new Map<string, SurfaceArtifact>();
  private subjectId = "";
  private stageId?: string;
  private stage?: StageDefinition;
  private referencePositions = new Map<string, Vec3>();
  private generation = 0;
  private disposed = false;
  private lastTime = 0;
  private worldCamera: Vec3 = [0, 0, 0];
  private cameraInterest: Vec3 = [Number.POSITIVE_INFINITY, 0, 0];
  private populationRevision = -1;
  private populationCenter = "";
  private populationSurfaces: RenderSurface[] = [];
  private ground = gridMesh(2000);
  private waterMeshes = new Map<string, MeshData>();
  private selectedMaterial?: string;
  private renderOrigin: Vec3 = [0, 0, 0];
  private seekEpoch = 0;
  private requestedSubjectId = "";
  private requestedStageId?: string;
  private requestedQuality: Quality = "interactive";
  private installedSignature = "";
  private installedTerrainKey = "";
  constructor(
    private project: Project,
    readonly options: SceneHostOptions = {},
  ) {
    for (const budget of [options.maxCacheBytes, options.maxInstalledBytes])
      if (budget !== undefined && (!Number.isFinite(budget) || budget < 0 || budget > 256 * 1024 * 1024))
        throw new RangeError("Invalid scene memory budget");
    this.documents = new Map(project.documents.map((d) => [d.id, d]));
  }
  get resourceUsage() {
    return {
      cacheBytes: artifactBytes(this.cache.values()),
      installedBytes: artifactBytes(this.artifacts.values()),
      // Unique currently owned products; unpublished worker products and JS engine overhead are excluded.
      liveBytes: artifactBytes([...this.cache.values(), ...this.artifacts.values()]),
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
      );
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
  private material(id: string) {
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
      ? [...world.instances.map((i) => i.definition), ...world.populations.map((p) => p.definition)]
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
      signature: contentKey({ subjectId, stage: stage?.id, ground: stage?.ground, quality, sources, world }),
      terrainKey: terrain?.kind === "terrain" ? contentKey(terrain) : "",
      terrain,
      stage,
    };
  }
  async prepare(
    subjectId: string,
    stageId?: string,
    quality: Quality = "interactive",
    resetRuntime = false,
  ): Promise<void> {
    if (this.disposed) throw new Error("Scene host disposed");
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
      const waterId = this.documents.get(subjectId)?.kind === "water" ? subjectId : this.world?.world.water;
      const water = this.documents.get(waterId ?? "");
      this.runtime.setWater(water?.kind === "water" ? water : undefined);
      if (
        this.world &&
        source.terrain?.kind === "terrain" &&
        source.terrainKey !== this.installedTerrainKey
      ) {
        this.world.replaceTerrain(source.terrain);
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
          candidate.world.loadSave(save);
        } catch (error) {
          candidate.dispose();
          throw error;
        }
      }
    }
    const oldRuntime = this.runtime,
      oldWorld = this.world;
    this.runtime = candidate.runtime;
    this.world = candidate.world;
    this.artifacts = candidate.artifacts;
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
  playMotion(definitionId: string, motion: string, blendSeconds = 0.2) {
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
    else if (subject.kind === "terrain")
      world = this.project.documents.find(
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
    if (world) {
      const terrain = this.documents.get(world.terrain);
      if (terrain?.kind !== "terrain") throw new Error("World terrain reference is unavailable");
      if (this.world?.world.id !== world.id || contentKey(this.world.world) !== contentKey(world)) {
        const save = this.world?.world.id === world.id ? this.world.save() : undefined;
        this.world?.dispose();
        this.world = new WorldSession(world, terrain, {
          generate: this.options.generateTerrain,
          resolution: quality === "export" ? 64 : quality === "review" ? 32 : 16,
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
        if (doc?.kind === "object" && artifact?.kind === "surface" && doc.collision !== "none")
          installObjectCollision(
            runtime.physics,
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
      if (subject.kind === "character" || subject.kind === "object" || subject.kind === "vegetation") {
        const artifact = await this.artifact(subject.id, quality);
        if (generation !== this.generation) return;
        if (subject.kind === "character" && artifact?.kind === "character")
          runtime.addCharacter(subject.id, artifact, subject);
        if (subject.kind === "object" && artifact?.kind === "surface" && subject.collision !== "none")
          installObjectCollision(runtime.physics, subject.id, subject, artifact.mesh, [0, 0, 0]);
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
          if (doc?.kind === "object" && artifact.kind === "surface" && doc.collision !== "none")
            installObjectCollision(runtime.physics, id, doc, artifact.mesh, position);
        }
      }
    }
    const waterId = subject.kind === "water" ? subject.id : world?.water;
    const water = waterId ? this.documents.get(waterId) : undefined;
    runtime.setWater(water?.kind === "water" ? water : undefined);
    runtime.resetReplay();
  }
  private appendArtifact(
    surfaces: RenderSurface[],
    id: string,
    artifact: SurfaceArtifact,
    matrix: Float32Array,
    source: string,
  ) {
    if (artifact.kind === "vegetation")
      for (const part of artifact.surfaces)
        surfaces.push({
          id: `${id}/${part.id}`,
          instanceId: id,
          source,
          mesh: part.mesh,
          details: part.details,
          matrix,
          material: this.material(part.material),
          wind:
            this.documents.get(source)?.kind === "vegetation"
              ? (this.documents.get(source) as Extract<Document, { kind: "vegetation" }>).windResponse
              : artifact.windResponse,
        });
    else if (artifact.kind === "surface")
      surfaces.push({
        id,
        instanceId: id,
        source,
        mesh: artifact.mesh,
        matrix,
        material: this.material(artifact.material),
      });
  }
  private materialSurfaces(surfaces: RenderSurface[]): RenderSurface[] {
    return surfaces.flatMap((original) => {
      const definition = this.documents.get(original.source);
      const surface =
        definition?.kind === "vegetation" ? { ...original, wind: definition.windResponse } : original;
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
        material: this.material(group.material),
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
  extract(camera: Camera, mode: EvaluatedScene["mode"] = "beauty"): EvaluatedScene {
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
        const artifact = this.artifacts.get(instance.definition);
        if (artifact)
          this.appendArtifact(
            surfaces,
            instance.id,
            artifact,
            instanceMatrix(
              relative(world.persistence.overrides.get(instance.id)?.position ?? instance.position),
              world.persistence.overrides.get(instance.id)?.rotation ?? instance.rotation,
              instance.scale,
            ),
            instance.definition,
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
      for (const instance of this.runtime.evaluatedCharacters(effectiveTime, origin))
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
        });
    const waterId = subject?.kind === "water" ? subject.id : world?.world.water;
    const water = waterId ? this.documents.get(waterId) : undefined;
    if (water?.kind === "water") {
      const key = contentKey({ level: water.level, world: !!world });
      let mesh = this.waterMeshes.get(key);
      if (!mesh) {
        mesh = gridMesh(world ? 640 : 30, world ? 256 : 64, water.level);
        this.waterMeshes.clear();
        this.waterMeshes.set(key, mesh);
      }
      surfaces.push({
        id: water.id,
        source: water.id,
        mesh,
        matrix: transformMatrix(
          relative(
            world
              ? [Math.floor(camera.position[0] / 32) * 32, 0, Math.floor(camera.position[2] / 32) * 32]
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
        waterApproximation: {
          spacing: world ? 640 / 256 : 30 / 64,
          maxHeightError: waterGeometryErrorBound(water.waves, world ? 640 / 256 : 30 / 64),
        },
        water: {
          ...water,
          waves: water.waves.map((wave) => ({
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
    }
    const env = this.documents.get(
        subject?.kind === "environment"
          ? subject.id
          : (world?.world.environment ?? this.stage?.environment ?? ""),
      ),
      lighting = this.documents.get(
        subject?.kind === "lighting" ? subject.id : (world?.world.lighting ?? this.stage?.lighting ?? ""),
      );
    const environment = evaluateEnvironment(
      env?.kind === "environment" ? env : undefined,
      lighting?.kind === "lighting" ? lighting : undefined,
      this.stage?.exposure ?? 1,
    );
    environment.windPhase = (origin[0] * 0.17 + origin[2] * 0.23) % (2 * Math.PI);
    environment.pointLights = environment.pointLights?.map((light) => ({
      ...light,
      position: relative(light.position),
    }));
    return {
      surfaces: this.materialSurfaces(surfaces),
      camera: { ...camera, position: relative(camera.position), target: relative(camera.target) },
      environment,
      time: effectiveTime,
      origin,
      ...(world ? { shadowRadius: 240 } : {}),
      mode,
      grid: !world && subject?.kind !== "water",
    };
  }
  saveRuntime(): WorldSave {
    if (!this.world || !this.runtime)
      throw new Error("A world preview must be prepared before saving runtime state");
    const save = this.world.save(),
      entities = new Map(save.dormant.map((entity) => [entity.id, entity]));
    for (const entity of this.runtime.snapshotEntities()) entities.set(entity.id, entity);
    return { ...save, dormant: [...entities.values()] };
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
      const time = runtime.restoreEntityStates(save.dormant);
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
    this.disposed = true;
    this.generation++;
    this.runtime?.dispose();
    this.world?.dispose();
    this.artifacts.clear();
    this.cache.clear();
    this.waterMeshes.clear();
  }
}
