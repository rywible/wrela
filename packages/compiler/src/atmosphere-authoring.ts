import { contentKey } from "@wrela/model";

import {
  authoredAtmosphereComposition,
  compileScatteringAtmosphereTable,
  type ScatteringAtmosphereTable,
} from "./atmosphere";

let authoredBasis: ScatteringAtmosphereTable | undefined;
/** The table cells contain density integrals, not extinction. A composition-independent
 * uniform transmission budget allows one basis to serve every supported weather state.
 * Float32/interpolation error and any exhausted cells retain the existing honest status. */
export function compileAuthoredAtmosphere(turbidity = 2, fogDensity = 0.003): ScatteringAtmosphereTable {
  const composition = authoredAtmosphereComposition(turbidity, fogDensity);
  if (!authoredBasis) {
    const result = compileScatteringAtmosphereTable(authoredAtmosphereComposition(10, 0.1), {
      transmissionBudget: 1e-4,
      extinctionUpperBound: 530,
      maxSegments: 256,
    });
    if ("status" in result) throw new Error("Atmosphere basis compilation unexpectedly cancelled");
    authoredBasis = result;
  }
  const data = authoredBasis.data.slice();
  data[7] = composition.aerosolExtinction * composition.aerosolAlbedo;
  data[8] = composition.aerosolExtinction;
  data[9] = composition.mistExtinction;
  return {
    ...authoredBasis,
    key: contentKey({ basis: authoredBasis.key, turbidity, fogDensity }),
    data,
    // The basis certifies the whole extinction interval, including every rebound header.
    maximumTransmissionWidth: authoredBasis.maximumTransmissionWidth,
  };
}
