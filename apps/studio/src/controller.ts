import {
  AuthoringSession,
  type CreatureRepairRequest,
  creatureGroundingStamp,
  type DiscoveryQuery,
  downloadBlob,
  type EditBatch,
  executeAgentRequest,
  explainAuthoringSource,
  groundCreatureHit,
  type InspectionQuery,
  ProjectStore,
} from "@wrela/authoring";
import { cookProjectAsync, queryWater } from "@wrela/compiler";
import { BrowserCompiler } from "@wrela/compiler/client";
import { type CreatureFixtureId, createCreatureFixture, referenceProject } from "@wrela/examples";
import { CreatureEncounterGame } from "@wrela/game-creature/rules";
import {
  type Camera,
  contentKey,
  type Diagnostic,
  type EvaluatedScene,
  type FrameMeasurements,
  identityColor,
  type Project,
  type Quality,
  type RenderCompleteness,
  type Vec3,
  VIEW_MODES,
} from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import {
  applyCreatureInspection,
  BrowserSceneHost,
  cameraRay,
  inspectCreatureMaterialAtHit,
  pickScene,
  reviewCreature,
  runtimeDiagnostics,
} from "@wrela/runtime";
import { z } from "zod";
import { AuthoringWorkbench } from "./authoring-workbench";
import { drawOverlays, type Overlay, overlayCapture } from "./capture";
import { CompilerClient } from "./compiler-client";
import { type CandidatePreviewOptions, previewCreatureCandidate } from "./creature-candidate-preview";
import { creaturePlayerMarker } from "./creature-encounter-marker";
import { createCreatureStudioTools } from "./creature-tools";
import { standalonePlayer } from "./player-export";
import { assertRenderAccepted } from "./preview-readiness";
import type { DraftSummary } from "./recovery-drafts";
export type PrepareOptions = {
  revision?: number;
  timeoutMs?: number;
  subject?: string;
  stage?: "world" | "studio";
  stageId?: string;
  quality?: Quality;
  region?: { center: [number, number]; radius: number };
};
export type CaptureOptions = PrepareOptions & {
  tick?: number;
  channels?: EvaluatedScene["mode"][];
  overlays?: Overlay[];
  camera?: Camera | NamedView;
};
export const NAMED_VIEWS = [
  "front",
  "back",
  "left",
  "right",
  "top",
  "three-quarter",
  "valley-overlook",
] as const;
export type NamedView = (typeof NAMED_VIEWS)[number];
export type PreviewState = {
  status: "starting" | "compiling" | "current" | "failed";
  revision: number;
  message: string;
  playing: boolean;
  seeking: boolean;
  time: number;
  mode: EvaluatedScene["mode"];
  stage: "world" | "studio";
  stageId?: string;
  quality: Quality;
  grid: boolean;
  hideGroom: boolean;
  overlays: Overlay[];
  exposure: number;
  reviewSunElevation?: number;
  reviewSunAzimuth?: number;
  wind: number;
  measurements: FrameMeasurements | null;
  camera: Camera;
  diagnostics: Diagnostic[];
  pickedInstance?: string;
  recoveryDrafts: DraftSummary[];
  returnProjectName?: string;
  encounter?: ReturnType<CreatureEncounterGame["snapshot"]>;
  encounterStarting?: boolean;
  pickedCreatureMaterial?: ReturnType<typeof inspectCreatureMaterialAtHit>;
};
export class StudioController {
  readonly authoring = new AuthoringSession(referenceProject());
  readonly work = new AuthoringWorkbench(
    () => this.authoring,
    async (sourceKey) => {
      if (contentKey(this.authoring.getSnapshot().project) !== sourceKey)
        throw Error("Source changed before adoption was saved; recover the pending publication");
      const saved = await this.save();
      if (saved.key !== sourceKey) throw Error("Saved source differs from adoption");
    },
  );
  readonly creature = {
    ...createCreatureStudioTools(this.authoring),
    review: (target: string, scenarioId: string) => {
      const document = this.authoring.getSnapshot().project.documents.find((item) => item.id === target);
      if (document?.kind !== "character" || !document.creature)
        throw Error("Select a character with creature source");
      const scenario = document.creature.reviewScenarios.find((item) => item.id === scenarioId);
      if (!scenario) throw Error("Unknown creature review scenario");
      return reviewCreature(document, scenario);
    },
    playScenario: async (target: string, scenarioId: string) => {
      this.stopCreatureEncounter();
      const source = this.authoring.getSnapshot();
      const document = source.project.documents.find((item) => item.id === target);
      if (document?.kind !== "character") throw Error("Select a character");
      const scenario = document.creature?.reviewScenarios.find((item) => item.id === scenarioId);
      if (!scenario) throw Error("Unknown creature review scenario");
      await this.prepare({
        subject: target,
        stage: "studio",
        quality: "interactive",
        revision: source.revision,
      });
      if (!this.host) throw Error("Creature preview is unavailable");
      await this.seek(0);
      if (scenario.motion) this.host.playMotion(target, scenario.motion, 0);
      this.patch({ camera: { ...scenario.cameras[0], fov: 48 }, playing: true });
    },
    capture: (options: CaptureOptions = {}) => this.capture({ ...options, stage: "studio" }),
    inspectView: (input: { channel?: EvaluatedScene["mode"]; skeleton?: boolean; hideGroom?: boolean }) => {
      const options = z
        .object({
          channel: z.enum(VIEW_MODES).optional(),
          skeleton: z.boolean().optional(),
          hideGroom: z.boolean().optional(),
        })
        .strict()
        .parse(input);
      this.patch({
        ...(options.channel ? { mode: options.channel } : {}),
        ...(options.hideGroom !== undefined ? { hideGroom: options.hideGroom } : {}),
        ...(options.skeleton !== undefined
          ? {
              overlays: options.skeleton
                ? [...new Set<Overlay>([...this.state.overlays, "rig"])]
                : this.state.overlays.filter((overlay) => overlay !== "rig"),
            }
          : {}),
      });
      return {
        channel: this.state.mode,
        overlays: this.state.overlays,
        hideGroom: this.state.hideGroom,
        visualApproval: "not-reviewed",
      };
    },
    inspectPixel: (input: { x: number; y: number; width?: number; height?: number }) => {
      const options = z
        .object({
          x: z.number().finite().nonnegative(),
          y: z.number().finite().nonnegative(),
          width: z.number().finite().positive().optional(),
          height: z.number().finite().positive().optional(),
        })
        .strict()
        .parse(input);
      if (!this.lastScene || this.state.revision !== this.authoring.getSnapshot().revision)
        throw Error("Wait for the current source to finish rendering before inspecting a pixel");
      const width = options.width ?? this.viewportCanvas?.clientWidth ?? 1,
        height = options.height ?? this.viewportCanvas?.clientHeight ?? 1;
      if (options.x > width || options.y > height) throw Error("Pixel lies outside the supplied viewport");
      const hit = pickScene(
        this.lastScene,
        cameraRay(this.lastScene.camera, options.x, options.y, width, height),
      );
      return hit ? inspectCreatureMaterialAtHit(this.lastScene, hit) : null;
    },
    groundPixel: (input: { x: number; y: number; width?: number; height?: number }) => {
      const inspected = this.creature.inspectPixel(input);
      const scene = this.lastScene,
        host = this.encounterSession?.host ?? this.host;
      if (!inspected || !scene || !host) return null;
      const width = input.width ?? this.viewportCanvas?.clientWidth ?? 1,
        height = input.height ?? this.viewportCanvas?.clientHeight ?? 1;
      const hit = pickScene(scene, cameraRay(scene.camera, input.x, input.y, width, height));
      const source = this.authoring.getSnapshot();
      const character = source.project.documents.find((document) => document.id === inspected.documentId);
      const surface = scene.surfaces.find((item) => item.id === hit?.surfaceId);
      if (!hit || !surface || character?.kind !== "character") return null;
      const grounded = host.inspectDisplayedCreature(surface, (artifact) =>
        groundCreatureHit(character, artifact, {
          ...creatureGroundingStamp(character, artifact, source.revision),
          triangle: hit.triangle,
          barycentric: hit.barycentric,
        }),
      );
      if (!grounded) throw Error("Displayed creature realization changed; render and pick again");
      return grounded;
    },
    previewCandidate: (id: string, options: CandidatePreviewOptions) =>
      this.previewCreatureCandidate(id, options),
    repairPixel: (input: {
      id: string;
      pixel: { x: number; y: number; width?: number; height?: number };
      restDisplacement: Vec3;
      support: CreatureRepairRequest["support"];
      protectPixels?: { x: number; y: number; width?: number; height?: number }[];
      tolerance?: number;
      detail?: CreatureRepairRequest["detail"];
      intent: string;
    }) => {
      if (this.encounterSession) throw Error("Return to authoring before proposing a surface repair");
      const hit = this.creature.groundPixel(input.pixel);
      if (!hit || hit.regions.length !== 1 || hit.geometrySources.length !== 1 || hit.groomRoots.length)
        throw Error("Pick a body surface with one anatomical owner");
      const snapshot = this.authoring.getSnapshot();
      const character = snapshot.project.documents.find(
        (d) => d.kind === "character" && contentKey(d) === hit.sourceKey,
      );
      if (!character || character.kind !== "character") throw Error("Rendered source changed; pick again");
      if (
        character.creature?.attachments.some((a) =>
          a.nodeIds.some((id) => hit.geometrySources.some((s) => s.id === id)),
        )
      )
        throw Error("Edit a mounted component through its attachment controls");
      if ((input.protectPixels?.length ?? 0) > 32)
        throw Error("At most 32 protected surface points are supported");
      const protectedPoints = (input.protectPixels ?? []).map((pixel, i) => {
        const point = this.creature.groundPixel(pixel);
        if (
          !point ||
          point.sourceKey !== hit.sourceKey ||
          point.regions.length !== 1 ||
          point.regions[0].id !== hit.regions[0].id ||
          point.geometrySources.length !== 1 ||
          point.geometrySources[0].id !== hit.geometrySources[0].id ||
          point.groomRoots.length
        )
          throw Error("Protected pixels must hit the same anatomical body region");
        return { id: `pin-${i}`, position: point.restPoint, tolerance: input.tolerance ?? 0.001 };
      });
      return this.authoring.repairCreature({
        id: input.id,
        target: character.id,
        sourceKey: hit.sourceKey,
        expectedRevision: snapshot.revision,
        region: hit.regions[0].id,
        nodeId: hit.geometrySources[0].id,
        handles: [
          {
            id: "picked-point",
            position: hit.restPoint,
            target: hit.restPoint.map((v, i) => v + input.restDisplacement[i]) as Vec3,
            tolerance: input.tolerance ?? 0.001,
          },
        ],
        protectedPoints,
        support: input.support,
        detail: input.detail,
        intent: input.intent,
      });
    },
    openFixture: (id: CreatureFixtureId) => this.openCreatureFixture(id),
    returnToProject: () => this.returnFromCreatureFixture(),
    encounter: {
      start: (target?: string) => this.startCreatureEncounter(target),
      stop: () => this.stopCreatureEncounter(),
      input: (input: { move?: [number, number]; dodge?: boolean; strike?: boolean }) =>
        this.setCreatureEncounterInput(input),
      inspect: () => this.encounterSession?.game.snapshot() ?? null,
    },
  };
  readonly store = new ProjectStore();
  private compiler: CompilerClient | null = null;
  renderer: WebGPURenderer | null = null;
  host: BrowserSceneHost | null = null;
  private listeners = new Set<() => void>();
  private raf = 0;
  private disposed = false;
  private dirty = true;
  private generation = 0;
  private seekGeneration = 0;
  private captureGeneration = 0;
  private captureBusy = false;
  private candidatePreviewAbort: AbortController | undefined;
  private encounterSession:
    | {
        host: BrowserSceneHost;
        game: CreatureEncounterGame;
        camera: Camera;
        playing: boolean;
        time: number;
        target: string;
      }
    | undefined;
  private encounterStartGeneration = 0;
  private encounterAccumulator = 0;
  private encounterInput = { move: [0, 0] as [number, number], dodge: false, strike: false };
  private last = 0;
  private lastUI = 0;
  private recoveryTimer: ReturnType<typeof setTimeout> | undefined;
  private state: PreviewState = {
    status: "starting",
    revision: -1,
    message: "Starting the studio",
    playing: false,
    seeking: false,
    time: 0,
    mode: "beauty",
    stage: "world",
    quality: "interactive",
    grid: false,
    hideGroom: false,
    overlays: [],
    exposure: 1,
    wind: 1,
    measurements: null,
    camera: { position: [24, 17, 32], target: [0, 1, 0], fov: 48 },
    diagnostics: [],
    recoveryDrafts: [],
  };
  private lastScene: EvaluatedScene | null = null;
  private lastWorldRevision = -1;
  private viewportCanvas: HTMLCanvasElement | undefined;
  private overlayCanvas: HTMLCanvasElement | undefined;
  private resizeObserver: ResizeObserver | undefined;
  private unsubscribeAuthoring: (() => void) | undefined;
  private preserveCamera = false;
  private sourceRevision = -1;
  private selection = "";
  private workspaceKey: string | null = null;
  private bridge = false;
  private projectBeforeCreature:
    | { project: Project; selection: string; stage: "world" | "studio"; stageId?: string }
    | undefined;
  getSnapshot = () => this.state;
  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };
  private emit(patch: Partial<PreviewState> = {}) {
    this.state = { ...this.state, ...patch };
    for (const fn of this.listeners) fn();
  }
  private report(error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    this.emit({
      status: "failed",
      seeking: false,
      playing: false,
      message,
      diagnostics: [{ severity: "error", code: "preview", message }],
    });
  }
  async start(canvas: HTMLCanvasElement, overlayCanvas?: HTMLCanvasElement) {
    this.overlayCanvas = overlayCanvas;
    this.viewportCanvas = canvas;
    let openingRevision = this.authoring.getSnapshot().revision;
    const assertOpeningSource = () => {
      if (this.authoring.getSnapshot().revision !== openingRevision)
        throw new Error("Source changed while opening saved work. Reopen explicitly to replace it.");
    };
    try {
      await this.store.open();
      const active = await this.store.loadRuntime("_active-project");
      const activeId = typeof active === "string" ? active : "winter-garden";
      const saved = await this.store.load(activeId),
        drafts = await this.store.listRecoveries(activeId),
        recovery = drafts.find(
          (draft) =>
            draft.key !== saved?.key &&
            (!saved ||
              draft.baseKey === saved.key ||
              (draft.baseKey === undefined && draft.updatedAt > saved.updatedAt)),
        );
      assertOpeningSource();
      if (recovery) {
        this.authoring.replace(recovery.project);
        this.emit({ message: "Recovered unsaved work" });
      } else if (saved) {
        this.authoring.replace(saved.project);
        this.authoring.markSaved(this.authoring.getSnapshot().revision);
      }
      openingRevision = this.authoring.getSnapshot().revision;
      try {
        const connection = await fetch("/bridge/session");
        if (connection.ok) {
          this.bridge = (await connection.json()).available;
        }
      } catch {
        /* Browser-only static hosting has no bridge. */
      }
      if (this.bridge) {
        const response = await fetch("/bridge/project");
        if (!response.ok) throw new Error("Workspace could not be read");
        const result = await response.json();
        if (result) {
          const knownDrafts = await this.store.listRecoveries(result.project.id);
          const preserved = new Set(knownDrafts.map((draft) => draft.key));
          // A workspace may be older than a browser snapshot from a failed
          // publication. Keep every differing local source before switching.
          for (const local of [saved, recovery]) {
            if (
              !local ||
              local.project.id !== result.project.id ||
              local.key === result.key ||
              preserved.has(local.key)
            )
              continue;
            assertOpeningSource();
            await this.store.preserveRecovery(
              local.project,
              local.revision,
              "Browser work before opening workspace",
            );
            preserved.add(local.key);
          }
          assertOpeningSource();
          this.workspaceKey = result.key;
          this.authoring.replace(result.project);
          this.authoring.markSaved(this.authoring.getSnapshot().revision);
        }
      }
      await this.refreshDrafts();
      if (this.disposed) {
        this.store.close();
        return;
      }
      this.compiler = new CompilerClient();
      this.host = new BrowserSceneHost(this.authoring.getSnapshot().project, {
        compile: this.compiler.compile,
        growVegetation: this.compiler.growVegetation,
        generateTerrain: this.compiler.generateTerrain,
      });
      this.renderer = await WebGPURenderer.create(canvas, {
        onDiagnostic: (d: Diagnostic) => {
          this.emit({ diagnostics: [...this.state.diagnostics.slice(-19), d] });
        },
      });
      if (this.disposed) {
        this.dispose();
        return;
      }
      this.unsubscribeAuthoring = this.authoring.subscribe(() => this.sourceChanged());
      this.resizeObserver = new ResizeObserver(() => {
        this.dirty = true;
      });
      this.resizeObserver.observe(canvas);
      await this.refresh(true);
      this.raf = requestAnimationFrame(this.frame);
      this.expose();
    } catch (e) {
      this.report(e);
    }
  }
  private sourceChanged() {
    const snapshot = this.authoring.getSnapshot();
    if (snapshot.revision !== this.sourceRevision || snapshot.selection !== this.selection) {
      this.emit({ pickedCreatureMaterial: undefined });
      this.stopCreatureEncounter();
      const changeSelection = snapshot.selection !== this.selection && !this.preserveCamera;
      this.preserveCamera = false;
      this.sourceRevision = snapshot.revision;
      this.selection = snapshot.selection;
      void this.refresh(changeSelection);
      clearTimeout(this.recoveryTimer);
      this.recoveryTimer = setTimeout(() => {
        void this.store.recover(snapshot.project, snapshot.revision).catch((e) => this.report(e));
      }, 300);
    }
  }
  private async refresh(resetCamera = false) {
    if (!this.host) return;
    const generation = ++this.generation,
      snapshot = this.authoring.getSnapshot();
    this.sourceRevision = snapshot.revision;
    this.selection = snapshot.selection;
    this.emit({ status: "compiling", seeking: true, message: "Preparing preview" });
    try {
      await this.host.setProject(snapshot.project);
      const subject =
        this.state.stage === "world" &&
        ["character", "object", "vegetation"].includes(
          snapshot.project.documents.find((d) => d.id === snapshot.selection)?.kind ?? "",
        )
          ? snapshot.project.entry
          : snapshot.selection;
      await this.host.prepare(subject, this.state.stageId || "neutral-stage", this.state.quality);
      if (generation !== this.generation || this.disposed) return;
      await this.host.seek(this.state.time);
      if (generation !== this.generation || this.disposed) return;
      const selected = snapshot.project.documents.find((d) => d.id === subject);
      if (!selected) throw Error("Selected subject no longer exists");
      if (resetCamera) {
        const isWorld = ["world", "terrain"].includes(selected.kind);
        const stage =
          selected.kind === "stage"
            ? selected
            : snapshot.project.documents.find((d) => d.id === this.state.stageId);
        this.state = {
          ...this.state,
          camera:
            !isWorld && stage?.kind === "stage"
              ? stage.camera
              : isWorld
                ? { position: [24, 17, 32], target: [0, 1, 0], fov: 48 }
                : selected.kind === "vegetation"
                  ? { position: [13, 8, 16], target: [0, 3, 0], fov: 42 }
                  : { position: [5, 3.2, 7], target: [0, 1.2, 0], fov: 42 },
        };
      }
      this.dirty = true;
      this.emit({
        status: "current",
        seeking: false,
        revision: snapshot.revision,
        message: "Preview current",
        diagnostics: [...this.host.diagnostics, ...(this.renderer?.diagnostics ?? [])],
      });
    } catch (e) {
      if (generation === this.generation) this.report(e);
    }
  }
  private frame = (now: number) => {
    if (this.disposed) return;
    const delta = Math.max(0, Math.min(0.1, (now - this.last) / 1000 || 0));
    this.last = Math.max(this.last, now);
    if (this.encounterSession) {
      try {
        this.encounterAccumulator = Math.min(0.1, this.encounterAccumulator + delta);
        while (this.encounterAccumulator >= 1 / 60) {
          this.encounterSession.game.step(1 / 60, this.encounterInput);
          this.encounterSession.host.advance(1 / 60, this.state.camera);
          this.encounterInput.dodge = false;
          this.encounterInput.strike = false;
          this.encounterAccumulator -= 1 / 60;
        }
        this.dirty = true;
      } catch (error) {
        this.stopCreatureEncounter();
        this.report(error);
      }
    } else if (this.state.playing && !this.state.seeking && this.host) {
      try {
        this.host.advance(delta, this.state.camera);
        this.state = { ...this.state, time: this.host.runtime?.clock.time ?? this.state.time };
        this.dirty = true;
      } catch (error) {
        this.report(error);
      }
    }
    const visibleHost = this.encounterSession?.host ?? this.host;
    if (
      (this.dirty ||
        this.renderer?.needsRender ||
        this.host?.world?.metrics.pending ||
        (this.host?.world?.metrics.revision ?? -1) !== this.lastWorldRevision) &&
      !this.state.seeking &&
      !this.candidatePreviewAbort &&
      this.renderer &&
      this.host
    ) {
      try {
        if (!visibleHost) throw new Error("Visible preview is unavailable");
        visibleHost.setViewportHeight(this.viewportCanvas?.height ?? 720);
        visibleHost.updateView(this.state.camera);
        const scene = applyCreatureInspection(
          visibleHost.extract(this.state.camera, this.state.mode, false),
          {
            hideGroom: this.state.hideGroom,
          },
        );
        if (this.encounterSession) {
          const encounter = this.encounterSession.game.snapshot();
          scene.surfaces.push(creaturePlayerMarker(encounter.playerPosition, encounter.invulnerable));
        }
        scene.grid = this.state.grid;
        if (this.state.reviewSunElevation !== undefined || this.state.reviewSunAzimuth !== undefined) {
          const elevation = this.state.reviewSunElevation ?? Math.asin(scene.environment.sunDirection[1]);
          const azimuth =
            this.state.reviewSunAzimuth ??
            Math.atan2(scene.environment.sunDirection[0], scene.environment.sunDirection[2]);
          scene.environment.sunDirection = [
            Math.cos(elevation) * Math.sin(azimuth),
            Math.sin(elevation),
            Math.cos(elevation) * Math.cos(azimuth),
          ];
        }
        scene.environment.exposure *= this.state.exposure;
        scene.environment.wind = scene.environment.wind.map((v) => v * this.state.wind) as Vec3;
        for (const surface of scene.surfaces)
          surface.selected =
            surface.source === this.authoring.getSnapshot().selection &&
            !["world", "terrain"].includes(
              this.authoring.getSnapshot().project.documents.find((d) => d.id === surface.source)?.kind ?? "",
            );
        visibleHost.applyIndirectLighting(scene);
        this.renderer.render(scene);
        this.lastScene = this.renderer.realizedScene ?? scene;
        if (this.overlayCanvas) {
          const overlay = this.overlayCanvas;
          if (
            overlay.width !== this.viewportCanvas?.width ||
            overlay.height !== this.viewportCanvas?.height
          ) {
            overlay.width = this.viewportCanvas?.width ?? 1;
            overlay.height = this.viewportCanvas?.height ?? 1;
          }
          const context = overlay.getContext("2d");
          if (context) {
            context.clearRect(0, 0, overlay.width, overlay.height);
            if (!this.encounterSession && this.state.overlays.length && this.host.runtime)
              drawOverlays(context, this.diagnosticLines().segments, scene.camera, this.state.overlays);
          }
        }
        this.lastWorldRevision = this.host.world?.metrics.revision ?? -1;
        this.dirty = false;
      } catch (e) {
        this.report(e);
      }
    }
    if (now - this.lastUI > 200) {
      this.lastUI = now;
      this.emit({
        measurements: this.renderer?.measurements ?? null,
        encounter: this.encounterSession?.game.snapshot(),
      });
    }
    this.raf = requestAnimationFrame(this.frame);
  };
  apply(batch: EditBatch) {
    return this.authoring.apply(batch);
  }
  async startCreatureEncounter(target = this.authoring.getSnapshot().selection) {
    if (this.captureBusy) throw Error("Wait for the current capture before starting an encounter");
    if (!this.compiler || this.disposed) throw Error("Studio compiler is unavailable");
    if (target !== "ash-warden" && target !== "reed-penitent")
      throw Error("Open an original creature study before starting its encounter");
    this.stopCreatureEncounter();
    const generation = ++this.encounterStartGeneration;
    const source = this.authoring.getSnapshot();
    const character = source.project.documents.find((document) => document.id === target);
    if (character?.kind !== "character") throw Error("Creature study is not present in this project");
    const fixture = createCreatureFixture(target);
    const host = new BrowserSceneHost(structuredClone(source.project), {
      compile: this.compiler.compile,
      growVegetation: this.compiler.growVegetation,
      generateTerrain: this.compiler.generateTerrain,
    });
    this.emit({ encounterStarting: true });
    try {
      await host.prepare(
        target,
        source.project.documents.some((document) => document.id === fixture.stageId)
          ? fixture.stageId
          : "neutral-stage",
        "interactive",
      );
      if (
        generation !== this.encounterStartGeneration ||
        this.captureBusy ||
        this.disposed ||
        this.authoring.getSnapshot().revision !== source.revision
      )
        throw Error("Encounter preparation superseded by changed source or another request");
      if (!host.runtime) throw Error("Encounter runtime is unavailable");
      const game = new CreatureEncounterGame({
        creatureId: target,
        encounter: fixture.encounter,
        motions: character.motions,
        runtime: host.runtime,
      });
      this.encounterSession = {
        host,
        game,
        target,
        camera: structuredClone(this.state.camera),
        playing: this.state.playing,
        time: this.state.time,
      };
      this.encounterAccumulator = 0;
      this.emit({
        encounter: game.snapshot(),
        playing: false,
        camera: { position: [0, 10, 15], target: [0, 0.8, 0], fov: 58 },
      });
      this.dirty = true;
      return game.snapshot();
    } catch (error) {
      host.dispose();
      throw error;
    } finally {
      if (generation === this.encounterStartGeneration) this.emit({ encounterStarting: false });
    }
  }
  stopCreatureEncounter() {
    this.encounterStartGeneration++;
    const encounter = this.encounterSession;
    this.encounterSession = undefined;
    this.encounterInput = { move: [0, 0], dodge: false, strike: false };
    if (encounter) {
      encounter.game.dispose();
      encounter.host.dispose();
      this.emit({
        encounter: undefined,
        encounterStarting: false,
        camera: encounter.camera,
        playing: encounter.playing,
        time: encounter.time,
      });
      this.dirty = true;
    } else if (this.state.encounterStarting) this.emit({ encounterStarting: false });
  }
  private setCreatureEncounterInput(input: { move?: [number, number]; dodge?: boolean; strike?: boolean }) {
    if (!this.encounterSession) throw Error("Start a creature encounter before sending actions");
    const parsed = z
      .object({
        move: z.tuple([z.number().min(-1).max(1), z.number().min(-1).max(1)]).optional(),
        dodge: z.boolean().optional(),
        strike: z.boolean().optional(),
      })
      .strict()
      .parse(input);
    this.encounterInput = { ...this.encounterInput, ...parsed };
    return this.encounterSession.game.snapshot();
  }
  async openCreatureFixture(id: CreatureFixtureId) {
    const fixture = createCreatureFixture(id);
    const source = this.authoring.getSnapshot();
    await this.store.preserveRecovery(source.project, source.revision, "Before opening creature study");
    if (this.authoring.getSnapshot().revision !== source.revision)
      throw Error("Source changed while preserving your project. Open the creature study again.");
    this.projectBeforeCreature ??= {
      project: structuredClone(source.project),
      selection: source.selection,
      stage: this.state.stage,
      stageId: this.state.stageId,
    };
    this.authoring.replace(fixture.project);
    this.authoring.select(fixture.characterId);
    this.patch({ stage: "studio", stageId: fixture.stageId, playing: false });
    this.emit({ returnProjectName: this.projectBeforeCreature.project.name });
    await this.refreshDrafts();
    return { characterId: fixture.characterId, stageId: fixture.stageId, brief: fixture.brief };
  }
  async returnFromCreatureFixture() {
    const previous = this.projectBeforeCreature;
    if (!previous) throw Error("No earlier project is retained in this window");
    const current = this.authoring.getSnapshot();
    await this.store.preserveRecovery(
      current.project,
      current.revision,
      "Creature study before returning to project",
    );
    if (this.authoring.getSnapshot().revision !== current.revision)
      throw Error("Source changed while preserving the creature study. Return to the project again.");
    this.authoring.replace(previous.project);
    this.authoring.select(previous.selection);
    this.patch({ stage: previous.stage, stageId: previous.stageId, playing: false });
    this.projectBeforeCreature = undefined;
    this.emit({ returnProjectName: undefined });
    await this.refreshDrafts();
  }
  async seek(time: number, playing = false) {
    if (this.encounterSession) this.stopCreatureEncounter();
    if (!Number.isFinite(time) || time < 0 || time > 120)
      throw Error("Preview time must be between 0 and 120 seconds");
    if (!this.host) throw Error("Preview is not prepared");
    const generation = ++this.seekGeneration;
    if (this.state.status === "starting" || this.state.status === "compiling") {
      await this.prepare();
      if (generation !== this.seekGeneration) return;
    }
    this.emit({ seeking: true, playing: false, message: "Replaying physics" });
    try {
      await this.host.seek(time, (fraction) => {
        if (generation === this.seekGeneration)
          this.emit({ message: `Replaying physics · ${Math.round(fraction * 100)}%` });
      });
      if (generation !== this.seekGeneration || this.disposed) return;
      this.dirty = true;
      this.emit({ time, playing, seeking: false, message: "Preview current" });
    } catch (error) {
      if (generation !== this.seekGeneration || this.disposed) return;
      this.emit({ seeking: false });
      if (!(error instanceof DOMException && error.name === "AbortError")) throw error;
    }
  }
  patch(patch: Partial<PreviewState>) {
    if (patch.time !== undefined) {
      const { time, playing, ...rest } = patch;
      this.patch(rest);
      void this.seek(time, playing ?? false).catch((error) => this.report(error));
      return;
    }
    const reprepare = patch.stage !== undefined || patch.stageId !== undefined || patch.quality !== undefined;
    if (this.encounterSession && (reprepare || patch.playing !== undefined)) this.stopCreatureEncounter();
    this.emit(patch);
    this.dirty = true;
    if (reprepare) void this.refresh(true);
  }
  pick(x: number, y: number, width: number, height: number) {
    if (this.encounterSession) return null;
    if (!this.lastScene) return null;
    const hit = pickScene(this.lastScene, cameraRay(this.lastScene.camera, x, y, width, height));
    if (hit) {
      const pickedCreatureMaterial = inspectCreatureMaterialAtHit(this.lastScene, hit);
      this.preserveCamera = true;
      this.authoring.select(hit.documentId);
      this.emit({ pickedInstance: hit.instanceId, pickedCreatureMaterial });
      this.dirty = true;
    }
    return hit;
  }
  orbit(dx: number, dy: number) {
    const { position, target } = this.state.camera;
    const x = position[0] - target[0],
      y = position[1] - target[1],
      z = position[2] - target[2],
      r = Math.hypot(x, y, z);
    let theta = Math.atan2(x, z) - dx * 0.007,
      phi = Math.acos(y / r) + dy * 0.007;
    phi = Math.max(0.06, Math.min(Math.PI * 0.48, phi));
    this.patch({
      camera: {
        ...this.state.camera,
        position: [
          target[0] + r * Math.sin(phi) * Math.sin(theta),
          target[1] + r * Math.cos(phi),
          target[2] + r * Math.sin(phi) * Math.cos(theta),
        ],
      },
    });
  }
  zoom(delta: number) {
    const { position, target } = this.state.camera,
      factor = Math.exp(Math.max(-0.2, Math.min(0.2, delta * 0.001)));
    const distance = Math.hypot(...position.map((v, i) => v - target[i]));
    if (distance * factor < 1 || distance * factor > 1500) return;
    this.patch({
      camera: {
        ...this.state.camera,
        position: position.map((v, i) => target[i] + (v - target[i]) * factor) as Vec3,
      },
    });
  }
  pan(dx: number, dy: number) {
    const { position, target } = this.state.camera;
    const distance = Math.hypot(...position.map((v, i) => v - target[i]));
    const yaw = Math.atan2(position[0] - target[0], position[2] - target[2]);
    const shift: Vec3 = [
      (-dx * Math.cos(yaw) + dy * Math.sin(yaw)) * distance * 0.0015,
      0,
      (dx * Math.sin(yaw) + dy * Math.cos(yaw)) * distance * 0.0015,
    ];
    this.patch({
      camera: {
        ...this.state.camera,
        position: position.map((v, i) => v + shift[i]) as Vec3,
        target: target.map((v, i) => v + shift[i]) as Vec3,
      },
    });
  }
  async travel(x: number, z: number) {
    if (!this.host?.world) throw Error("Open the world stage to travel");
    this.emit({ status: "compiling", message: "Preparing destination" });
    try {
      const result = await this.host.world.teleport([x, 0, z], {
        id: "view",
        visualRadius: 192,
        collisionRadius: 24,
      });
      if (!result.ready) throw Error(`Destination is not ready: ${result.missing.join(", ")}`);
      const old = this.state.camera,
        target: Vec3 = [x, 1, z],
        position: Vec3 = [x + 24, 17, z + 32];
      this.patch({ camera: { ...old, position, target }, status: "current", message: "Destination ready" });
    } catch (error) {
      this.report(error);
      throw error;
    }
  }
  removeInstance(id: string, removed = true) {
    const world = this.host?.world;
    if (!world) throw Error("Open a world stage first");
    if (!this.lastScene?.surfaces.some((s) => s.instanceId === id) && !world.persistence.overrides.has(id))
      throw Error("Select a resident occurrence before changing it");
    world.persistence.overrides.set(id, { ...world.persistence.overrides.get(id), removed });
    this.dirty = true;
  }
  async saveRuntime() {
    await this.prepare();
    const world = this.host?.world;
    if (!world) throw Error("Open a world stage first");
    const save = this.host?.saveRuntime();
    if (!save) throw Error("Runtime is unavailable");
    await this.store.saveRuntime(`world:${this.authoring.getSnapshot().project.id}:${world.world.id}`, save);
    return save;
  }
  async restoreRuntime() {
    const world = this.host?.world;
    if (!world) throw Error("Open a world stage first");
    const save = await this.store.loadRuntime(
      `world:${this.authoring.getSnapshot().project.id}:${world.world.id}`,
    );
    if (!save) throw Error("No runtime save exists for this world");
    await this.loadRuntime(save);
  }
  async loadRuntime(save: unknown) {
    if (!this.host?.world) throw Error("Open a world stage first");
    this.emit({ playing: false, seeking: true, message: "Restoring runtime" });
    try {
      const restored = await this.host?.loadRuntime(save as Parameters<BrowserSceneHost["loadRuntime"]>[0]);
      if (restored) {
        this.emit({ time: restored.time });
        this.last = performance.now();
      }
      this.dirty = true;
    } finally {
      this.emit({ seeking: false, message: "Preview current" });
    }
  }
  async resetRuntime() {
    if (!this.host) throw Error("Preview is unavailable");
    this.emit({ playing: false, seeking: true, message: "Resetting runtime" });
    try {
      await this.host.resetRuntime();
      this.emit({ time: 0 });
      this.dirty = true;
    } finally {
      this.emit({ seeking: false, message: "Preview current" });
    }
  }
  async save() {
    const { project, revision } = this.authoring.getSnapshot();
    if (this.bridge) {
      const expectedKey = this.workspaceKey;
      // Publishing may fail or the tab may close while it is in flight. Keep
      // this snapshot durable without deleting a newer recovery revision.
      await this.store.recover(project, revision);
      if (this.authoring.getSnapshot().revision !== revision || this.workspaceKey !== expectedKey)
        throw new Error("Source changed while preparing the save. Save the current work again.");
      const response = await fetch("/bridge/project", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ project, expectedKey }),
      });
      const result = await response.json();
      if (!response.ok) throw Error(result.error);
      this.workspaceKey = result.key;
      const current = this.authoring.getSnapshot();
      if (current.project.id === project.id && current.revision > revision)
        await this.store.recover(current.project, current.revision);
    }
    await this.store.save(project, revision);
    if (this.authoring.getSnapshot().project.id === project.id) this.authoring.markSaved(revision);
    await this.refreshDrafts();
    return { revision, key: contentKey(project) };
  }
  private async refreshDrafts() {
    const project = this.authoring.getSnapshot().project;
    const key = contentKey(project);
    const drafts = await this.store.listRecoveries(project.id);
    this.emit({
      recoveryDrafts: drafts
        .filter((draft) => draft.key !== key)
        .map((draft) => ({
          writer: draft.writer,
          updatedAt: draft.updatedAt,
          name: draft.label ? `${draft.label} · ${draft.project.name}` : draft.project.name,
          baseKey: draft.baseKey,
          key: draft.key,
        })),
    });
  }
  async restoreDraft(writer: string) {
    const current = this.authoring.getSnapshot();
    const draft = (await this.store.listRecoveries(current.project.id)).find(
      (item) => item.writer === writer,
    );
    if (!draft) throw new Error("Recovery draft is no longer available");
    const assertCurrent = () => {
      if (this.authoring.getSnapshot().revision !== current.revision)
        throw new Error("Source changed while restoring a draft. Retry against the current work.");
    };
    assertCurrent();
    const preserved = await this.store.preserveRecovery(
      current.project,
      current.revision,
      "Before restoring a draft",
    );
    assertCurrent();
    this.authoring.replace(draft.project);
    await this.refreshDrafts();
    return { writer, key: draft.key, preservedWriter: preserved.writer };
  }
  async reopen() {
    if (this.bridge) {
      const response = await fetch("/bridge/project");
      if (!response.ok) throw Error("Workspace could not be read");
      const current = await response.json();
      if (!current) throw Error("Workspace has no saved generation");
      this.workspaceKey = current.key;
      this.authoring.replace(current.project);
      this.authoring.markSaved(this.authoring.getSnapshot().revision);
      await this.store.save(this.authoring.getSnapshot().project, this.authoring.getSnapshot().revision);
      return;
    }
    const saved = await this.store.load(this.authoring.getSnapshot().project.id);
    if (!saved) throw Error("Save the project before reopening it");
    this.authoring.replace(saved.project);
    this.authoring.markSaved(this.authoring.getSnapshot().revision);
  }
  async prepare(options: PrepareOptions = {}) {
    options = prepareOptionsSchema.parse(options);
    if (options.subject) {
      if (!this.authoring.getSnapshot().project.documents.some((d) => d.id === options.subject))
        throw Error("Unknown preview subject");
      this.authoring.select(options.subject);
    }
    if (options.quality && options.quality !== this.state.quality) this.patch({ quality: options.quality });
    if (options.stage && options.stage !== this.state.stage) this.patch({ stage: options.stage });
    if (options.stageId !== undefined && options.stageId !== this.state.stageId) {
      if (
        !this.authoring
          .getSnapshot()
          .project.documents.some((d) => d.id === options.stageId && d.kind === "stage")
      )
        throw Error("Unknown reference stage");
      this.patch({ stageId: options.stageId });
    }
    if (options.region) {
      if (!this.host?.world) throw Error("A world must be active to prepare a region");
      const { center, radius } = options.region;
      if (!Number.isFinite(radius) || radius < 1 || radius > 512 || !center.every(Number.isFinite))
        throw Error("Invalid capture region");
      this.host.world.setInterest({
        id: "capture",
        position: [center[0], 0, center[1]],
        visualRadius: radius,
        collisionRadius: 0,
      });
      this.host.world.update();
      this.dirty = true;
    }

    const revision = options.revision ?? this.authoring.getSnapshot().revision,
      deadline = performance.now() + (options.timeoutMs ?? 15000);
    while (
      this.state.revision !== revision ||
      this.state.status !== "current" ||
      this.state.seeking ||
      this.dirty ||
      this.renderer?.needsRender ||
      this.host?.world?.metrics.pending ||
      (this.host?.world?.metrics.revision ?? -1) !== this.lastWorldRevision
    ) {
      if (this.authoring.getSnapshot().revision !== revision)
        throw Error("Requested revision was superseded");
      if (this.state.status === "failed") throw Error(this.state.message);
      if (this.state.revision === revision && !this.dirty) assertRenderAccepted(this.renderer?.completeness);
      if (performance.now() > deadline)
        throw Error("Preview readiness timed out; required products are not ready");
      await new Promise((r) => setTimeout(r, 25));
    }
    assertRenderAccepted(this.renderer?.completeness);
    return { revision, missing: [] };
  }
  private diagnosticLines() {
    if (!this.host?.runtime || !this.lastScene) throw Error("Preview diagnostics are unavailable");
    const origin = this.state.camera.position.map(
      (v, i) => v - (this.lastScene?.camera.position[i] ?? v),
    ) as Vec3;
    return runtimeDiagnostics(this.host.runtime, { time: this.state.time, origin });
  }
  async capture(options: CaptureOptions = {}) {
    if (this.encounterSession) throw Error("Stop the encounter before capturing source review images");
    if (this.captureBusy) throw Error("A capture is already running; wait for it to finish");
    this.captureBusy = true;
    try {
      return await this.captureCurrent(options);
    } finally {
      this.captureBusy = false;
    }
  }
  async previewCreatureCandidate(id: string, options: CandidatePreviewOptions) {
    if (this.encounterSession) throw Error("Stop the encounter before comparing source candidates");
    if (this.captureBusy) throw Error("A capture is already running; wait for it to finish");
    if (!this.compiler || this.disposed) throw Error("Studio compiler is unavailable");
    const compiler = this.compiler;
    this.captureBusy = true;
    const abort = new AbortController();
    this.candidatePreviewAbort = abort;
    try {
      return await previewCreatureCandidate(
        this.authoring,
        id,
        options,
        async (signal) => {
          const canvas = document.createElement("canvas");
          canvas.setAttribute("aria-hidden", "true");
          canvas.style.cssText = `position:fixed;left:-10000px;top:0;width:${options.width ?? 320}px;height:${options.height ?? 240}px;pointer-events:none;visibility:hidden`;
          document.body.append(canvas);
          let renderer: WebGPURenderer | undefined;
          const hosts = new Set<BrowserSceneHost>();
          const hostsBySource = new Map<string, BrowserSceneHost>();
          let disposed = false;
          const dispose = () => {
            if (disposed) return;
            disposed = true;
            signal.removeEventListener("abort", dispose);
            for (const host of hosts) host.dispose();
            hosts.clear();
            hostsBySource.clear();
            renderer?.dispose();
            canvas.remove();
          };
          signal.addEventListener("abort", dispose, { once: true });
          try {
            signal.throwIfAborted();
            renderer = await WebGPURenderer.create(canvas, {
              quality: "low",
              pixelRatio: 1,
              maxGpuBytes: 64 * 1024 * 1024,
              maxUploadBytesPerFrame: 4 * 1024 * 1024,
            });
            if (disposed || signal.aborted) {
              renderer.dispose();
              signal.throwIfAborted();
              throw Error("Candidate preview was disposed");
            }
            const activeRenderer = renderer;
            return {
              dispose,
              render: async (project, settings, renderSignal) => {
                renderSignal.throwIfAborted();
                const sourceKey = contentKey(project);
                let host = hostsBySource.get(sourceKey);
                if (!host) {
                  if (hostsBySource.size >= 2)
                    throw Error("Candidate preview exceeded its isolated host budget");
                  host = new BrowserSceneHost(project, {
                    compile: compiler.compile,
                    growVegetation: compiler.growVegetation,
                    generateTerrain: compiler.generateTerrain,
                    maxCacheBytes: 32 * 1024 * 1024,
                    maxInstalledBytes: 32 * 1024 * 1024,
                  });
                  hosts.add(host);
                  await host.prepare(settings.subject, settings.stageId ?? "neutral-stage", "interactive");
                  renderSignal.throwIfAborted();
                  if (settings.motion) host.playMotion(settings.subject, settings.motion, 0);
                  hostsBySource.set(sourceKey, host);
                }
                // Motion commands enter the isolated runtime on tick 1; requested tick is motion-relative.
                await host.seek((settings.tick + (settings.motion ? 1 : 0)) / 60);
                renderSignal.throwIfAborted();
                host.setViewportHeight(settings.height);
                const scene = applyCreatureInspection(host.extract(settings.camera, settings.channel), {
                  hideGroom: settings.hideGroom,
                });
                scene.grid = false;
                activeRenderer.render(scene);
                const captured = await activeRenderer.capture();
                const overlayDiagnostics =
                  settings.overlays.length && host.runtime
                    ? runtimeDiagnostics(host.runtime, {
                        time: host.runtime.clock.time,
                        origin: scene.origin ?? [0, 0, 0],
                      })
                    : undefined;
                const blob = overlayDiagnostics
                  ? await overlayCapture(
                      captured,
                      overlayDiagnostics.segments,
                      scene.camera,
                      settings.overlays,
                    )
                  : captured;
                renderSignal.throwIfAborted();
                assertRenderAccepted(activeRenderer.completeness);
                return {
                  blob,
                  measurements: structuredClone(activeRenderer.measurements),
                  diagnostics: [...host.diagnostics, ...activeRenderer.diagnostics],
                };
              },
            };
          } catch (error) {
            dispose();
            throw error;
          }
        },
        abort.signal,
      );
    } finally {
      this.candidatePreviewAbort = undefined;
      this.captureBusy = false;
      this.dirty = true;
    }
  }
  private async captureCurrent(options: CaptureOptions = {}) {
    options = captureOptionsSchema.parse(options);
    const channels = options.channels ?? [this.state.mode];
    const overlays = options.overlays ?? this.state.overlays;
    if (
      channels.length < 1 ||
      channels.length > VIEW_MODES.length ||
      channels.some((c) => !VIEW_MODES.includes(c))
    )
      throw Error("Unsupported capture channels");
    if (
      options.tick !== undefined &&
      (!Number.isInteger(options.tick) || options.tick < 0 || options.tick > 7200)
    )
      throw Error("Capture tick must be between 0 and 7200");
    const generation = ++this.captureGeneration;
    await this.prepare(options);
    if (generation !== this.captureGeneration) throw Error("Capture superseded");
    const prior = { playing: this.state.playing, mode: this.state.mode, camera: this.state.camera };
    const revision = this.state.revision;
    this.patch({
      playing: false,
      ...(options.camera ? { camera: this.resolveCamera(options.camera) } : {}),
    });
    const requestedCameraKey = contentKey(this.state.camera);
    let requestedMode = this.state.mode;
    try {
      if (options.tick !== undefined) await this.seek(options.tick / 60);
      if (!this.renderer) throw Error("Renderer is unavailable");
      const cameraKey = contentKey(this.state.camera),
        captureTime = this.state.time;
      const viewKey = () =>
        contentKey({
          stage: this.state.stage,
          stageId: this.state.stageId,
          quality: this.state.quality,
          selection: this.authoring.getSnapshot().selection,
          exposure: this.state.exposure,
          reviewSunElevation: this.state.reviewSunElevation,
          reviewSunAzimuth: this.state.reviewSunAzimuth,
          wind: this.state.wind,
          grid: this.state.grid,
          hideGroom: this.state.hideGroom,
          playing: this.state.playing,
          width: this.viewportCanvas?.clientWidth,
          height: this.viewportCanvas?.clientHeight,
        });
      const captureViewKey = viewKey();
      const assertCapture = (mode: EvaluatedScene["mode"]) => {
        if (
          generation !== this.captureGeneration ||
          contentKey(this.state.camera) !== cameraKey ||
          this.state.time !== captureTime ||
          viewKey() !== captureViewKey ||
          this.state.mode !== mode
        )
          throw Error("Capture view or clock changed during evaluation");
        if (this.authoring.getSnapshot().revision !== revision)
          throw Error("Capture source changed during evaluation");
      };
      const images: Partial<Record<EvaluatedScene["mode"], Blob>> = {};
      const frames: Partial<Record<EvaluatedScene["mode"], RenderCompleteness>> = {};
      let identitySurfaces: EvaluatedScene["surfaces"] = [];
      for (const mode of channels) {
        requestedMode = mode;
        this.patch({ mode });
        await this.prepare({ revision, timeoutMs: options.timeoutMs });
        assertCapture(mode);
        const captured = await this.renderer.capture();
        frames[mode] = structuredClone(this.renderer.completeness);
        if (mode === "identity")
          identitySurfaces = (this.renderer.realizedScene?.surfaces ?? []).filter((surface) =>
            this.renderer?.completeness.rendered.includes(surface.id),
          );
        images[mode] =
          overlays.length && this.lastScene
            ? await overlayCapture(captured, this.diagnosticLines().segments, this.lastScene.camera, overlays)
            : captured;
        assertCapture(mode);
      }
      const metadata = {
        revision,
        key: contentKey(this.authoring.getSnapshot().project),
        stage:
          this.state.stage === "studio"
            ? this.state.stageId || "neutral-stage"
            : this.authoring.getSnapshot().project.entry,
        camera: this.state.camera,
        tick: Math.round(this.state.time * 60),
        quality: this.state.quality,
        subject: this.authoring.getSnapshot().selection,
        viewOverrides: {
          exposure: this.state.exposure,
          reviewSunElevation: this.state.reviewSunElevation,
          reviewSunAzimuth: this.state.reviewSunAzimuth,
          wind: this.state.wind,
          grid: this.state.grid,
          hideGroom: this.state.hideGroom,
        },
        rendering: {
          profile: this.renderer.qualityProfile,
          outputResolution: this.renderer.measurements.outputResolution,
          renderResolution: this.renderer.measurements.renderResolution,
        },
        environment: navigator.userAgent,
        adapter: this.renderer.measurements.adapter,
        channels,
        overlays,
        diagnostics: overlays.length ? this.diagnosticLines() : undefined,
        waterApproximations: this.lastScene?.surfaces
          .filter((surface) => surface.waterApproximation)
          .map((surface) => ({ document: surface.source, ...surface.waterApproximation })),
        identities: channels.includes("identity")
          ? Array.from(
              new Map(
                identitySurfaces.map((surface) => {
                  const id = surface.instanceId ?? surface.id;
                  return [
                    id,
                    {
                      id,
                      document: surface.source,
                      color: identityColor(id).map((v) => Math.round(v * 255)),
                    },
                  ];
                }),
              ).values(),
            )
          : undefined,
        region: options.region,
        missing: this.renderer.completeness.rejected.map((item) => item.id),
        completeness: structuredClone(this.renderer.completeness),
        frames,
      };
      const blob = images[channels[0]];
      if (!blob) throw Error("Capture produced no image");
      return { blob, images, metadata };
    } finally {
      if (generation === this.captureGeneration) {
        this.host?.world?.removeInterest("capture");
        this.patch({
          ...(!this.state.playing ? { playing: prior.playing } : {}),
          ...(this.state.mode === requestedMode ? { mode: prior.mode } : {}),
          ...(contentKey(this.state.camera) === requestedCameraKey ? { camera: prior.camera } : {}),
        });
      }
    }
  }
  private resolveCamera(view: Camera | NamedView): Camera {
    if (typeof view !== "string") return view;
    const { target, position, fov } = this.state.camera;
    const radius = Math.max(2, Math.hypot(...position.map((v, i) => v - target[i])));
    const directions: Record<NamedView, Vec3> = {
      front: [0, 0, 1],
      back: [0, 0, -1],
      left: [-1, 0, 0],
      right: [1, 0, 0],
      top: [0, 1, 0.0001],
      "three-quarter": [0.6, 0.35, 0.72],
      "valley-overlook": [0.6, 0.4, 0.7],
    };
    const direction = directions[view],
      length = Math.hypot(...direction);
    return {
      target: [...target],
      position: target.map((v, i) => v + (direction[i] * radius) / length) as Vec3,
      fov,
    };
  }
  async exportPlayer() {
    const project = this.authoring.getSnapshot().project;
    const identity = await fetch("/compiler-identity.json");
    if (!identity.ok) throw new Error("Compiler build identity is unavailable");
    const { compilerSource } = z
      .object({ compilerSource: z.string().regex(/^[a-f0-9]{64}$/) })
      .parse(await identity.json());
    const [response, workerResponse] = await Promise.all([
      fetch("/player-bundle.js"),
      fetch("/compile-worker.js"),
    ]);
    if (!response.ok) throw Error("Standalone player bundle is unavailable");
    const code = await response.text();
    if (!workerResponse.ok) throw Error("Compiler worker bundle is unavailable");
    const workerCode = await workerResponse.text();
    const workerUrl = URL.createObjectURL(new Blob([workerCode], { type: "text/javascript" }));
    let compiler: BrowserCompiler | undefined;
    try {
      compiler = new BrowserCompiler(workerUrl, 2, "classic", 30000, compilerSource);
      const cooked = await cookProjectAsync(project, "review", compiler.compile, compilerSource);
      return standalonePlayer(project, code, workerCode, cooked);
    } finally {
      compiler?.dispose();
      URL.revokeObjectURL(workerUrl);
    }
  }
  async playGame() {
    const player = window.open("about:blank", "_blank");
    if (!player) throw new Error("Allow a new player window, or use Export to play the downloaded project.");
    player.opener = null;
    try {
      const url = URL.createObjectURL(await this.exportPlayer());
      player.location.href = `${url}#play`;
      setTimeout(() => URL.revokeObjectURL(url), 60000);
    } catch (error) {
      player.close();
      throw error;
    }
  }
  expose() {
    const api = {
      discover: (query: DiscoveryQuery = {}) => ({
        ...this.authoring.discover(query),
        preview: {
          controls: z.toJSONSchema(previewControlsSchema),
          prepare: z.toJSONSchema(prepareOptionsSchema),
          capture: z.toJSONSchema(captureOptionsSchema),
          views: NAMED_VIEWS,
          channels: VIEW_MODES,
          seek: { argument: "seconds", minimum: 0, maximum: 120, asynchronous: true },
          playMotion: { arguments: ["instance or definition id", "motion id", "blend seconds (0–5)"] },
        },
        methods: {
          "query.water": "(definitionId, x, z, seconds?) → height, normal, velocity",
          "query.ground": "(x, z) → ready height/normal or explicit not-resident",
          measure: "() → source identity, frame, residency and memory measurements",
          export: "() → Promise<Blob> containing a self-contained HTML player",
          project: "() → authored project JSON",
          "world.save": "() → Promise<WorldSave>; persists runtime independently of source",
          "world.restore": "() → restore compatible saved runtime",
          "world.load": "(WorldSave) → validate and restore a runtime backup",
          "world.reset": "() → clear live runtime changes",
          "world.removeInstance": "(instanceId, removed=true) → hide/restore a resident occurrence",
          "creature.inspect": "(characterId, regionId?) → anatomy and linked authoring controls",
          "creature.fields":
            "(characterId) → typed fields with units, coordinate spaces, bounded support and actual edit operations",
          "creature.dependencies": "(characterId,{domain,id},direction?) → source dependency/dependent paths",
          "creature.select": "(characterId, restPoint, radius?) → anatomical source selection",
          "creature.explain": "(characterId, regionId) → source dependencies",
          "creature.solve": "(request) → bounded, inspectable authoring proposal",
          "creature.repair":
            "(request) → rest-space handle fit with protected points, displacement/gradient bounds, measured sensitivities and an isolated sculpt candidate",
          "creature.repairPixel":
            "({id,pixel,restDisplacement,support,protectPixels?,tolerance?,detail?,intent}) → ground the current body surface and prepare a repair; displacement is character-rest metres, not screen pixels; compare posed captures before adoption",
          "creature.jobs":
            "start({request,deadlineMs?,evaluationsPerSlice?}), inspect/list/pause/resume/cancel/await/release/usage; asynchronous solver prepares a candidate without adoption",
          "creature.review": "(characterId, scenarioId) → deterministic CPU motion evidence",
          "creature.playScenario":
            "(characterId, scenarioId) → preview the authored motion and review camera",
          "creature.capture": "(captureOptions) → revision-bound studio capture",
          "creature.previewCandidate":
            "(candidateId, {subject,camera,motion?,tick?,ticks?,channel?,overlays?,hideGroom?,stageId?,width?,height?,timeoutMs?}) → isolated matched A/B pose sequence; ≤8 frames,1 megapixel/version,512px/30s limits",
          "creature.inspectView":
            "({channel?,skeleton?,hideGroom?}) → presentation-only inspection. Clay + skeleton: {channel:'clay',skeleton:true,hideGroom:true}; preserves camera/source/pose",
          "creature.inspectPixel":
            "({x,y,width?,height?}) → actual rendered triangle's weighted anatomical regions, rest coordinates, material/optical properties and albedo factors; viewport pixels",
          "creature.groundPixel":
            "({x,y,width?,height?}) → validated current displayed-LOD source grounding: chart anchors, material/source nodes, skin joints and editable controls; rejects stale realization",
          "creature.compareCaptures": "(baselineMetadata, candidateMetadata) → matching review conditions",
          "creature.candidates": "propose, list, inspect, compare, adopt, cancel, release",
          "creature.openFixture":
            "(ash-warden | reed-penitent) → open an original study, preserving current work in recovery",
          "creature.returnToProject":
            "() → preserve the creature study and return to the earlier project in this window",
          "creature.encounter":
            "start(characterId?), input({move:[x,z],dodge?,strike?}), inspect(), stop(); isolated playable study with original preview restored on stop",
        },
      }),
      inspect: (id?: string) => this.authoring.inspect(id),
      inspectFields: (query: InspectionQuery) => this.authoring.inspectFields(query),
      impact: (batch: EditBatch) => this.authoring.impact(batch),
      explain: (target: string, node?: string) =>
        explainAuthoringSource(this.authoring.getSnapshot().project, target, node),
      explainPixel: (input: { x: number; y: number; width?: number; height?: number }) => {
        if (!this.lastScene || this.state.revision !== this.authoring.getSnapshot().revision)
          throw Error("Prepare the current revision before picking");
        const hit = pickScene(
          this.lastScene,
          cameraRay(
            this.lastScene.camera,
            input.x,
            input.y,
            input.width ?? this.viewportCanvas?.clientWidth ?? 1,
            input.height ?? this.viewportCanvas?.clientHeight ?? 1,
          ),
        );
        return hit
          ? {
              hit,
              attribution: explainAuthoringSource(
                this.authoring.getSnapshot().project,
                hit.documentId,
                hit.nodeId,
              ),
            }
          : null;
      },
      work: this.work,
      creature: this.creature,
      apply: (batch: EditBatch) => this.apply(batch),
      transactions: {
        preview: (batch: EditBatch) => this.authoring.preview(batch),
        list: () => this.authoring.transactions(),
        inspect: (id: string) => this.authoring.inspectTransaction(id),
        revert: (id: string, options?: Parameters<AuthoringSession["revert"]>[1]) =>
          this.authoring.revert(id, options),
        usage: () => this.authoring.historyUsage(),
      },
      recovery: {
        list: () => this.store.listRecoveries(this.authoring.getSnapshot().project.id),
        restore: (writer: string) => this.restoreDraft(writer),
      },
      undo: () => this.authoring.undo(),
      redo: () => this.authoring.redo(),
      save: () => this.save(),
      reopen: () => this.reopen(),
      preview: {
        prepare: (options?: PrepareOptions) => this.prepare(options),
        set: (options: unknown) => this.patch(previewControlsSchema.parse(options)),
        capture: (options?: CaptureOptions) => this.capture(options),
        seek: (time: number) => this.seek(time),
        playMotion: (id: string, motion: string, blend = 0.2) => {
          if (!Number.isFinite(blend) || blend < 0 || blend > 5) throw Error("Invalid blend duration");
          if (!this.host) throw Error("Preview is unavailable");
          this.host.playMotion(id, motion, blend);
          this.patch({ playing: true });
        },
        setLocomotionSpeed: (id: string, speed: number) => {
          if (!this.host) throw Error("Preview is unavailable");
          this.host.setLocomotionSpeed(id, speed);
        },
        travel: (x: number, z: number) => this.travel(x, z),
        inspect: () => ({
          ...structuredClone(this.state),
          earliestSeekTick: this.host?.runtime?.earliestSeekTick ?? 0,
        }),
      },
      world: {
        inspect: () => (this.host?.world ? this.host.saveRuntime() : undefined),
        removeInstance: (id: string, removed = true) => this.removeInstance(id, removed),
        save: () => this.saveRuntime(),
        restore: () => this.restoreRuntime(),
        load: (save: unknown) => this.loadRuntime(save),
        reset: () => this.resetRuntime(),
      },
      query: {
        water: (id: string, x: number, z: number, time = this.state.time) => {
          const doc = this.authoring.getSnapshot().project.documents.find((d) => d.id === id);
          if (doc?.kind !== "water") throw Error("Unknown water definition");
          const surface = this.lastScene?.surfaces.find(
            (surface) => surface.source === id && surface.waterApproximation,
          );
          return { ...queryWater(doc, x, z, time), renderApproximation: surface?.waterApproximation };
        },
        ground: (x: number, z: number) =>
          this.host?.world?.queryGround(x, z) ?? { status: "unsupported", reason: "No active world" },
      },
      measure: () => ({
        source: {
          revision: this.state.revision,
          key: contentKey(this.authoring.getSnapshot().project),
          stage: this.state.stageId || this.state.stage,
          camera: structuredClone(this.state.camera),
          tick: Math.round(this.state.time * 60),
          quality: this.state.quality,
        },
        frame: this.renderer?.measurements,
        completeness: this.renderer?.completeness,
        world: this.host?.world?.metrics,
        compiler: this.host?.resourceUsage,
      }),
      export: () => this.exportPlayer(),
      play: () => this.playGame(),
      project: () => this.authoring.export(),
    };
    const agent = { ...api, execute: (request: unknown) => executeAgentRequest(api, request) };
    window.wrela = agent;
    return agent;
  }
  dispose() {
    this.disposed = true;
    this.stopCreatureEncounter();
    this.candidatePreviewAbort?.abort(new Error("Studio disposed"));
    cancelAnimationFrame(this.raf);
    this.resizeObserver?.disconnect();
    this.unsubscribeAuthoring?.();
    clearTimeout(this.recoveryTimer);
    this.renderer?.dispose();
    this.host?.dispose();
    this.compiler?.dispose();
    const snapshot = this.authoring.getSnapshot();
    void this.store
      .recover(snapshot.project, snapshot.revision)
      .catch(() => {})
      .finally(() => this.store.close());
    this.listeners.clear();
  }
}
const v3 = z.tuple([z.number().finite(), z.number().finite(), z.number().finite()]);
const cameraSchema = z
  .object({ position: v3, target: v3, fov: z.number().min(15).max(100) })
  .refine(
    (camera) => Math.hypot(...camera.position.map((v, i) => v - camera.target[i])) > 0.01,
    "Camera position and target must differ",
  );
