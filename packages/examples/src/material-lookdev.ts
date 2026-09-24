import type { MaterialDefinition } from "@wrela/model";
import { alpineConiferMaterials, createSurfaceAppearance, createSurfaceLayer } from "@wrela/model";

const material = (
  id: string,
  name: string,
  values: Pick<MaterialDefinition, "color" | "secondary"> & Partial<MaterialDefinition>,
): MaterialDefinition => ({
  id: `alpine-lookdev-${id}`,
  name,
  schemaVersion: 1,
  dependencies: [],
  kind: "material",
  roughness: 0.9,
  metallic: 0,
  pattern: "noise",
  scale: 2,
  normalStrength: 0,
  domain: "world",
  ...values,
});
/** Shared art-direction palette for an alpine creek, grove and weathered limestone ruin.
 * Values are linear RGB. Noise stays low contrast; silhouette, fractures and grain-scale
 * structure belong to authored geometry. All presets remain editable source documents. */
export function createLookdevMaterials(): MaterialDefinition[] {
  const conifer = alpineConiferMaterials();
  const groundAppearance = createSurfaceAppearance();
  const bank = createSurfaceLayer("damp-bank");
  bank.name = "Damp creek bank";
  bank.color = [0.037, 0.029, 0.018];
  bank.roughness = 0.72;
  bank.coverage = 0.85;
  bank.mask = { ...bank.mask, kind: "height", minimumHeight: -20, maximumHeight: 0.05, softness: 0.3 };
  const slope = createSurfaceLayer("exposed-slopes");
  slope.name = "Exposed stone on steep slopes";
  slope.color = [0.135, 0.131, 0.108];
  slope.roughness = 0.93;
  slope.mask = { ...slope.mask, kind: "slope", threshold: 0.65, softness: 0.15, invert: true };
  const wornSoil = createSurfaceLayer("worn-soil");
  wornSoil.name = "Broken turf and earth";
  wornSoil.color = [0.095, 0.076, 0.049];
  wornSoil.coverage = 0.45;
  wornSoil.roughness = 0.97;
  wornSoil.mask = { ...wornSoil.mask, kind: "noise", scale: 0.7, threshold: 0.57, softness: 0.08 };
  groundAppearance.layers = [wornSoil, slope, bank];
  groundAppearance.detail = { kind: "soil", scale: 1, strength: 1 };
  const stoneAppearance = createSurfaceAppearance();
  stoneAppearance.detail = { kind: "mineral", scale: 1, strength: 0.85 };
  const lichen = createSurfaceLayer("old-lichen");
  lichen.name = "Sparse weathered lichen";
  lichen.color = [0.14, 0.17, 0.087];
  lichen.coverage = 0.08;
  lichen.roughness = 0.97;
  lichen.mask = { ...lichen.mask, kind: "noise", scale: 3.5, threshold: 0.64, softness: 0.07 };
  stoneAppearance.layers = [lichen];
  stoneAppearance.weathering = 0.025;
  const wetStone = createSurfaceAppearance();
  wetStone.wetness = 0.55;
  wetStone.detail = { kind: "mineral", scale: 1, strength: 0.75 };
  const iron = createSurfaceAppearance("metal");
  const rust = createSurfaceLayer("iron-oxidation");
  rust.name = "Old iron oxidation";
  rust.color = [0.12, 0.057, 0.029];
  rust.coverage = 0.14;
  rust.roughness = 0.93;
  rust.mask = { ...rust.mask, kind: "noise", scale: 9, threshold: 0.58, softness: 0.18 };
  iron.layers = [rust];
  const bronze = createSurfaceAppearance("metal");
  const patina = createSurfaceLayer("bronze-patina");
  patina.name = "Dull bronze patina";
  patina.color = [0.069, 0.094, 0.067];
  patina.coverage = 0.16;
  patina.roughness = 0.92;
  patina.mask = { ...patina.mask, kind: "noise", scale: 8, threshold: 0.57, softness: 0.18 };
  bronze.layers = [patina];
  const wood = createSurfaceAppearance();
  wood.detail = { kind: "wood", scale: 1, strength: 0.85 };
  const silvered = createSurfaceLayer("silvered-wood");
  silvered.name = "Sun-bleached exposed wood";
  silvered.color = [0.16, 0.145, 0.112];
  silvered.coverage = 0.38;
  silvered.roughness = 0.95;
  silvered.mask = {
    ...silvered.mask,
    kind: "combined",
    scale: 2.1,
    threshold: 0.57,
    softness: 0.15,
    slopeInfluence: 0.3,
    heightInfluence: 0,
  };
  wood.layers = [silvered];
  const masonry = structuredClone(stoneAppearance);
  masonry.detail = { kind: "mineral", scale: 2.2, strength: 1 };
  const deposits = createSurfaceLayer("masonry-deposits");
  deposits.name = "Broken mineral deposits on exposed stone";
  deposits.color = [0.28, 0.25, 0.18];
  deposits.coverage = 0.22;
  deposits.relief = 0.0003;
  deposits.mask = {
    ...deposits.mask,
    kind: "combined",
    scale: 2.7,
    threshold: 0.61,
    softness: 0.25,
    slopeInfluence: 0.4,
    heightInfluence: 0,
  };
  const dampFoot = createSurfaceLayer("masonry-damp-foot");
  dampFoot.name = "Moisture and moss near the footing";
  dampFoot.color = [0.045, 0.056, 0.027];
  dampFoot.coverage = 0.68;
  dampFoot.roughness = 0.76;
  dampFoot.mask = {
    ...dampFoot.mask,
    kind: "combined",
    scale: 4,
    threshold: 0.45,
    softness: 0.17,
    slopeInfluence: 0.15,
    minimumHeight: -2,
    maximumHeight: 0.6,
  };
  masonry.layers = [...masonry.layers, deposits, dampFoot];
  return [
    conifer.needles,
    conifer.bark,
    material("ground", "Alpine moss and earth", {
      color: [0.07, 0.087, 0.045],
      secondary: [0.135, 0.145, 0.076],
      scale: 0.8,
      normalStrength: 0,
      roughness: 0.96,
      appearance: groundAppearance,
    }),
    material("rock", "Weathered alpine limestone", {
      color: [0.105, 0.107, 0.097],
      secondary: [0.16, 0.16, 0.138],
      scale: 1.8,
      normalStrength: 0,
      roughness: 0.92,
      appearance: stoneAppearance,
    }),
    material("rock-dark", "Damp creek limestone", {
      color: [0.085, 0.091, 0.078],
      secondary: [0.13, 0.139, 0.12],
      scale: 2,
      normalStrength: 0,
      roughness: 0.92,
      appearance: wetStone,
    }),
    material("snow", "Sheltered alpine snow", {
      color: [0.66, 0.73, 0.77],
      secondary: [0.8, 0.83, 0.84],
      scale: 18,
      normalStrength: 0.04,
      roughness: 0.86,
    }),
    material("wood", "Weathered mountain oak", {
      appearance: wood,
      color: [0.037, 0.02, 0.01],
      secondary: [0.18, 0.114, 0.061],
      domain: "local",
      scale: 8,
      normalStrength: 0,
      roughness: 0.9,
    }),
    material("iron", "Aged wrought iron", {
      color: [0.12, 0.123, 0.119],
      secondary: [0.155, 0.155, 0.146],
      domain: "local",
      scale: 12,
      normalStrength: 0.065,
      roughness: 0.76,
      appearance: iron,
    }),
    material("bronze", "Patinated bronze", {
      color: [0.22, 0.13, 0.052],
      secondary: [0.27, 0.17, 0.068],
      domain: "local",
      scale: 10,
      normalStrength: 0.055,
      roughness: 0.78,
      appearance: bronze,
    }),
    material("masonry", "Old warm limestone masonry", {
      color: [0.102, 0.1, 0.087],
      secondary: [0.244, 0.229, 0.191],
      scale: 2.2,
      normalStrength: 0,
      roughness: 0.94,
      appearance: masonry,
    }),
    material("understory", "Alpine sedge and low shrubs", {
      color: [0.105, 0.135, 0.062],
      secondary: [0.17, 0.18, 0.082],
      domain: "local",
      pattern: "noise",
      scale: 2.8,
      roughness: 0.91,
      appearance: createSurfaceAppearance("foliage"),
    }),
    material("understory-dry", "Weathered grass and needle litter", {
      color: [0.18, 0.145, 0.069],
      secondary: [0.235, 0.185, 0.093],
      domain: "local",
      pattern: "noise",
      scale: 3.5,
      roughness: 0.94,
      appearance: createSurfaceAppearance("foliage"),
    }),
  ];
}

