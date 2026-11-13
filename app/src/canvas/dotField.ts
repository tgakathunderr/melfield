// Studio background: a living mel-spectrogram dot field (ported from
// design/screens/Main.dc.html draw()). Quiet in the centre, busier at the
// edges; swells and ripples with the REAL playback spectrum (audioBus).

import { audioBus, BANDS } from "../lib/audioBus";
import { fit, rgba } from "./loop";

export interface DotFieldOptions {
  intensity: number;
  /** Reduced effects: half density, no 3px dots. */
  reduced: boolean;
}

const BAND_DEFS = [
  { c: 0.7, a: 0.55, s: 0.0007 },
  { c: 0.8, a: 0.85, s: 0.0011 },
  { c: 0.9, a: 1.0, s: 0.0009 },
  { c: 0.14, a: 0.35, s: 0.0005 },
];

/** Map a horizontal position to a spectrum band, mirrored from the centre. */
export function bandAt(nx: number, bands: ArrayLike<number>): number {
  const d = Math.min(1, Math.abs(nx - 0.5) * 2); // 0 centre .. 1 edge
  const idx = Math.min(BANDS - 1, Math.floor(d * (BANDS - 1)));
  return bands[idx] ?? 0;
}

export function drawDotField(cv: HTMLCanvasElement, t: number, opts: DotFieldOptions): void {
  const f = fit(cv);
  if (!f) return;
  const { ctx, w, h } = f;
  const energy = audioBus.energy;
  const live = audioBus.playing;
  const gain = opts.intensity * (1 + 1.4 * energy);
  const step = opts.reduced ? 14 : 10;
  const cx = w / 2;
  const cy = h * 0.46;
  for (let x = step / 2; x < w; x += step) {
    const nx = x / w;
    const spec = live ? bandAt(nx, audioBus.bands) : 0;
    const col = (0.45 + 0.55 * Math.abs(Math.sin(nx * 9 - t * 0.0011) * Math.sin(nx * 3.1 + t * 0.0006))) * (1 + 1.2 * spec);
    for (let y = step / 2; y < h; y += step) {
      const ny = y / h;
      let e = 0;
      for (let b = 0; b < BAND_DEFS.length; b++) {
        const B = BAND_DEFS[b];
        // Spectrum pushes the low bands upward like a rising level meter.
        const lift = b < 3 ? spec * 0.06 : 0;
        const c = B.c - lift + 0.035 * Math.sin(nx * 7 + t * B.s * 2 + b) + 0.02 * Math.sin(nx * 23 - t * B.s * 3);
        const d = (ny - c) / (0.035 + spec * 0.02);
        e += B.a * Math.exp(-d * d);
      }
      e *= col * gain;
      const dx = (x - cx) / (w * 0.36);
      const dy = (y - cy) / (h * 0.3);
      const r = Math.sqrt(dx * dx + dy * dy);
      const fade = Math.min(1, Math.max(0, (r - 0.55) / 0.6));
      const a = (0.05 + Math.min(1, e) * 0.85) * (0.15 + 0.85 * fade);
      if (a < 0.03) continue;
      let R: number;
      let G: number;
      let Bc: number;
      if (e < 0.12) {
        R = 200;
        G = 200;
        Bc = 210;
      } else if (ny > 0.5) {
        const k = Math.max(0, Math.min(1, (ny - 0.62) / 0.3));
        R = 224 + 31 * k;
        G = 56 + 50 * k;
        Bc = 154 - 111 * k;
      } else {
        R = 107;
        G = 75;
        Bc = 216;
      }
      ctx.fillStyle = rgba(R | 0, G | 0, Bc | 0, a);
      const s = e > 0.75 && !opts.reduced ? 3 : 2;
      ctx.fillRect(x - s / 2, y - s / 2, s, s);
    }
  }
}