const prepareOptionsSchema = z.object({
  revision: z.number().int().nonnegative().optional(),
  timeoutMs: z.number().int().min(100).max(120000).optional(),
  subject: z.string().min(1).max(100).optional(),
  stage: z.enum(["world", "studio"]).optional(),
  stageId: z.string().max(100).optional(),
  quality: z.enum(["interactive", "review", "export"]).optional(),
  region: z
    .object({
      center: z.tuple([z.number().finite(), z.number().finite()]),
      radius: z.number().min(1).max(512),
    })
    .optional(),
});
const captureOptionsSchema = prepareOptionsSchema
  .extend({
    tick: z.number().int().min(0).max(7200).optional(),
    channels: z.array(z.enum(VIEW_MODES)).min(1).max(VIEW_MODES.length).optional(),
    camera: z.union([cameraSchema, z.enum(NAMED_VIEWS)]).optional(),
    overlays: z
      .array(z.enum(["rig", "colliders"]))
      .max(2)
      .optional(),
  })
  .strict();
const previewControlsSchema = z
  .object({
    playing: z.boolean().optional(),
    time: z.number().min(0).max(120).optional(),
    mode: z.enum(VIEW_MODES).optional(),
    stage: z.enum(["world", "studio"]).optional(),
    stageId: z.string().max(100).optional(),
    quality: z.enum(["interactive", "review", "export"]).optional(),
    grid: z.boolean().optional(),
    hideGroom: z.boolean().optional(),
    overlays: z
      .array(z.enum(["rig", "colliders"]))
      .max(2)
      .optional(),
    exposure: z.number().min(0.1).max(8).optional(),
    wind: z.number().min(0).max(4).optional(),
    camera: cameraSchema.optional(),
  })
  .strict();
export const studio = new StudioController();
export { downloadBlob };

export type StudioAgentApi = ReturnType<StudioController["expose"]>;
declare global {
  interface Window {
    wrela: StudioAgentApi;
  }
}
