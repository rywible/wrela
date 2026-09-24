import { createLookdevMaterials, createSubstanceLookdevMaterials, referenceProject } from "@wrela/examples";
import { type EvaluatedScene, identityMatrix, type MeshData } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { evaluateEnvironment, gridMesh, renderMaterial } from "@wrela/runtime";

function sphere(): MeshData {
  const positions: number[] = [],
    normals: number[] = [],
    indices: number[] = [];
  const rings = 48,
    sides = 80;
  for (let ring = 0; ring <= rings; ring++)
    for (let side = 0; side <= sides; side++) {
      const theta = (ring / rings) * Math.PI,
        phi = (side / sides) * Math.PI * 2;
      const n = [Math.sin(theta) * Math.cos(phi), Math.cos(theta), Math.sin(theta) * Math.sin(phi)];
      normals.push(...n);
      positions.push(...n.map((value) => value * 0.82));
      if (ring < rings && side < sides) {
        const a = ring * (sides + 1) + side,
          b = a + 1,
          c = a + sides + 1;
        indices.push(a, b, c, b, c + 1, c);
      }
    }
  return {
    positions: new Float32Array(positions),
    normals: new Float32Array(normals),
    indices: new Uint32Array(indices),
    bounds: { min: [-0.82, -0.82, -0.82], max: [0.82, 0.82, 0.82] },
  };
}
export async function createMaterialLookdevFixture(families = false) {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing lookdev canvas");
  const materials = families ? createSubstanceLookdevMaterials() : createLookdevMaterials(),
    mesh = sphere(),
    failures: string[] = [];
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    antialiasing: "spatial",
    onDiagnostic: (diagnostic) => {
      if (diagnostic.severity === "error") failures.push(diagnostic.message);
    },
  });
  const project = referenceProject();
  const environment = project.documents.find((document) => document.kind === "environment");
  const lighting = project.documents.find((document) => document.kind === "lighting");
  if (!environment || environment.kind !== "environment" || !lighting || lighting.kind !== "lighting")
    throw Error("Missing review lighting");
  environment.fogDensity = 0;
  environment.sunElevation = 0.7;
  environment.sunAzimuth = -0.7;
  environment.turbidity = 2;
  lighting.ambient = 0.65;
  lighting.lights = [
    { id: "sun", type: "directional", color: [1, 0.96, 0.9], intensity: 3, position: [0, 5, 4] },
  ];
  const columns = families ? 3 : 4;
  const centerHeight = families ? 2.6 : 3.7;
  const scene: EvaluatedScene = {
    surfaces: materials.map((material, index) => {
      const matrix = identityMatrix();
      matrix[12] = ((index % columns) - (columns - 1) / 2) * 2.5;
      matrix[13] =
        centerHeight + ((Math.ceil(materials.length / columns) - 1) / 2 - Math.floor(index / columns)) * 2.5;
      return { id: material.id, source: material.id, mesh, matrix, material: renderMaterial(material) };
    }),
    camera: { position: [0, centerHeight, 14], target: [0, centerHeight, 0], fov: 44 },
    environment: evaluateEnvironment(environment, lighting),
    mode: "beauty",
    time: 0,
    grid: false,
  };
  scene.surfaces.push({
    id: "floor",
    source: "floor",
    mesh: gridMesh(100, 1, 0),
    matrix: identityMatrix(),
    material: {
      color: [0.095, 0.1, 0.098],
      secondary: [0.095, 0.1, 0.098],
      roughness: 0.96,
      metallic: 0,
      scale: 1,
      pattern: 0,
      normalStrength: 0,
    },
  });
  return {
    async capture(
      light: "neutral" | "grazing" | "backlit",
      distance = 14,
      weathered = false,
      mode: EvaluatedScene["mode"] = "beauty",
    ) {
      if (families) {
        const sources = createSubstanceLookdevMaterials(weathered);
        sources.forEach((source, index) => {
          scene.surfaces[index].material = renderMaterial(source);
        });
      }
      scene.mode = mode;
      environment.sunElevation = light === "grazing" ? 0.14 : light === "backlit" ? 0.35 : 0.7;
      environment.sunAzimuth = light === "backlit" ? Math.PI : -0.7;
      lighting.ambient = light === "neutral" ? 0.65 : 0.3;
      scene.environment = evaluateEnvironment(environment, lighting);
      scene.camera.position[2] = distance;
      renderer.render(scene);
      const blob = await renderer.capture();
      const image = await new Promise<string>((resolve) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.readAsDataURL(blob);
      });
      return {
        image,
        light,
        distance,
        weathered,
        mode,
        materials: materials.map(({ id, color, roughness, appearance }) => ({
          id,
          color,
          roughness,
          family: appearance?.family ?? "generic",
        })),
        failures,
        adapter: renderer.measurements.adapter,
        completeness: renderer.completeness,
      };
    },
    dispose() {
      renderer.dispose();
    },
  };
}
