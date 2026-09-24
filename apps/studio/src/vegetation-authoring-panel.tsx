import { botanicalStructure, compileVegetation } from "@wrela/compiler";
import { shapedPineLookdevDefinition } from "@wrela/examples";
import {
  alpineConiferSchema,
  type BotanicalAuthoring,
  type BotanicalSpecies,
  botanicalPreset,
  createVegetationStand,
  type VegetationDefinition,
} from "@wrela/model";
import { useEffect, useMemo, useRef, useState } from "react";
import type { DomainPanelProps } from "./domain-authoring";
import { VegetationGrowthPanel } from "./vegetation-growth-panel";
import { vegetationReviewTriangles } from "./vegetation-review";
import { paintVegetationReview } from "./vegetation-review-paint";

function PlantReview({ document }: { document: VegetationDefinition }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [distance, setDistance] = useState(15);
  const [stand, setStand] = useState(false);
  const [angle, setAngle] = useState(0.4);
  const [silhouette, setSilhouette] = useState(false);
  const [time, setTime] = useState(0);
  const artifact = useMemo(() => compileVegetation(document, "interactive"), [document]);
  const population = useMemo(() => {
    if (!stand) return undefined;
    const population = createVegetationStand(document);
    return {
      ...population,
      artifacts: new Map(
        population.documents.map((variant) => [variant.id, compileVegetation(variant, "interactive")]),
      ),
    };
  }, [document, stand]);
  useEffect(() => {
    const context = canvas.current?.getContext("2d");
    if (!context) return;
    context.fillStyle = "#dce5e2";
    context.fillRect(0, 0, 360, 260);
    paintVegetationReview(
      context,
      vegetationReviewTriangles(
        document,
        artifact,
        { distance, stand, angle, silhouette, time },
        360,
        260,
        stand ? population : undefined,
      ),
    );
  }, [document, artifact, population, distance, stand, angle, silhouette, time]);
  return (
    <details open>
      <summary>Plant and stand review</summary>
      <canvas
        ref={canvas}
        width={360}
        height={260}
        style={{ width: "100%" }}
        aria-label="Geometry review of the authored vegetation"
      >
        Vegetation geometry review
      </canvas>
      <p className="hint">
        Geometry and silhouette inspection. Surface colors are illustrative; use the main viewport to judge
        materials and lighting.
      </p>
      <label className="row-control">
        <span>Distance (m)</span>
        <input
          type="number"
          min={0.5}
          max={500}
          value={distance}
          onChange={(event) => setDistance(Math.max(0.5, Number(event.target.value)))}
        />
      </label>
      <div>
        {(document.botanical?.review.distances ?? [3, 15, 60]).map((value) => (
          <button type="button" key={value} onClick={() => setDistance(value)}>
            {value} m
          </button>
        ))}
      </div>
      <label className="row-control">
        <span>Orbit</span>
        <input
          type="range"
          min={-3.14}
          max={3.14}
          step={0.05}
          value={angle}
          onChange={(event) => setAngle(Number(event.target.value))}
        />
      </label>
      <label className="row-control">
        <span>Wind phase</span>
        <input
          type="range"
          min={0}
          max={20}
          step={0.1}
          value={time}
          onChange={(event) => setTime(Number(event.target.value))}
        />
      </label>
      <label>
        <input type="checkbox" checked={stand} onChange={(event) => setStand(event.target.checked)} />
        Review stand
      </label>
      <label>
        <input
          type="checkbox"
          checked={silhouette}
          onChange={(event) => setSilhouette(event.target.checked)}
        />
        Silhouette
      </label>
      <p className="hint">
        {artifact.surfaces.some((s) => s.mesh.shoots)
          ? `${artifact.surfaces.reduce((n, s) => n + (s.mesh.shoots?.sourceIds.length ?? 0), 0).toLocaleString()} leafy shoots. Close needles and distant silhouettes use different detail.`
          : `${artifact.surfaces.reduce((n, s) => n + s.mesh.indices.length / 3, 0).toLocaleString()} triangles per plant at interactive quality.`}
      </p>
      {artifact.diagnostics.map((diagnostic) => (
        <p className="hint" key={diagnostic.code}>
          {diagnostic.message}
        </p>
      ))}
    </details>
  );
}

