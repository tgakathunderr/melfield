// One shared rAF loop for every canvas. Pauses when the window is hidden,
// renders a single static frame under prefers-reduced-motion, caps DPR at 2.

type Drawer = (t: number) => void;

const drawers = new Set<Drawer>();
let raf = 0;
let reducedMotion = false;

if (typeof window !== "undefined" && window.matchMedia) {
  const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
  reducedMotion = mq.matches;
  mq.addEventListener?.("change", (e) => {
    reducedMotion = e.matches;
    kick();
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) stop();
    else kick();
  });
}

export function prefersReducedMotion(): boolean {
  return reducedMotion;
}

function frame(t: number) {
  raf = 0;
  drawers.forEach((d) => d(t));
  if (!reducedMotion && !document.hidden && drawers.size > 0) {
    raf = requestAnimationFrame(frame);
  }
}

function stop() {
  if (raf) cancelAnimationFrame(raf);
  raf = 0;
}

/** Start (or, under reduced motion, draw once). */
export function kick(): void {
  if (!raf && drawers.size > 0 && typeof requestAnimationFrame !== "undefined") {
    raf = requestAnimationFrame(frame);
  }
}

export function addDrawer(d: Drawer): () => void {
  drawers.add(d);
  kick();
  return () => {
    drawers.delete(d);
    if (drawers.size === 0) stop();
  };
}

/** Size a canvas to its CSS box at a capped DPR; returns a ready 2D context. */
export function fit(cv: HTMLCanvasElement): { ctx: CanvasRenderingContext2D; w: number; h: number } | null {
  const w = cv.clientWidth;
  const h = cv.clientHeight;
  if (!w || !h) return null;
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  const pw = Math.round(w * dpr);
  const ph = Math.round(h * dpr);
  if (cv.width !== pw || cv.height !== ph) {
    cv.width = pw;
    cv.height = ph;
  }
  const ctx = cv.getContext("2d");
  if (!ctx) return null;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  return { ctx, w, h };
}

/** Cached rgba() strings: alpha quantised to 1/32 to keep fillStyle churn low. */
const rgbaCache = new Map<number, string>();
export function rgba(r: number, g: number, b: number, a: number): string {
  const qa = Math.max(0, Math.min(32, Math.round(a * 32)));
  const key = ((r & 255) << 24) | ((g & 255) << 16) | ((b & 255) << 8) | qa;
  let s = rgbaCache.get(key);
  if (!s) {
    s = `rgba(${r & 255},${g & 255},${b & 255},${(qa / 32).toFixed(3)})`;
    if (rgbaCache.size > 20000) rgbaCache.clear();
    rgbaCache.set(key, s);
  }
  return s;
}
