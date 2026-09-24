import { referenceProject } from "@wrela/examples";
import type { Vec3 } from "@wrela/model";

export const WINTER_ACCEPTANCE_RECIPE = "winter-brook-trail-2";
/** Acceptance uses the same authored scene as the studio and game. */
export const winterAcceptanceProject = referenceProject;
export const WINTER_TRAJECTORY: { time: number; position: Vec3; target: Vec3; sun: Vec3 }[] = [
  { time: 0, position: [8, 4.2, 11], target: [0.8, 1, 0.5], sun: [0.4, 0.2, 0.8] },
  { time: 0.5, position: [7, 3.8, 10], target: [0.7, 1, 0.2], sun: [0.45, 0.19, 0.78] },
  { time: 1, position: [6, 3.3, 9], target: [0.5, 1, 0], sun: [0.5, 0.18, 0.75] },
  { time: 1.5, position: [5, 3, 8], target: [0.3, 1, 0], sun: [0.55, 0.17, 0.72] },
];
