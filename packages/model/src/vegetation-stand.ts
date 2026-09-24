import type { VegetationDefinition, WorldDefinition } from "./documents";
import { contentKey, random01, type Vec3 } from "./math";

export type VegetationStand = {
  documents: VegetationDefinition[];
  instances: WorldDefinition["instances"];
};

/** A bounded, deterministic review population uses source variants, not scaled
 * copies alone. It can be installed in a world or inspected in Studio unchanged. */
export function createVegetationStand(
  source: VegetationDefinition,
  origin: Vec3 = [0, 0, 0],
): VegetationStand {
  const review = source.botanical?.review;
  const prefix =
    source.id.length <= 65 ? source.id : `${source.id.slice(0, 53)}-${contentKey(source.id).slice(-8)}`;
  const count = review?.standCount ?? 9;
  const variants = Math.min(5, count);
  const columns = Math.ceil(Math.sqrt(count));
  const rows = Math.ceil(count / columns);
  const spacing = review?.spacing ?? source.radius * 2;
  const sample = (index: number, stream: number) => random01(source.seed + index * 7919 + stream * 104729);
  const documents = Array.from({ length: variants }, (_, index) => {
    const document = structuredClone(source);
    document.id = `${prefix}-stand-${index}`;
    document.name = `${source.name} · stand variant ${index + 1}`;
    document.seed = source.seed + Math.round(index * 7919 * (review?.seedVariation ?? 0.6));
    if (document.botanical)
      document.botanical.age = Math.max(
        0,
        (source.botanical?.age ?? 1) - (review?.ageVariation ?? 0.25) * sample(index, 1),
      );
    if (document.botanical?.development) {
      document.botanical.development.steps = Math.max(
        1,
        Math.round(
          document.botanical.development.steps * (1 - (review?.ageVariation ?? 0.25) * sample(index, 1)),
        ),
      );
    }
    return document;
  });
  const instances = Array.from({ length: count }, (_, index) => ({
    id: `${prefix}-stand-instance-${index}`,
    definition: documents[index % variants].id,
    position: [
      origin[0] + ((index % columns) - (columns - 1) / 2 + (sample(index, 2) - 0.5) * 0.35) * spacing,
      origin[1],
      origin[2] + (Math.floor(index / columns) - (rows - 1) / 2 + (sample(index, 3) - 0.5) * 0.35) * spacing,
    ] as Vec3,
    rotation: [0, sample(index, 4) * Math.PI * 2, 0] as Vec3,
    scale: 0.88 + sample(index, 5) * 0.24,
  }));
  return { documents, instances };
}