export function VegetationAuthoringPanel({
  document,
  onChange,
  project,
}: DomainPanelProps<VegetationDefinition>) {
  const botanical = document.botanical;
  const [branchId, setBranchId] = useState("b0");
  const branches = useMemo(() => botanicalStructure(document).branches, [document]);
  const set = (path: (string | number)[], value: unknown, label?: string) =>
    onChange(["botanical", ...path], value, label);
  const number = (
    label: string,
    path: (string | number)[],
    value: number,
    min: number,
    max: number,
    step = 0.05,
  ) => (
    <label className="row-control" key={path.join(".")}>
      <span>{label}</span>
      <input
        aria-label={label}
        type="number"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => {
          const next = Number(event.target.value);
          if (Number.isFinite(next))
            set(path, Math.min(max, Math.max(min, step === 1 ? Math.round(next) : next)));
        }}
      />
    </label>
  );
  const edit = botanical?.branchEdits.find((item) => item.branch === branchId);
  const updateEdit = (patch: Partial<BotanicalAuthoring["branchEdits"][number]>) => {
    if (!botanical) return;
    const value = {
      branch: branchId,
      lengthScale: 1,
      bend: [0, 0, 0] as [number, number, number],
      bare: false,
      ...edit,
      ...patch,
    };
    set(
      ["branchEdits"],
      [...botanical.branchEdits.filter((item) => item.branch !== branchId), value],
      "Shape branch",
    );
  };
  if (!botanical)
    return (
      <section>
        <h3>Botanical authoring</h3>
        <p className="hint">
          Enable branching and individual foliage clusters. Existing conifers keep their original geometry
          until enabled.
        </p>
        <button
          type="button"
          onClick={() => onChange(["botanical"], botanicalPreset(), "Enable botanical growth")}
        >
          Enable botanical growth
        </button>
      </section>
    );
  return (
    <section>
      <h3>Botanical authoring</h3>
      {botanical.species === "pine" && (!botanical.conifer?.architecture || botanical.development) && (
        <div>
          <p className="hint">
            Shape the crown, supporting branches, and leafy twigs directly. Replaces the branch structure and
            branch edits; undo restores the previous tree.
          </p>
          <button
            type="button"
            onClick={() => {
              const next = structuredClone(botanical);
              delete next.development;
              next.conifer = shapedPineLookdevDefinition().botanical?.conifer;
              next.growth.levels = 3;
              next.branchEdits = [];
              next.pruning.removedBranches = [];
              onChange(["botanical"], next, "Use shaped pine architecture");
            }}
          >
            Use shaped pine
          </button>
        </div>
      )}
      <VegetationGrowthPanel document={document} onChange={onChange} />
      <label className="row-control">
        <span>Bark / stem material</span>
        <select
          aria-label="Bark or stem material"
          value={document.trunkMaterial}
          onChange={(event) => onChange(["trunkMaterial"], event.target.value, "Set stem material")}
        >
          {project.documents
            .filter((item) => item.kind === "material")
            .map((material) => (
              <option key={material.id} value={material.id}>
                {material.name}
              </option>
            ))}
        </select>
      </label>
      <label className="row-control">
        <span>Species</span>
        <select
          aria-label="Botanical species"
          value={botanical.species}
          onChange={(event) =>
            onChange(
              ["botanical"],
              {
                ...botanicalPreset(event.target.value as BotanicalSpecies),
                age: botanical.age,
                review: botanical.review,
              },
              "Apply species growth",
            )
          }
        >
          {(["pine", "oak", "birch", "shrub", "fern", "grass"] as const).map((species) => (
            <option key={species}>{species}</option>
          ))}
        </select>
      </label>
      {number("Maturity", ["age"], botanical.age, 0, 1)}
      <details open>
        <summary>Branch hierarchy</summary>
        {botanical.conifer ? (
          <p className="hint">
            Whorled main limbs and alternating lateral shoots. Use conifer growth controls to shape this
            specialized hierarchy.
          </p>
        ) : (
          <>
            {number("Branch levels", ["growth", "levels"], botanical.growth.levels, 1, 3, 1)}
            {number("Child branches", ["growth", "children"], botanical.growth.children, 1, 4, 1)}
            {number("Branch angle (rad)", ["growth", "branchAngle"], botanical.growth.branchAngle, 0.1, 1.5)}
            {number("Child length ratio", ["growth", "lengthRatio"], botanical.growth.lengthRatio, 0.15, 0.8)}
            {number("Branch taper", ["growth", "taper"], botanical.growth.taper, 0.15, 0.8)}
            {number("Growth toward light", ["growth", "tropism"], botanical.growth.tropism, -1, 1)}
          </>
        )}
        {number("Asymmetry", ["growth", "asymmetry"], botanical.growth.asymmetry, 0, 1)}
      </details>
      {botanical.species === "pine" && !botanical.conifer && (
        <button
          type="button"
          onClick={() =>
            onChange(["botanical", "conifer"], alpineConiferSchema.parse({}), "Enable conifer growth")
          }
        >
          Enable conifer growth
        </button>
      )}
      {botanical.conifer && (
        <details open>
          <summary>Conifer growth</summary>
          {number("Branches per whorl", ["conifer", "whorlSize"], botanical.conifer.whorlSize, 2, 6, 1)}
          {number("Whorl irregularity", ["conifer", "whorlJitter"], botanical.conifer.whorlJitter, 0, 1)}
          {number("Crown start", ["conifer", "crownStart"], botanical.conifer.crownStart, 0.04, 0.5)}
          {number("Crown height bias", ["conifer", "crownBias"], botanical.conifer.crownBias, 0.6, 1.5)}
          {number("Crown taper", ["conifer", "crownPower"], botanical.conifer.crownPower, 0.3, 1.8)}
          {number("Limb sag", ["conifer", "limbSag"], botanical.conifer.limbSag, 0, 1)}
          {number("Trunk radius (m)", ["conifer", "trunkRadius"], botanical.conifer.trunkRadius, 0.025, 0.6)}
          {number("Trunk flare", ["conifer", "trunkFlare"], botanical.conifer.trunkFlare, 0, 0.6)}
          {number("Shoots per limb", ["conifer", "shootsPerLimb"], botanical.conifer.shootsPerLimb, 4, 20, 1)}
          {number("Shoot spread", ["conifer", "shootSpread"], botanical.conifer.shootSpread, 0.2, 1)}
          {botanical.conifer.architecture && (
            <>
              <p className="hint">
                Maturity extends the trunk and branches through a controlled shape. It is not a calendar age
                or an ecological simulation.
              </p>
              <label>
                <input
                  type="checkbox"
                  checked={botanical.conifer.architecture.sharedShoots}
                  onChange={(event) =>
                    set(
                      ["conifer", "architecture", "sharedShoots"],
                      event.target.checked,
                      "Change close needle detail",
                    )
                  }
                />{" "}
                Detailed needles nearby
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={botanical.conifer.architecture.canopyVisibility}
                  onChange={(event) =>
                    set(
                      ["conifer", "architecture", "canopyVisibility"],
                      event.target.checked,
                      "Change local canopy shade",
                    )
                  }
                />{" "}
                Local canopy shade
              </label>
              <p className="hint">
                Distant shoot silhouettes remain an approximation. Curved twig edits retain their authored
                shape.
              </p>
              {number(
                "Twig pairs",
                ["conifer", "architecture", "twigPairs"],
                botanical.conifer.architecture.twigPairs,
                1,
                4,
                1,
              )}
              {number(
                "Twig length (m)",
                ["conifer", "architecture", "twigLength"],
                botanical.conifer.architecture.twigLength,
                0.08,
                0.6,
                0.01,
              )}
              {number(
                "Limb thickness",
                ["conifer", "architecture", "limbThickness"],
                botanical.conifer.architecture.limbThickness,
                0.15,
                0.5,
              )}
              {number(
                "Leafy twig start",
                ["conifer", "architecture", "foliageStart"],
                botanical.conifer.architecture.foliageStart,
                0.1,
                0.75,
              )}
            </>
          )}
          {number(
            "Needles per shoot",
            ["conifer", "needlesPerShoot"],
            botanical.conifer.needlesPerShoot,
            16,
            160,
            1,
          )}
          {number(
            "Needle length (m)",
            ["conifer", "needleLength"],
            botanical.conifer.needleLength,
            0.02,
            0.12,
            0.005,
          )}
          {number(
            "Needle width (m)",
            ["conifer", "needleWidth"],
            botanical.conifer.needleWidth,
            0.0008,
            0.006,
            0.0002,
          )}
        </details>
      )}
      <details open>
        <summary>Canopy and clusters</summary>
        {number("Canopy density", ["canopy", "density"], botanical.canopy.density, 0, 1)}
        {!botanical.conifer && (
          <>
            {number("Clusters per tip", ["canopy", "clusters"], botanical.canopy.clusters, 1, 8, 1)}
            {number(
              "Leaves per cluster",
              ["canopy", "leavesPerCluster"],
              botanical.canopy.leavesPerCluster,
              2,
              12,
              1,
            )}
            {number(
              "Leaf length (m)",
              ["canopy", "leafLength"],
              botanical.canopy.leafLength,
              0.01,
              1.5,
              0.01,
            )}
            {number("Leaf width (m)", ["canopy", "leafWidth"], botanical.canopy.leafWidth, 0.003, 0.8, 0.005)}
            {number("Leaf droop", ["canopy", "droop"], botanical.canopy.droop, 0, 1)}
          </>
        )}
      </details>
      <details>
        <summary>Damage and pruning</summary>
        {number("Broken branches", ["damage", "brokenBranches"], botanical.damage.brokenBranches, 0, 1)}
        {number("Leaf loss", ["damage", "leafLoss"], botanical.damage.leafLoss, 0, 1)}
        {number("Clear trunk fraction", ["pruning", "clearTrunk"], botanical.pruning.clearTrunk, 0, 1)}
        {number("Pruned radius (m)", ["pruning", "radiusLimit"], botanical.pruning.radiusLimit, 0.1, 20, 0.1)}
        <label className="row-control">
          <span>Branch</span>
          <select
            value={branchId}
            aria-label="Selected botanical branch"
            onChange={(event) => setBranchId(event.target.value)}
          >
            {branches
              .filter((branch) => branch.level > 0)
              .map((branch) => (
                <option key={branch.id} value={branch.id}>
                  {branch.id}
                  {branch.broken ? " (broken)" : ""}
                </option>
              ))}
          </select>
        </label>
        <label className="row-control">
          <span>Branch length</span>
          <input
            type="number"
            min={0.05}
            max={2}
            step={0.05}
            value={edit?.lengthScale ?? 1}
            onChange={(event) =>
              updateEdit({ lengthScale: Math.max(0.05, Math.min(2, Number(event.target.value))) })
            }
          />
        </label>
        <label className="row-control">
          <span>Branch bend</span>
          <input
            type="range"
            min={-1}
            max={1}
            step={0.05}
            value={edit?.bend[1] ?? 0}
            onChange={(event) => updateEdit({ bend: [0, Number(event.target.value), 0] })}
          />
        </label>
        <label>
          <input
            type="checkbox"
            checked={edit?.bare ?? false}
            onChange={(event) => updateEdit({ bare: event.target.checked })}
          />
          Bare branch
        </label>
        <button
          type="button"
          disabled={
            !branches.some((branch) => branch.id === branchId) ||
            botanical.pruning.removedBranches.length >= 256
          }
          onClick={() =>
            set(
              ["pruning", "removedBranches"],
              [...botanical.pruning.removedBranches, branchId],
              "Prune branch and descendants",
            )
          }
        >
          Prune selected branch
        </button>
        {botanical.pruning.removedBranches.length > 0 && (
          <button
            type="button"
            onClick={() => set(["pruning", "removedBranches"], [], "Restore pruned branches")}
          >
            Restore pruned branches
          </button>
        )}
      </details>
      {number("Stem stiffness", ["motion", "stiffness"], botanical.motion.stiffness, 0, 1)}
      {number("Branch sway", ["motion", "branchSway"], botanical.motion.branchSway, 0, 1)}
      {number("Leaf flutter", ["motion", "leafFlutter"], botanical.motion.leafFlutter, 0, 1)}
      {number("Review stand count", ["review", "standCount"], botanical.review.standCount, 1, 25, 1)}
      {number("Review spacing (m)", ["review", "spacing"], botanical.review.spacing, 0.1, 30, 0.1)}
      {number("Stand seed variation", ["review", "seedVariation"], botanical.review.seedVariation, 0, 1)}
      {number("Stand age variation", ["review", "ageVariation"], botanical.review.ageVariation, 0, 1)}
      <PlantReview document={document} />
    </section>
  );
}
