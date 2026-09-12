// Resolution-only governor. It never changes world detail, vegetation or lighting.
export class AdaptiveResolution {
  constructor(height = 540) {
    this.steps = [420, 480, 540, 600, 660, 720];
    this.index = this.steps.reduce((best, h, i) => Math.abs(h - height) < Math.abs(this.steps[best] - height) ? i : best, 0);
    this.blockUpUntil = 0;
    this.probe = null;
    this.reset(0);
  }
  get height() { return this.steps[this.index]; }
  reset(now) {
    this.since = now; this.sum = 0; this.frames = 0; this.slow = 0;
    this.stableSince = now; this.holdUntil = now + 2500; this.active = false;
  }
  sample(now, gap, eligible) {
    if (!eligible) { this.active = false; return null; }
    if (!this.active) { this.reset(now); this.active = true; }
    if (now < this.holdUntil || gap <= 0 || gap > 250) return null;
    if (!this.frames) this.since = now;
    this.sum += gap; this.frames++; this.slow += Number(gap > 20.5);
    if (now - this.since < 1800) return null;
    const mean = this.sum / this.frames, slow = this.slow / this.frames;
    this.sum = 0; this.frames = 0; this.slow = 0;
    const overloaded = mean > 20.5 && slow > 0.15;
    const stable = mean < 17.8 && slow < 0.035;
    if (!stable) this.stableSince = now;
    if (overloaded && this.index > 0) {
      if (this.probe !== null) this.blockUpUntil = now + 60000;
      this.index--; this.probe = null;
      this.reset(now); this.active = true; return this.height;
    }
    // RAF at the refresh limit does not prove spare GPU time. An upshift is a
    // slow probe; failed probes back off for a minute instead of pumping detail.
    if (stable && now - this.stableSince > 18000 && now > this.blockUpUntil && this.index < this.steps.length - 1) {
      this.probe = this.index; this.index++;
      this.reset(now); this.active = true; return this.height;
    }
    if (stable && this.probe !== null && now - this.stableSince > 8000) this.probe = null;
    return null;
  }
}
