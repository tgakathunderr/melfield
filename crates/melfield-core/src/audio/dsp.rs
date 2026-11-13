//! Mono f32 PCM helpers: resample, trim, stitch, normalise.

use crate::error::{MelError, MelResult};

#[derive(Debug, Clone, PartialEq)]
pub struct Pcm {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Pcm {
    pub fn new(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            sample_rate,
        }
    }

    pub fn duration_s(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / self.sample_rate as f64
    }

    pub fn silence(seconds: f32, sample_rate: u32) -> Self {
        Self::new(
            vec![0.0; (seconds * sample_rate as f32) as usize],
            sample_rate,
        )
    }

    pub fn rms(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        (self.samples.iter().map(|s| s * s).sum::<f32>() / self.samples.len() as f32).sqrt()
    }

    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }
}

/// Resample with a high-quality sinc resampler. No-op when rates match.
pub fn resample(pcm: &Pcm, to_rate: u32) -> MelResult<Pcm> {
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };
    if pcm.sample_rate == to_rate || pcm.samples.is_empty() {
        return Ok(Pcm::new(pcm.samples.clone(), to_rate));
    }
    let params = SincInterpolationParameters {
        sinc_len: 128,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    let chunk = 1024;
    let ratio = to_rate as f64 / pcm.sample_rate as f64;
    let mut rs =
        SincFixedIn::<f32>::new(ratio, 1.0, params, chunk, 1).map_err(MelError::internal)?;
    let mut out = Vec::with_capacity((pcm.samples.len() as f64 * ratio) as usize + chunk);
    let mut pos = 0;
    while pos + chunk <= pcm.samples.len() {
        let block = rs
            .process(&[&pcm.samples[pos..pos + chunk]], None)
            .map_err(MelError::internal)?;
        out.extend_from_slice(&block[0]);
        pos += chunk;
    }
    let tail = &pcm.samples[pos..];
    let block = rs
        .process_partial(Some(&[tail]), None)
        .map_err(MelError::internal)?;
    out.extend_from_slice(&block[0]);
    // Flush the filter delay.
    let block = rs
        .process_partial::<&[f32]>(None, None)
        .map_err(MelError::internal)?;
    out.extend_from_slice(&block[0]);
    let expected = (pcm.samples.len() as f64 * ratio).round() as usize;
    let delay = rs.output_delay();
    let start = delay.min(out.len());
    let end = (start + expected).min(out.len());
    Ok(Pcm::new(out[start..end].to_vec(), to_rate))
}

/// Trim leading/trailing near-silence, keeping a small natural pad.
pub fn trim_silence(pcm: &Pcm, threshold: f32, pad_s: f32) -> Pcm {
    let pad = (pad_s * pcm.sample_rate as f32) as usize;
    let first = pcm.samples.iter().position(|s| s.abs() > threshold);
    let last = pcm.samples.iter().rposition(|s| s.abs() > threshold);
    match (first, last) {
        (Some(f), Some(l)) => {
            let start = f.saturating_sub(pad);
            let end = (l + pad + 1).min(pcm.samples.len());
            Pcm::new(pcm.samples[start..end].to_vec(), pcm.sample_rate)
        }
        _ => Pcm::new(Vec::new(), pcm.sample_rate),
    }
}

/// Append `next` to `acc` with an equal-power crossfade of `fade_s` seconds
/// after a `gap_s` pause. Both must share a sample rate.
pub fn stitch(acc: &mut Vec<f32>, next: &[f32], sample_rate: u32, gap_s: f32, fade_s: f32) {
    if acc.is_empty() {
        acc.extend_from_slice(next);
        return;
    }
    acc.extend(std::iter::repeat_n(
        0.0,
        (gap_s * sample_rate as f32) as usize,
    ));
    let fade = ((fade_s * sample_rate as f32) as usize)
        .min(next.len())
        .min(acc.len());
    let start = acc.len() - fade;
    for i in 0..fade {
        let t = (i as f32 + 0.5) / fade as f32;
        let (fo, fi) = (
            (t * std::f32::consts::FRAC_PI_2).cos(),
            (t * std::f32::consts::FRAC_PI_2).sin(),
        );
        acc[start + i] = acc[start + i] * fo + next[i] * fi;
    }
    acc.extend_from_slice(&next[fade..]);
}