/** Six substances for common lookdev evidence. The same source recipes are editable in Studio. */
export function createSubstanceLookdevMaterials(weathered = false): MaterialDefinition[] {
  const stone = createSurfaceAppearance();
  stone.detail = { kind: "mineral", scale: 1.6, strength: 0.75 };
  const skin = createSurfaceAppearance("skin");
  skin.transmission = 0.28;
  skin.response = {
    subsurface: 0.22,
    thickness: 0.004,
    scatterColor: [1, 0.54, 0.4],
    clearcoat: 0.08,
    clearcoatRoughness: 0.3,
  };
  const foliage = createSurfaceAppearance("foliage");
  foliage.transmission = 0.45;
  foliage.response = { scatterColor: [0.55, 0.85, 0.3], thickness: 0.0006 };
  const fabric = createSurfaceAppearance("fabric");
  fabric.response = { sheen: 0.45, anisotropy: 0.4 };
  const metal = createSurfaceAppearance("metal");
  const glass = createSurfaceAppearance("glass");
  glass.transmission = 0.92;
  glass.indexOfRefraction = 1.52;
  const materials = [
    material("study-stone", "Honed limestone", {
      color: [0.23, 0.22, 0.18],
      secondary: [0.34, 0.32, 0.26],
      roughness: 0.72,
      appearance: stone,
    }),
    material("study-skin", "Warm skin", {
      color: [0.32, 0.16, 0.11],
      secondary: [0.36, 0.19, 0.14],
      roughness: 0.46,
      scale: 25,
      appearance: skin,
    }),
    material("study-foliage", "Broadleaf green", {
      color: [0.035, 0.12, 0.027],
      secondary: [0.065, 0.18, 0.037],
      roughness: 0.57,
      appearance: foliage,
    }),
    material("study-fabric", "Woven indigo", {
      color: [0.025, 0.055, 0.12],
      secondary: [0.055, 0.095, 0.18],
      pattern: "weave",
      scale: 95,
      roughness: 0.69,
      appearance: fabric,
    }),
    material("study-metal", "Brushed bronze", {
      color: [0.42, 0.24, 0.09],
      secondary: [0.47, 0.29, 0.12],
      roughness: 0.3,
      appearance: metal,
    }),
    material("study-glass", "Sea-green glass", {
      color: [0.3, 0.67, 0.52],
      secondary: [0.3, 0.67, 0.52],
      pattern: "solid",
      roughness: 0.08,
      appearance: glass,
    }),
  ];
  for (const source of materials) {
    source.domain = "local";
    if (!weathered || !source.appearance) continue;
    const coating = createSurfaceLayer("deposited-dust");
    coating.name = "Dust on upper exposed surfaces";
    coating.color = [0.19, 0.15, 0.1];
    coating.coverage = 0.5;
    coating.relief = 0.0003;
    coating.mask = {
      ...coating.mask,
      kind: "combined",
      scale: 5.5,
      threshold: 0.38,
      softness: 0.38,
      slopeThreshold: 0.15,
      slopeSoftness: 0.45,
      heightInfluence: 0.4,
      minimumHeight: -0.1,
      maximumHeight: 1,
    };
    source.appearance.layers = [coating];
    source.appearance.weathering = 0.12;
    source.appearance.damage = 0.04;
  }
  return materials;
}
