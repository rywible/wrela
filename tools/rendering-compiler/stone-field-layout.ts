import { type EvaluatedScene, type RenderSurface, transformMatrix } from "@wrela/model";

export const STONE_COMPILE_QUALITY = "review";
export function stoneFieldCamera(side: number) {
  return {
    position: [0, side * 3.8, side * 0.08] as [number, number, number],
    target: [0, 0, 0] as [number, number, number],
    fov: 45,
  };
}
export function stoneFieldScene(base: EvaluatedScene, stone: RenderSurface, count: number): EvaluatedScene {
  const side = Math.sqrt(count);
  const camera = stoneFieldCamera(side);
  const scene: EvaluatedScene = {
    ...base,
    camera,
    time: 0,
    mode: "beauty",
    grid: false,
    shadowRadius: side * 2,
    surfaces: Array.from({ length: count }, (_, i) => ({
      ...stone,
      id: `stone-${i}`,
      instanceId: `stone-${i}`,
      matrix: transformMatrix([
        ((i % side) - (side - 1) / 2) * 2.5,
        0,
        (Math.floor(i / side) - (side - 1) / 2) * 2.5,
      ]),
    })),
  };
  return scene;
}
