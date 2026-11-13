// High-frequency audio state for canvases, kept OUT of React state so 60 Hz
// spectrum frames never trigger re-renders. Canvases read it inside rAF.

export const BANDS = 64;

export const audioBus = {
  /** Latest spectrum bands, 0..1 each. */
  bands: new Float32Array(BANDS),
  /** Overall energy 0..1 (mean of mid bands), smoothed. */
  energy: 0,
  playing: false,
  /** Live mic level 0..1 while recording. */
  micLevel: 0,
  recording: false,
  /** 0..1 download progress for the onboarding dots-fill. */
  fill: 0,
};

export function pushBands(bands: number[], playing: boolean): void {
  const n = Math.min(BANDS, bands.length);
  let sum = 0;
  for (let i = 0; i < n; i++) {
    audioBus.bands[i] = bands[i];
    if (i > 4 && i < 48) sum += bands[i];
  }
  const target = playing ? sum / 43 : 0;
  audioBus.energy += (target - audioBus.energy) * 0.35;
  audioBus.playing = playing;
}
