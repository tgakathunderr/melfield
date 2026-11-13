// Clone record orb (ported from design/screens/Clone.dc.html draw()):
// calm grey dots at rest; red and reacting to the live mic level while recording.

import { audioBus } from "../lib/audioBus";
import { fit, rgba } from "./loop";

export function drawRecordOrb(cv: HTMLCanvasElement, t: number, recording: boolean): void {
  const f = fit(cv);
  if (!f) return;
  const { ctx, w: S } = f;
  const c = S / 2;
  const step = 5;
  const mic = audioBus.micLevel;
  const energy = recording ? 0.12 + 0.35 * mic + 0.05 * Math.abs(Math.sin(t * 0.007)) : 0.05;
  const base = S * (recording ? 0.34 + 0.06 * mic : 0.3);
  for (let x = step / 2; x < S; x += step) {
    for (let y = step / 2; y < S; y += step) {
      const dx = x - c;
      const dy = y - c;
      const r = Math.sqrt(dx * dx + dy * dy);
      const th = Math.atan2(dy, dx);
      const R = base * (1 + energy * Math.sin(5 * th + t * 0.002) + energy * 0.6 * Math.sin(9 * th - t * 0.0035));
      if (r > R) continue;
      const ny = y / S;
      const tw = 0.5 + 0.5 * Math.sin(x * 12.99 + y * 78.23 + t * (recording ? 0.01 : 0.002));
      const col = recording ? [255, 90 + 60 * ny, 70 + 60 * ny] : [200 + 30 * (1 - ny), 200, 215];
      const a = (recording ? 0.5 : 0.25) + (recording ? 0.5 : 0.35) * tw;
      ctx.fillStyle = rgba(col[0] | 0, col[1] | 0, col[2] | 0, a);
      ctx.fillRect(x - 1, y - 1, 2, 2);
    }
  }
}
