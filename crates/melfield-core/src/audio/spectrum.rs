//! Spectrum analysis for the dot field: 64 log-spaced bands, 0..1, smoothed.

use std::sync::Arc;

use rustfft::{num_complex::Complex, Fft, FftPlanner};

pub const BANDS: usize = 64;
const FFT_SIZE: usize = 2048;

pub struct Analyzer {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    edges: Vec<usize>,
    smoothed: [f32; BANDS],
}

impl Analyzer {
    pub fn new(sample_rate: u32) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let window = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos())
            .collect();
        // Log-spaced band edges from 60 Hz to min(12 kHz, nyquist).
        let nyquist = sample_rate as f32 / 2.0;
        let (lo, hi) = (60.0f32, 12_000.0f32.min(nyquist * 0.95));
        let bin_hz = sample_rate as f32 / FFT_SIZE as f32;
        let edges = (0..=BANDS)
            .map(|i| {
                let f = lo * (hi / lo).powf(i as f32 / BANDS as f32);
                ((f / bin_hz) as usize).clamp(1, FFT_SIZE / 2 - 1)
            })
            .collect();
        Self {
            fft,
            window,
            edges,
            smoothed: [0.0; BANDS],
        }
    }

    /// Analyse the window ending at `pos` in `samples`. Silence decays smoothly.
    pub fn frame(&mut self, samples: &[f32], pos: usize) -> [f32; BANDS] {
        let mut buf: Vec<Complex<f32>> = (0..FFT_SIZE)
            .map(|i| {
                let idx = (pos + i).checked_sub(FFT_SIZE);
                let s = idx.and_then(|j| samples.get(j)).copied().unwrap_or(0.0);
                Complex::new(s * self.window[i], 0.0)
            })
            .collect();
        self.fft.process(&mut buf);
        let mut out = [0.0f32; BANDS];
        for (b, slot) in out.iter_mut().enumerate() {
            let (a, z) = (self.edges[b], self.edges[b + 1].max(self.edges[b] + 1));
            let energy = buf[a..z].iter().map(|c| c.norm_sqr()).sum::<f32>() / (z - a) as f32;
            // Map to 0..1 on a dB scale (-70 dB .. -10 dB relative to full scale FFT energy).
            let db =
                10.0 * (energy.max(1e-12) / (FFT_SIZE as f32 * FFT_SIZE as f32 / 16.0)).log10();
            let v = ((db + 70.0) / 60.0).clamp(0.0, 1.0);
            // Fast attack, slow release.
            let prev = self.smoothed[b];
            let next = if v > prev {
                prev + (v - prev) * 0.6
            } else {
                prev + (v - prev) * 0.15
            };
            self.smoothed[b] = next;
            *slot = next;
        }
        out
    }

    pub fn reset(&mut self) {
        self.smoothed = [0.0; BANDS];
    }
}

/// Coarse amplitude envelope for the result-pill dot waveform (n columns, 0..1).
pub fn envelope(samples: &[f32], columns: usize) -> Vec<f32> {
    if samples.is_empty() || columns == 0 {
        return vec![0.0; columns];
    }
    let per = (samples.len() / columns).max(1);
    let raw: Vec<f32> = (0..columns)
        .map(|c| {
            let s = &samples[(c * per).min(samples.len())..((c + 1) * per).min(samples.len())];
            if s.is_empty() {
                0.0
            } else {
                (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
            }
        })
        .collect();
    let max = raw.iter().copied().fold(0.0f32, f32::max).max(1e-6);
    raw.into_iter().map(|v| (v / max).clamp(0.0, 1.0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_lights_the_right_band() {
        let rate = 24_000;
        let samples: Vec<f32> = (0..rate)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32).sin())
            .collect();
        let mut a = Analyzer::new(rate as u32);
        let mut f = [0.0; BANDS];
        for _ in 0..10 {
            f = a.frame(&samples, 12_000);
        }
        let loudest = f
            .iter()
            .enumerate()
            .max_by(|x, y| x.1.total_cmp(y.1))
            .unwrap()
            .0;
        assert!((25..45).contains(&loudest), "loudest band {loudest}");
        assert!(f[loudest] > 0.5);
        assert!(f[0] < f[loudest]);
    }

    #[test]
    fn silence_decays() {
        let mut a = Analyzer::new(24_000);
        let f = a.frame(&[0.0; 4096], 4096);
        assert!(f.iter().all(|v| *v < 0.01));
    }

    #[test]
    fn envelope_is_normalised() {
        let e = envelope(&[0.0, 0.0, 0.5, 0.5, 1.0, 1.0], 3);
        assert_eq!(e.len(), 3);
        assert_eq!(e[2], 1.0);
        assert_eq!(e[0], 0.0);
        assert_eq!(envelope(&[], 4), vec![0.0; 4]);
    }
}
