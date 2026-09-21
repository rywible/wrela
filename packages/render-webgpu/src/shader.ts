import display from "./display.wgsl" with { type: "text" };
import source from "./scene.wgsl" with { type: "text" };
export const shader = source;
export const displayShader = display;
