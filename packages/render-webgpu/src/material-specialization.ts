import type { EvaluatedScene, RenderSurface } from "@wrela/model";

export type MaterialSpecialization = "plain" | "foliage" | "bark";
/** Exact partial evaluation of material features whose source proves them absent.
 * Any unsupported authoring edit immediately selects the complete shader again. */
export function materialSpecialization(surface: RenderSurface): MaterialSpecialization | undefined {
  if (
    surface.water ||
    surface.reliefAppearance ||
    surface.creatureInspection ||
    surface.skin ||
    surface.deformation ||
    surface.material.layers?.length
  )
    return undefined;
  const { appearance: a, creature: c } = surface.material;
  if (!a)
    return !c && surface.material.pattern === 0 && surface.material.normalStrength === 0
      ? "plain"
      : undefined;
  if (a.weathering || a.wetness || a.dirt || a.damage || a.layers.length || c?.frame || c?.clearcoat)
    return undefined;
  if (a.family === "foliage" && a.detail.kind === "none" && c?.family === "skin") return "foliage";
  if (a.family === "generic" && a.detail.kind === "bark" && (!c || c.family === "hard")) return "bark";
  return undefined;
}
export function specializeMaterialShader(
  shader: string,
  profile: MaterialSpecialization,
  daylight = false,
): string {
  const facts: Record<string, number> = {
    "obj.coordinates.y": 0,
    "obj.creatureCoat.x": 0,
    "obj.creatureCoat.z": 0,
    "obj.relief.x": 0,
    "obj.surface.x": profile === "plain" ? 0 : profile === "foliage" ? 3 : 1,
    "obj.creature.x": profile === "foliage" ? 1 : 0,
    "obj.surface.y": 0,
    "obj.surface.z": 0,
    "obj.surface.w": 0,
    "obj.surfaceHistory.x": 0,
    "obj.surfaceDirt.w": 0,
    "obj.surfaceDetail.x": profile === "bark" ? 2 : 0,
  };
  if (profile === "plain") {
    facts["obj.material.x"] = 0;
    facts["obj.material.z"] = 0;
  }
  if (daylight) {
    facts["g.moon.w"] = 0;
    facts["g.viewport.z"] = 0;
    facts["g.renderCompiler.x"] = 0;
    shader = shader.replace(
      "fn compiledIndirectDiffuse(world:vec3f,n:vec3f)->vec4f {",
      "fn compiledIndirectDiffuse(world:vec3f,n:vec3f)->vec4f { return vec4f(0.0);",
    );
    shader = shader.replace(
      "fn physicalCloudShadowCached(world:vec3f,ray:vec3f)->f32 {",
      "fn physicalCloudShadowCached(world:vec3f,ray:vec3f)->f32 { return 0.0;",
    );
  }
  // The irradiance basis has exactly nine coefficients. Preserve summation
  // order while making each index constant, eliminating the per-sample switch.
  shader = shader.replace(
    "for(var i=0u;i<9u;i++){value+=skyIrradiance[i].xyz*irradianceBasis(normal,i);}",
    Array.from(
      { length: 9 },
      (_, i) => `value+=skyIrradiance[${i}u].xyz*irradianceBasis(normal,${i}u);`,
    ).join("\n"),
  );
  for (const [field, value] of Object.entries(facts))
    shader = shader.replaceAll(new RegExp(`\\b${field.replaceAll(".", "\\.")}\\b`, "g"), `${value}.0`);
  return shader;
}

export function simpleDaylightScene(scene: EvaluatedScene, finiteSun: boolean): boolean {
  return (
    !scene.indirectLighting &&
    !finiteSun &&
    !scene.environment.pointLights?.length &&
    !(scene.environment.moonIntensity ?? 0) &&
    !(scene.environment.cloudCover ?? 0)
  );
}

/** The fully admitted flat path interpolates low-frequency local radiance.
 * Glossy materials retain their sky reflection and full BRDF per pixel.
 * Keep the general path during transitions and after any material/geometry edit. */
export function cachedSurfaceRadiance(surface: RenderSurface): boolean {
  return (
    materialSpecialization(surface) === "plain" &&
    !!surface.mesh.radianceFlatNormals &&
    !!surface.mesh.radianceProbes &&
    surface.radianceWeight === 1 &&
    !surface.wind &&
    !surface.mesh.wind &&
    !surface.mesh.shoots &&
    !surface.material.creature &&
    (!surface.selectedRenderProduct || surface.selectedRenderProduct.kind === "direct-mesh")
  );
}
