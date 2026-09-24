import type { CompiledWaterDomain, WaterDefinition } from "@wrela/model";

const gravity = 9.81,
  dry = 1e-7;
export interface WaterSimulationSnapshot {
  state: Float64Array;
  exchangedVolume: number;
  elapsed?: number;
  wetness?: Float32Array;
}
/** Conservative finite-volume SWE with hydrostatic reconstruction and reflecting boundaries.
 * State is h, hu, hv, h*foam. Double precision keeps CPU queries authoritative without GPU readback. */
export class WaterSimulation {
  readonly state: Float64Array;
  private readonly delta: Float64Array;
  private readonly sources: { rate: number; velocity: [number, number]; weights: [number, number][] }[];
  revision = 0;
  exchangedVolume = 0;
  substeps = 0;
  lastStepMs = 0;
  activeTiles = 0;
  private readonly active: Uint8Array;
  private readonly halo: Uint8Array;
  private resting: boolean;
  readonly previous: Float32Array;
  readonly wetness: Float32Array;
  elapsed = 0;
  private readonly effectCells: { index: number; u: number; distance: number }[][];
  constructor(
    readonly domain: CompiledWaterDomain,
    readonly definition: WaterDefinition,
  ) {
    const cells = domain.cells;
    this.active = new Uint8Array(Math.ceil(domain.resolution / 8) ** 2);
    this.halo = new Uint8Array(this.active.length);
    this.resting = !definition.domain?.sources.length && cells.every((v, i) => i % 4 < 2 || v === 0);
    this.state = new Float64Array(cells.length);
    this.delta = new Float64Array(cells.length);
    this.previous = new Float32Array(cells.length);
    this.wetness = new Float32Array(cells.length / 4);
    this.effectCells = (definition.effects ?? []).map((effect) => {
      const dx = effect.end[0] - effect.start[0],
        dz = effect.end[1] - effect.start[1],
        length = Math.hypot(dx, dz),
        list = [];
      for (let i = 0; i < cells.length / 4; i++) {
        const x = domain.min[0] + (i % domain.resolution) * domain.spacing[0] - effect.start[0];
        const z = domain.min[1] + Math.floor(i / domain.resolution) * domain.spacing[1] - effect.start[1];
        const u = (x * dx + z * dz) / (length * length),
          distance = (-x * dz + z * dx) / length;
        if (u >= 0 && u <= 1 && Math.abs(distance) < effect.width * 1.2) list.push({ index: i, u, distance });
      }
      return list;
    });
    for (let i = 0; i < cells.length; i += 4) {
      const h = Math.max(0, cells[i + 1] - cells[i]);
      this.state[i] = h;
      this.wetness[i / 4] = h > dry ? 1 : 0;
      this.state[i + 1] = h * cells[i + 2];
      this.state[i + 2] = h * cells[i + 3];
    }
    const n = domain.resolution;
    this.sources = (definition.domain?.sources ?? []).map((source) => {
      const weights: [number, number][] = [];
      let total = 0;
      for (let row = 0; row < n; row++)
        for (let col = 0; col < n; col++) {
          const x = domain.min[0] + col * domain.spacing[0],
            z = domain.min[1] + row * domain.spacing[1];
          const distance = Math.hypot(x - source.position[0], z - source.position[1]);
          if (distance > Math.max(source.radius, ...domain.spacing)) continue;
          const weight = Math.exp(-3 * (distance / Math.max(source.radius, ...domain.spacing)) ** 2);
          weights.push([(row * n + col) * 4, weight]);
          total += weight;
        }
      if (!total) throw new Error("Water source lies outside its simulation domain");
      for (const weight of weights) weight[1] /= total;
      return { ...source, weights };
    });
    let level: number | undefined;
    for (let i = 0; i < cells.length; i += 4)
      if (this.state[i] > dry) {
        level ??= cells[i + 1];
        if (Math.abs(cells[i + 1] - level) > 1e-9) this.resting = false;
      }
    this.writeRender(this.previous);
  }
  get byteLength() {
    return (
      this.state.byteLength +
      this.delta.byteLength +
      this.previous.byteLength +
      this.wetness.byteLength +
      this.active.byteLength +
      this.halo.byteLength
    );
  }
  get volume() {
    let total = 0;
    for (let i = 0; i < this.state.length; i += 4) total += this.state[i];
    return total * this.domain.spacing[0] * this.domain.spacing[1];
  }
  private face(a: number, b: number, axis: 1 | 2, dt: number) {
    const q = this.state,
      d = this.delta,
      bed = this.domain.cells;
    const hA = q[a],
      hB = q[b],
      z = Math.max(bed[a], bed[b]);
    if (hA === 0 && hB === 0) return;
    const left = Math.max(0, hA + bed[a] - z),
      right = Math.max(0, hB + bed[b] - z);
    const uA = hA > dry ? q[a + axis] / hA : 0,
      uB = hB > dry ? q[b + axis] / hB : 0;
    const speed = Math.max(
      Math.abs(uA) + Math.sqrt(gravity * left),
      Math.abs(uB) + Math.sqrt(gravity * right),
    );
    const mass = (left * uA + right * uB - speed * (right - left)) * 0.5;
    d[a] -= dt * mass;
    d[b] += dt * mass;
    for (let k = 1; k <= 2; k++) {
      const l = hA > dry ? (q[a + k] * left) / hA : 0,
        r = hB > dry ? (q[b + k] * right) / hB : 0;
      const pressure = k === axis ? gravity * 0.25 * (left * left + right * right) : 0;
      const flux = 0.5 * (l * uA + r * uB - speed * (r - l)) + pressure;
      d[a + k] -= dt * (flux + (k === axis ? gravity * 0.5 * (hA * hA - left * left) : 0));
      d[b + k] += dt * (flux + (k === axis ? gravity * 0.5 * (hB * hB - right * right) : 0));
    }
    const concentration = mass >= 0 ? (hA > dry ? q[a + 3] / hA : 0) : hB > dry ? q[b + 3] / hB : 0;
    d[a + 3] -= dt * mass * concentration;
    d[b + 3] += dt * mass * concentration;
  }
  step(seconds: number) {
    if (!Number.isFinite(seconds) || seconds < 0 || seconds > 0.1)
      throw new RangeError("Water steps must be between zero and 0.1 seconds");
    if (!seconds) return;
    if (!this.definition.domain?.simulate) {
      this.advanceSurface(seconds);
      this.revision++;
      this.lastStepMs = 0;
      return;
    }
    const started = performance.now();
    if (this.resting && !this.definition.effects?.length) {
      this.lastStepMs = 0;
      this.substeps = 0;
      return;
    }
    const n = this.domain.resolution,
      [dx, dz] = this.domain.spacing,
      q = this.state,
      d = this.delta;
    let remaining = seconds;
    this.substeps = 0;
    while (remaining > 1e-10) {
      let rate = 0;
      const tileN = Math.ceil(n / 8);
      this.active.fill(0);
      for (let i = 0; i < q.length; i += 4)
        if (q[i] > dry) {
          const cell = i / 4,
            tx = Math.floor((cell % n) / 8),
            tz = Math.floor(Math.floor(cell / n) / 8);
          this.active[tz * tileN + tx] = 1;
          const wave = Math.sqrt(gravity * q[i]);
          rate = Math.max(
            rate,
            (Math.abs(q[i + 1] / q[i]) + wave) / dx + (Math.abs(q[i + 2] / q[i]) + wave) / dz,
          );
        }
      this.halo.set(this.active);
      for (let tile = 0; tile < this.active.length; tile++)
        if (this.active[tile]) {
          const tx = tile % tileN,
            tz = Math.floor(tile / tileN);
          // Include the complete flux halo once per tile, rather than once per wet cell.
          for (let z = Math.max(0, tz - 1); z <= Math.min(tileN - 1, tz + 1); z++)
            for (let x = Math.max(0, tx - 1); x <= Math.min(tileN - 1, tx + 1); x++)
              this.halo[z * tileN + x] = 1;
        }
      this.active.set(this.halo);
      this.activeTiles = this.active.reduce((a, b) => a + b, 0);
      const dt = Math.min(remaining, rate ? 0.4 / rate : remaining);
      const damping = 1 / (1 + dt * (this.definition.domain?.friction ?? 0));
      const foamDecay = Math.exp(-dt / (this.definition.optics?.foamLifetime ?? 3.125));
      if (++this.substeps > 128)
        throw new Error("Water stability budget exceeded; increase cell size or reduce forcing");
      d.fill(0);
      for (let row = 0; row < n; row++)
        for (let col = 0; col < n; col++) {
          if (!this.active[Math.floor(row / 8) * tileN + Math.floor(col / 8)]) {
            col = Math.min(n - 1, (Math.floor(col / 8) + 1) * 8 - 1);
            continue;
          }
          const i = (row * n + col) * 4;
          if (col + 1 < n) this.face(i, i + 4, 1, dt / dx);
          if (row + 1 < n) this.face(i, i + n * 4, 2, dt / dz);
          // A reflecting ghost cell supplies the outer pressure without mass flux.
          if (col === 0) d[i + 1] += (dt / dx) * gravity * q[i] * q[i] * 0.5;
          if (col === n - 1) d[i + 1] -= (dt / dx) * gravity * q[i] * q[i] * 0.5;
          if (row === 0) d[i + 2] += (dt / dz) * gravity * q[i] * q[i] * 0.5;
          if (row === n - 1) d[i + 2] -= (dt / dz) * gravity * q[i] * q[i] * 0.5;
        }
      for (let i = 0; i < q.length; i += 4) {
        const h = q[i] + d[i];
        if (h < -1e-9 || !Number.isFinite(h)) throw new Error("Nonphysical water depth");
        q[i] = Math.max(0, h);
        if (h <= 0) {
          q[i + 1] = 0;
          q[i + 2] = 0;
          q[i + 3] = 0;
          continue;
        }
        q[i + 1] = h > dry ? (q[i + 1] + d[i + 1]) * damping : 0;
        q[i + 2] = h > dry ? (q[i + 2] + d[i + 2]) * damping : 0;
        const speed = h > dry ? Math.hypot(q[i + 1], q[i + 2]) / h : 0;
        const generation = Math.max(0, speed / Math.sqrt(gravity * Math.max(h, 0.03)) - 0.55) * 0.35;
        q[i + 3] = Math.min(q[i], Math.max(0, (q[i + 3] + d[i + 3]) * foamDecay + dt * generation * q[i]));
      }
      for (const source of this.sources)
        for (const [i, weight] of source.weights) {
          const addition = Math.max(-q[i], (source.rate * weight * dt) / (dx * dz));
          if (addition < 0 && q[i] > 0) {
            const fraction = (q[i] + addition) / q[i];
            q[i + 1] *= fraction;
            q[i + 2] *= fraction;
            q[i + 3] *= fraction;
          } else {
            q[i + 1] += addition * source.velocity[0];
            q[i + 2] += addition * source.velocity[1];
          }
          q[i] += addition;
          this.exchangedVolume += addition * dx * dz;
        }
      remaining -= dt;
    }
    this.advanceSurface(seconds);
    this.revision++;
    this.lastStepMs = performance.now() - started;
  }
  private advanceSurface(seconds: number) {
    this.elapsed += seconds;
    const decay = Math.exp(-seconds / (this.definition.domain?.dryingSeconds ?? 18));
    for (let i = 0; i < this.wetness.length; i++) {
      this.wetness[i] = this.state[i * 4] > dry ? 1 : this.wetness[i] * decay;
      if (!this.definition.domain?.simulate)
        this.state[i * 4 + 3] *= Math.exp(-seconds / (this.definition.optics?.foamLifetime ?? 4));
    }
    for (const [e, effect] of (this.definition.effects ?? []).entries()) {
      for (const cell of this.effectCells[e]) {
        const phase = (this.elapsed / effect.period + effect.phase + 0.025 * Math.sin(cell.u * 12)) % 1;
        const center = effect.kind === "breaker" ? (phase - 0.4) * effect.width : effect.width;
        const strength = Math.exp(-(((cell.distance - center) / Math.max(0.25, effect.width * 0.16)) ** 2));
        const i = cell.index * 4,
          h = this.state[i];
        if (h > dry) this.state[i + 3] = Math.min(h, this.state[i + 3] + seconds * strength * h * 2.5);
        if (this.domain.cells[i] < this.definition.level + effect.height * strength * 0.18)
          this.wetness[cell.index] = Math.max(this.wetness[cell.index], strength);
      }
    }
  }
  /** Return a body's horizontal drag impulse to the fluid (water density 1000 kg/m³). */
  receiveBodyImpulse(x: number, z: number, radius: number, impulseX: number, impulseZ: number) {
    if (!this.definition.domain?.simulate) return;
    if (![x, z, radius, impulseX, impulseZ].every(Number.isFinite) || radius <= 0 || radius > 20)
      throw new RangeError("Invalid body/water impulse");
    const n = this.domain.resolution,
      [dx, dz] = this.domain.spacing,
      q = this.state;
    const r = Math.max(radius, dx, dz),
      cellX = (x - this.domain.min[0]) / dx,
      cellZ = (z - this.domain.min[1]) / dz;
    const weights: [number, number][] = [];
    let sum = 0;
    for (
      let row = Math.max(0, Math.floor(cellZ - (r * 2) / dz));
      row <= Math.min(n - 1, Math.ceil(cellZ + (r * 2) / dz));
      row++
    )
      for (
        let col = Math.max(0, Math.floor(cellX - (r * 2) / dx));
        col <= Math.min(n - 1, Math.ceil(cellX + (r * 2) / dx));
        col++
      ) {
        const at = (row * n + col) * 4;
        if (q[at] <= dry) continue;
        const distanceSquared = ((col - cellX) * dx) ** 2 + ((row - cellZ) * dz) ** 2;
        if (distanceSquared > r * r * 4) continue;
        const weight = q[at] * Math.exp((-3 * distanceSquared) / (r * r));
        weights.push([at, weight]);
        sum += weight;
      }
    if (!sum) return;
    for (const [at, weight] of weights) {
      q[at + 1] += (impulseX * weight) / (sum * 1000 * dx * dz);
      q[at + 2] += (impulseZ * weight) / (sum * 1000 * dx * dz);
    }
    this.resting = false;
    this.revision++;
  }
  /** Radial momentum forcing creates a wave without inventing or deleting water. */
  disturb(x: number, z: number, radius: number, strength: number) {
    this.resting = false;
    if (
      ![x, z, radius, strength].every(Number.isFinite) ||
      radius <= 0 ||
      radius > 20 ||
      Math.abs(strength) > 5
    )
      throw new RangeError("Invalid water disturbance");
    const n = this.domain.resolution,
      q = this.state;
    const r = Math.max(radius, ...this.domain.spacing);
    const cx = (x - this.domain.min[0]) / this.domain.spacing[0],
      cz = (z - this.domain.min[1]) / this.domain.spacing[1];
    for (
      let row = Math.max(0, Math.floor(cz - (2 * r) / this.domain.spacing[1]));
      row <= Math.min(n - 1, Math.ceil(cz + (2 * r) / this.domain.spacing[1]));
      row++
    )
      for (
        let col = Math.max(0, Math.floor(cx - (2 * r) / this.domain.spacing[0]));
        col <= Math.min(n - 1, Math.ceil(cx + (2 * r) / this.domain.spacing[0]));
        col++
      ) {
        const i = (row * n + col) * 4,
          px = this.domain.min[0] + col * this.domain.spacing[0] - x,
          pz = this.domain.min[1] + row * this.domain.spacing[1] - z;
        const distance = Math.hypot(px, pz);
        if (distance > r * 2 || q[i] < 0.005) continue;
        const force =
          (strength * Math.exp((-3 * distance * distance) / (r * r)) * q[i]) / Math.max(r, distance);
        q[i + 1] += px * force;
        q[i + 2] += pz * force;
        q[i + 3] = Math.min(q[i], q[i + 3] + Math.abs(force) * 0.08);
      }
    this.revision++;
  }
  writeRender(target: Float32Array) {
    const q = this.state,
      bed = this.domain.cells,
      n = this.domain.resolution,
      extension = this.delta;
    // The residual scratch is idle between substeps. Scatter free-surface
    // heights from wet cells, rather than searching nine neighbours at every
    // dry bank node. Iterating sources in row order preserves the old sum order.
    extension.fill(0);
    for (let i = 0; i < q.length; i += 4) {
      const wet = q[i] > dry;
      const level = bed[i] + q[i];
      target[i] = wet ? level : bed[i + 1];
      target[i + 1] = wet ? q[i + 1] / q[i] : 0;
      target[i + 2] = wet ? q[i + 2] / q[i] : 0;
      target[i + 3] = wet ? q[i + 3] / q[i] : 0;
      if (wet) {
        const cell = i / 4,
          row = Math.floor(cell / n),
          col = cell % n;
        for (let dz = -1; dz <= 1; dz++)
          for (let dx = -1; dx <= 1; dx++) {
            if (row + dz < 0 || row + dz >= n || col + dx < 0 || col + dx >= n) continue;
            const at = ((row + dz) * n + col + dx) * 4;
            if (q[at] <= dry) {
              extension[at] += level;
              extension[at + 1]++;
            }
          }
      }
    }
    for (let i = 0; i < q.length; i += 4)
      if (extension[i + 1] > 0) target[i] = extension[i] / extension[i + 1];
  }
  snapshot(): WaterSimulationSnapshot {
    return {
      state: this.state.slice(),
      exchangedVolume: this.exchangedVolume,
      elapsed: this.elapsed,
      wetness: this.wetness.slice(),
    };
  }
  restore(snapshot: WaterSimulationSnapshot) {
    if (
      snapshot.state.length !== this.state.length ||
      !Number.isFinite(snapshot.exchangedVolume) ||
      !snapshot.state.every(Number.isFinite)
    )
      throw new Error("Invalid water checkpoint");
    for (let i = 0; i < snapshot.state.length; i += 4)
      if (snapshot.state[i] < 0 || snapshot.state[i + 3] < 0 || snapshot.state[i + 3] > snapshot.state[i])
        throw new Error("Nonphysical water checkpoint");
    if (snapshot.elapsed !== undefined && (!Number.isFinite(snapshot.elapsed) || snapshot.elapsed < 0))
      throw Error("Invalid water clock");
    if (
      snapshot.wetness &&
      (snapshot.wetness.length !== this.wetness.length ||
        !snapshot.wetness.every((v) => Number.isFinite(v) && v >= 0 && v <= 1))
    )
      throw Error("Invalid water wetness");
    this.elapsed = snapshot.elapsed ?? 0;
    if (snapshot.wetness) this.wetness.set(snapshot.wetness);
    else for (let i = 0; i < this.wetness.length; i++) this.wetness[i] = snapshot.state[i * 4] > dry ? 1 : 0;
    this.resting = false;
    this.state.set(snapshot.state);
    this.exchangedVolume = snapshot.exchangedVolume;
    this.writeRender(this.previous);
    this.revision++;
  }
}