/// Integrated-loudness normalisation (EBU R128) to `target_lufs`, with a
/// simple peak ceiling so nothing clips.
pub fn normalize_loudness(pcm: &Pcm, target_lufs: f64, ceiling: f32) -> Pcm {
    if pcm.samples.len() < pcm.sample_rate as usize / 2 {
        return peak_limit(pcm, ceiling);
    }
    let measured = (|| {
        let mut m = ebur128::EbuR128::new(1, pcm.sample_rate, ebur128::Mode::I).ok()?;
        m.add_frames_f32(&pcm.samples).ok()?;
        m.loudness_global().ok()
    })();
    let Some(lufs) = measured.filter(|l| l.is_finite()) else {
        return peak_limit(pcm, ceiling);
    };
    let gain = 10f64.powf((target_lufs - lufs) / 20.0).clamp(0.1, 8.0) as f32;
    let boosted = Pcm::new(
        pcm.samples.iter().map(|s| s * gain).collect(),
        pcm.sample_rate,
    );
    peak_limit(&boosted, ceiling)
}

fn peak_limit(pcm: &Pcm, ceiling: f32) -> Pcm {
    let peak = pcm.peak();
    if peak <= ceiling || peak == 0.0 {
        return pcm.clone();
    }
    let g = ceiling / peak;
    Pcm::new(pcm.samples.iter().map(|s| s * g).collect(), pcm.sample_rate)
}

/// Gentle speed change by resampling the timeline (pitch shifts slightly);
/// used only for engines without a native speed control.
pub fn high_pass(pcm: &Pcm, cutoff_hz: f32) -> Pcm {
    let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
    let dt = 1.0 / pcm.sample_rate as f32;
    let alpha = rc / (rc + dt);
    let mut out = Vec::with_capacity(pcm.samples.len());
    let (mut prev_in, mut prev_out) = (0.0f32, 0.0f32);
    for &x in &pcm.samples {
        let y = alpha * (prev_out + x - prev_in);
        out.push(y);
        prev_in = x;
        prev_out = y;
    }
    Pcm::new(out, pcm.sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, secs: f32, rate: u32, amp: f32) -> Pcm {
        let n = (secs * rate as f32) as usize;
        Pcm::new(
            (0..n)
                .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
                .collect(),
            rate,
        )
    }

    #[test]
    fn resample_keeps_duration() {
        let p = sine(440.0, 1.0, 24_000, 0.5);
        let r = resample(&p, 48_000).unwrap();
        assert!((r.duration_s() - 1.0).abs() < 0.01, "{}", r.duration_s());
        let r2 = resample(&p, 16_000).unwrap();
        assert!((r2.duration_s() - 1.0).abs() < 0.01);
        assert!((r2.rms() - p.rms()).abs() < 0.05);
    }

    #[test]
    fn trims_silence_with_pad() {
        let mut s = vec![0.0; 1000];
        s.extend(vec![0.5; 500]);
        s.extend(vec![0.0; 1000]);
        let t = trim_silence(&Pcm::new(s, 1000), 0.01, 0.1);
        assert_eq!(t.samples.len(), 500 + 200);
        assert!(trim_silence(&Pcm::new(vec![0.0; 10], 1000), 0.01, 0.1)
            .samples
            .is_empty());
    }

    #[test]
    fn stitch_adds_gap_and_crossfades_without_clicks() {
        let mut acc = vec![0.5; 1000];
        stitch(&mut acc, &[0.5; 1000], 1000, 0.1, 0.02);
        assert_eq!(acc.len(), 1000 + 100 + 1000 - 20);
        assert!(acc.iter().all(|s| s.abs() <= 0.71));
    }

    #[test]
    fn normalises_quiet_audio_up_and_never_clips() {
        let quiet = sine(220.0, 2.0, 24_000, 0.01);
        let n = normalize_loudness(&quiet, -16.0, 0.89);
        assert!(n.rms() > quiet.rms() * 3.0);
        assert!(n.peak() <= 0.891);
        let loud = sine(220.0, 2.0, 24_000, 1.0);
        assert!(normalize_loudness(&loud, -16.0, 0.89).peak() <= 0.891);
    }

    #[test]
    fn high_pass_removes_dc() {
        let dc = Pcm::new(vec![0.5; 24_000], 24_000);
        let hp = high_pass(&dc, 80.0);
        assert!(hp.samples[23_000].abs() < 0.01);
    }
}
