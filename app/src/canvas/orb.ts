// Voice orbs (ported from design/screens/Voices.dc.html drawOrb()).
// `pulse` (0..1) swells the orb while its preview plays; `assemble` (0..1)
// drives the clone reveal where scattered dots converge into the shape.

import type { Orb } from "../lib/types";
import { fit, rgba } from "./loop";

export interface OrbDrawOptions {
  seed: number;
  pulse?: number;
  assemble?: number;
  step?: number;
}

/** Deterministic hash → [0,1) for scatter positions. */
function hash(x: number, y: number, s: number): number {
  const v = Math.sin(x * 127.1 + y * 311.7 + s * 74.7) * 43758.5453;
  return v - Math.floor(v);
}

export function drawOrb(cv: HTMLCanvasElement, orb: Orb, t: number, opts: OrbDrawOptions): void {
  const f = fit(cv);
  if (!f) return;
  const { ctx, w: S } = f;
  const step = opts.step ?? 4;
  const c = S / 2;
  const pulse = opts.pulse ?? 0;
  const assemble = opts.assemble ?? 1;
  const base = S * (0.38 + 0.03 * pulse);
  const amp = orb.amp * (1 + 1.6 * pulse);
  const dir = opts.seed % 2 ? 1 : -1;
  const speed = 1 + 2.5 * pulse;
  for (let x = step / 2; x < S; x += step) {
    for (let y = step / 2; y < S; y += step) {
      const dx = x - c;
      const dy = y - c;
      const r = Math.sqrt(dx * dx + dy * dy);
      const th = Math.atan2(dy, dx);
      const R = base * (1 + amp * Math.sin(orb.k * th + t * 0.0006 * dir * speed) + 0.04 * Math.sin(3 * th - t * 0.0011 + opts.seed));
      let inside: boolean;
      if (orb.ring) {
        inside = Math.abs(r - R * 0.82) < 5 || Math.abs(r - R * 0.5 * (1 + 0.2 * Math.sin(orb.k * th - t * 0.0009))) < 3 || r < 6;
      } else {
        inside = r < R;
      }
      if (!inside) continue;
      const ny = y / S;
      const tw = 0.5 + 0.5 * Math.sin(x * 12.9898 + y * 78.233 + t * 0.003 * speed + opts.seed);
      const a = Math.min(1, (orb.ring ? 0.55 : 0.4) + 0.6 * tw * (1 - (r / (R + 1)) * 0.5));
      const cr = (orb.c1[0] + (orb.c2[0] - orb.c1[0]) * ny) | 0;
      const cg = (orb.c1[1] + (orb.c2[1] - orb.c1[1]) * ny) | 0;
      const cb = (orb.c1[2] + (orb.c2[2] - orb.c1[2]) * ny) | 0;
      let px = x;
      let py = y;
      if (assemble < 1) {
        // Ease-out converge from a scattered start position.
        const k = 1 - Math.pow(1 - assemble, 3);
        const sx = hash(x, y, opts.seed) * S;
        const sy = hash(y, x, opts.seed + 1) * S;
        px = sx + (x - sx) * k;
        py = sy + (y - sy) * k;
      }
      ctx.fillStyle = rgba(cr, cg, cb, a * (0.3 + 0.7 * assemble));
      ctx.fillRect(px - 1, py - 1, 2, 2);
    }
  }
}

/** Tiny dotted orb for the voice chip (CSS-sized, static). */
export function orbGradient(orb: Orb): string {
  const [r, g, b] = orb.c1;
  return `radial-gradient(rgb(${r},${g},${b}) 1px, transparent 1.4px)`;
}
