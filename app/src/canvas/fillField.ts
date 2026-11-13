// Onboarding: dots fill the screen from the bottom as voices download
// (ported from design/screens/Onboarding.dc.html draw()).

import { fit, rgba } from "./loop";

export function drawFillField(cv: HTMLCanvasElement, t: number, progress: number): void {
  const f = fit(cv);
  if (!f) return;
  const { ctx, w, h } = f;
  const p = Math.max(0, Math.min(1, progress));
  const step = 10;
  const level = 1 - p * 0.42;
  for (let x = step / 2; x < w; x += step) {
    const nx = x / w;
    const front = level + 0.05 * Math.sin(nx * 6 + t * 0.0009) + 0.025 * Math.sin(nx * 17 - t * 0.0016) + 0.015 * Math.sin(nx * 41 + t * 0.002);
    for (let y = step / 2; y < h; y += step) {
      const ny = y / h;
      if (ny < front) {
        if (ny > 0.5) {
          ctx.fillStyle = "rgba(200,200,210,0.05)";
          ctx.fillRect(x - 1, y - 1, 2, 2);
        }
        continue;
      }
      const depth = Math.min(1, (ny - front) / 0.35);
      let R = 224 + 31 * (1 - depth);
      let G = 56 + 50 * (1 - depth);
      let B = 154 - 111 * (1 - depth);
      if (depth > 0.6) {
        R = 107 + (R - 107) * 0.4;
        G = 75;
        B = 216;
      }
      const tw = 0.5 + 0.5 * Math.sin(x * 12.99 + y * 78.23 + t * 0.002);
      let a = (1 - depth * 0.75) * (0.45 + 0.55 * tw);
      if (ny - front < 0.012) a = 1;
      ctx.fillStyle = rgba(R | 0, G | 0, B | 0, a);
      const s = ny - front < 0.02 ? 3 : 2;
      ctx.fillRect(x - s / 2, y - s / 2, s, s);
    }
    // Dripping dot columns.
    if (Math.sin(nx * 97 + Math.floor(t / 900)) > 0.985) {
      ctx.fillStyle = "rgba(255,140,90,0.5)";
      for (let yy = front * h; yy < h; yy += step) ctx.fillRect(x - 0.5, yy, 1, 4);
    }
  }
}
