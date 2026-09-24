import {
  type CreatureDefinition,
  type CreatureLandmark,
  creaturePatchHasConsistentOrientation,
  type Vec3,
} from "@wrela/model";

/** A landmark transaction updates its source dependents, so compilation, save,
 * undo, candidates and runtime all observe one explicit, coherent source. */
export function updateCreatureLandmarkBindings(
  source: CreatureDefinition,
  landmark: CreatureLandmark,
  previous?: CreatureLandmark,
) {
  const anchors = source.anchors.filter((anchor) => anchor.landmark === landmark.id);
  const garments = source.cloth.filter((cloth) => cloth.fittingLandmarks?.includes(landmark.id));
  if (previous && previous.region !== landmark.region && (anchors.length || garments.length))
    throw Error(
      "A bound landmark cannot change regions; detach its fittings before moving it to new anatomy",
    );
  for (const anchor of anchors) anchor.coordinates = [...landmark.position];
  for (const cloth of garments) {
    const chart = source.charts.find((candidate) => candidate.id === cloth.chart);
    if (!chart || chart.kind !== "patch" || !cloth.fittingLandmarks)
      throw Error(`Garment ${cloth.id} lost its fitting patch`);
    const points = cloth.fittingLandmarks.map((id) => {
      const corner = source.landmarks.find(
        (candidate) => candidate.id === id && candidate.region === cloth.region,
      );
      if (!corner) throw Error(`Garment ${cloth.id} requires fitting landmark ${id} in ${cloth.region}`);
      return [...corner.position] as Vec3;
    }) as [Vec3, Vec3, Vec3, Vec3];
    if (!creaturePatchHasConsistentOrientation(points, chart.points))
      throw Error(
        `Landmark edit would fold, collapse or reverse garment ${cloth.id}; preserve its corner order`,
      );
    // Stable corner identities retain normalized cloth/groom/anchor correspondence.
    chart.points = points;
  }
}
