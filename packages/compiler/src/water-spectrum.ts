import { type CompiledWaterSpectrum, contentKey, type WaterDefinition } from "@wrela/model";

const cache = new Map<string, CompiledWaterSpectrum>();
/** A bounded directional spectrum. Analytic carriers retain deterministic CPU/GPU queries. */
export function compileWaterSpectrum(water: WaterDefinition): CompiledWaterSpectrum {
  const key = contentKey({
    version: 5,
    spectrum: water.spectrum,
    bounded: !!water.domain,
    waves: water.waves,
    current: water.flow?.velocity,
  });
  const cached = cache.get(key);
  if (cached) return cached;
  let seed = water.spectrum?.seed ?? 17;
  const random = () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) | 0;
    return (seed >>> 0) / 4294967296;
  };
  const waves = [...water.waves];
  const spectrum = water.spectrum;
  const physical = spectrum?.mode === "sea-state";
  const peakPeriod = spectrum?.peakPeriod ?? Math.max(0.5, spectrum?.windSpeed ?? 6) * 0.78;
  const peakLength = physical ? (9.81 * peakPeriod ** 2) / (2 * Math.PI) : (spectrum?.wavelength ?? 12);
  if (spectrum?.swell) {
    const swell = spectrum.swell;
    for (let i = 0; i < 6; i++) {
      const period = swell.period * (0.9 + i * 0.04);
      const wavelength = (9.81 * period ** 2) / (2 * Math.PI);
      waves.push({
        wavelength,
        amplitude: swell.height / Math.sqrt(48),
        direction: swell.direction + ((i - 2.5) * swell.spread) / 2.5,
        speed: wavelength / period,
        phase: random() * 2 * Math.PI,
      });
    }
  }
  const tiles: [number, number, number] | undefined = spectrum
    ? [peakLength * 8, peakLength, peakLength / 8]
    : undefined;
  if (spectrum) {
    const generated: WaterDefinition["waves"] = [];
    let squared = 0;
    for (let band = 0; band < 9; band++)
      for (let direction = 0; direction < 6; direction++) {
        const jitter = random();
        // Fill each short-wave octave instead of placing six nearly equal
        // wavelengths on a narrow ring. Those rings made regular oval pits in
        // specular reflections. Retain the large-wave frequency/phase choices.
        const scale = band < 3 ? 0.85 + jitter * 0.3 : 2 ** ((direction + jitter) / 6 - 0.3);
        let wavelength = peakLength * 2 ** (1 - band) * scale;
        // Short ripples stay wind-aligned. Giving them the swell's full angular
        // spread produces round cross-wave intersections instead of wavelets.
        const spread = band < 6 ? 1 : 0.45;
        let angle = spectrum.direction + (random() - 0.5) * spectrum.spread * 2 * spread;
        if (tiles) {
          // Integer spatial frequencies make every mip seam periodic, including after rebasing.
          // The same quantized carriers drive geometry, CPU queries and the shading maps.
          const period = tiles[Math.floor(band / 3)];
          const x = Math.round((Math.cos(angle) * period) / wavelength);
          const z = Math.round((Math.sin(angle) * period) / wavelength);
          wavelength = period / Math.hypot(x, z);
          angle = Math.atan2(z, x);
        }
        const k = (2 * Math.PI) / wavelength;
        // Peak swell plus a bounded short-wave tail. Tail amplitude falls with wavelength,
        // keeping slope energy finite instead of giving centimetre waves metre-scale height.
        const omega = Math.sqrt(9.81 * k * Math.tanh(k * (spectrum.depth ?? 1000)));
        const peak = (2 * Math.PI) / peakPeriod;
        const sigma = omega <= peak ? 0.07 : 0.09;
        const enhancement = 3.3 ** Math.exp(-0.5 * ((omega - peak) / (sigma * peak)) ** 2);
        // Log-frequency quadrature of JONSWAP; TMA suppresses shallow long waves.
        const tma = Math.tanh(k * (spectrum.depth ?? 1000)) ** 2;
        const energy = Math.max(
          1e-30,
          omega ** -4 * Math.exp(-1.25 * (peak / omega) ** 4) * enhancement * tma,
        );
        const weight = physical
          ? Math.sqrt(energy)
          : Math.exp(-0.8 * (band - 1) ** 2) + (0.3 * wavelength) / peakLength;
        const amplitude = weight * (0.7 + random() * 0.6);
        squared += amplitude * amplitude;
        generated.push({
          wavelength,
          amplitude,
          direction: angle,
          speed: Math.sqrt((9.81 / k + 0.000074 * k) * Math.tanh(k * (spectrum.depth ?? 1000))),
          phase: random() * Math.PI * 2,
        });
      }
    const energyAmplitude = physical
      ? (spectrum.significantHeight ?? Math.min(12, 0.021 * spectrum.windSpeed ** 2)) / Math.sqrt(8)
      : spectrum.amplitude * Math.sqrt(spectrum.windSpeed / 6);
    for (const wave of generated)
      waves.push({
        ...wave,
        amplitude: (wave.amplitude * energyAmplitude) / Math.sqrt(squared),
      });
  }
  const carriers = new Float32Array(waves.length * 8);
  let amplitudeBound = 0,
    slopeVariance = 0,
    steepness = 0;
  const slopeCovariance: [number, number, number] = [0, 0, 0];
  const bandSlopeVariance: [number, number, number] = [0, 0, 0];
  let maxFrequency = 0;
  waves.forEach((wave, i) => {
    const k = (2 * Math.PI) / wave.wavelength,
      dx = Math.cos(wave.direction),
      dz = Math.sin(wave.direction);
    const velocity = water.flow?.velocity ?? [0, 0];
    const speed = wave.speed + dx * velocity[0] + dz * velocity[1];
    carriers.set([k * dx, k * dz, -k * speed, wave.phase, wave.amplitude, wave.wavelength, dx, dz], i * 8);
    steepness += Math.abs(wave.amplitude * k);
    amplitudeBound += Math.abs(wave.amplitude);
    slopeCovariance[0] += 0.5 * (wave.amplitude * k * dx) ** 2;
    slopeCovariance[1] += 0.5 * wave.amplitude ** 2 * k ** 2 * dx * dz;
    slopeCovariance[2] += 0.5 * (wave.amplitude * k * dz) ** 2;
    maxFrequency = Math.max(maxFrequency, Math.abs(k * speed));
    slopeVariance += 0.5 * (wave.amplitude * k) ** 2;
    const generatedIndex = i - (waves.length - 54);
    if (spectrum && generatedIndex >= 0)
      bandSlopeVariance[Math.floor(generatedIndex / 18)] += 0.5 * (wave.amplitude * k) ** 2;
  });
  const result = {
    key,
    tiles,
    carriers,
    amplitudeBound,
    slopeVariance,
    slopeCovariance,
    bandSlopeVariance,
    realization: {
      method: "carriers" as const,
      mapSize: (water.domain ? 128 : 256) as 128 | 256,
      layers: (water.domain || !spectrum?.choppiness ? 3 : 6) as 3 | 6,
      maxFrequency,
      reason:
        "Bounded analytic gameplay queries; periodic slope moments. FFT adoption requires equal-energy GPU evidence.",
    },
    choppiness: Math.min(water.domain ? 0 : (spectrum?.choppiness ?? 0), 0.65 / Math.max(steepness, 1e-9)),
  };
  if (cache.size >= 32) cache.delete(cache.keys().next().value as string);
  cache.set(key, result);
  return result;
}
export function sampleWaterSpectrum(
  spectrum: CompiledWaterSpectrum,
  x: number,
  z: number,
  time: number,
  spacing = 0,
) {
  let height = 0,
    dx = 0,
    dz = 0,
    velocity = 0,
    lateralX = 0,
    lateralZ = 0,
    velocityX = 0,
    velocityZ = 0,
    jxx = 1,
    jxz = 0,
    jzz = 1;
  const data = spectrum.carriers;
  for (let i = 0; i < data.length; i += 8) {
    const phase = data[i] * x + data[i + 1] * z + data[i + 2] * time + data[i + 3];
    const t = Math.max(0, Math.min(1, (spacing / data[i + 5] - 0.2) / 0.4));
    const amplitude = data[i + 4] * (1 - t * t * (3 - 2 * t));
    const derivative = amplitude * Math.cos(phase);
    lateralX += spectrum.choppiness * data[i + 6] * derivative;
    lateralZ += spectrum.choppiness * data[i + 7] * derivative;
    const stretch = spectrum.choppiness * amplitude * Math.sin(phase);
    velocityX -= stretch * data[i + 6] * data[i + 2];
    velocityZ -= stretch * data[i + 7] * data[i + 2];
    jxx -= stretch * data[i + 6] * data[i];
    jxz -= stretch * data[i + 6] * data[i + 1];
    jzz -= stretch * data[i + 7] * data[i + 1];
    height += amplitude * Math.sin(phase);
    dx += derivative * data[i];
    dz += derivative * data[i + 1];
    velocity += derivative * data[i + 2];
  }
  return { height, dx, dz, velocity, lateralX, lateralZ, velocityX, velocityZ, jxx, jxz, jzz };
}

/** Newton inversion maps a world query back to the same bounded Gerstner surface. */
export function queryWaterSpectrum(spectrum: CompiledWaterSpectrum, x: number, z: number, time: number) {
  let px = x,
    pz = z;
  let wave = sampleWaterSpectrum(spectrum, px, pz, time);
  for (let iteration = 0; iteration < 6 && spectrum.choppiness > 0; iteration++) {
    const ex = px + wave.lateralX - x,
      ez = pz + wave.lateralZ - z;
    if (Math.abs(ex) + Math.abs(ez) < 1e-8) break;
    const det = wave.jxx * wave.jzz - wave.jxz * wave.jxz;
    px -= (wave.jzz * ex - wave.jxz * ez) / det;
    pz -= (wave.jxx * ez - wave.jxz * ex) / det;
    wave = sampleWaterSpectrum(spectrum, px, pz, time);
  }
  const det = wave.jxx * wave.jzz - wave.jxz * wave.jxz;
  return {
    ...wave,
    dx: (wave.dx * wave.jzz - wave.dz * wave.jxz) / det,
    dz: (wave.dz * wave.jxx - wave.dx * wave.jxz) / det,
  };
}
