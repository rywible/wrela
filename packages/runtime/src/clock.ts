export class FixedClock {
  tick = 0;
  private accumulator = 0;
  droppedSeconds = 0;
  constructor(
    readonly stepSeconds = 1 / 60,
    readonly maxSteps = 8,
  ) {
    if (!Number.isFinite(stepSeconds) || stepSeconds <= 0 || !Number.isInteger(maxSteps) || maxSteps < 1)
      throw new RangeError("Invalid simulation clock");
  }
  get time() {
    return this.tick * this.stepSeconds;
  }
  get alpha() {
    return this.accumulator / this.stepSeconds;
  }
  advance(seconds: number, step: (dt: number, tick: number) => void) {
    if (!Number.isFinite(seconds) || seconds < 0) throw new RangeError("Invalid frame duration");
    const accepted = Math.min(seconds, this.stepSeconds * this.maxSteps);
    this.droppedSeconds += seconds - accepted;
    this.accumulator += accepted;
    let count = 0;
    while (this.accumulator + 1e-10 >= this.stepSeconds && count < this.maxSteps) {
      this.accumulator -= this.stepSeconds;
      this.tick++;
      step(this.stepSeconds, this.tick);
      count++;
    }
    return count;
  }
  reset(tick = 0) {
    if (!Number.isInteger(tick) || tick < 0) throw new RangeError("Invalid tick");
    this.tick = tick;
    this.accumulator = 0;
  }
}
