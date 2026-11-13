//! Native Kokoro via sherpa-onnx, fed by our GPL-free English G2P.

use std::path::Path;

use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use sherpa_onnx::{GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig};

use super::{Engine, SynthOutput, SynthRequest, VoiceTarget};
use crate::audio::Pcm;
use crate::error::{ErrorCode, MelError, MelResult};
use crate::g2p::{self, EnglishG2p};

static G2P_US: OnceCell<EnglishG2p> = OnceCell::new();
static G2P_GB: OnceCell<EnglishG2p> = OnceCell::new();

fn g2p_for(lang: &str, preset_sid: i32) -> &'static EnglishG2p {
    // Kokoro's British voices (sid 20..=27) sound right with GB phonemes.
    let british = lang.eq_ignore_ascii_case("en-gb") || (20..=27).contains(&preset_sid);
    if british {
        G2P_GB.get_or_init(|| EnglishG2p::new(true))
    } else {
        G2P_US.get_or_init(|| EnglishG2p::new(false))
    }
}

pub struct KokoroEngine {
    tts: Mutex<OfflineTts>,
    sample_rate: u32,
}

impl KokoroEngine {
    pub fn load(model_dir: &Path, threads: usize) -> MelResult<Self> {
        let p = |f: &str| Some(model_dir.join(f).to_string_lossy().into_owned());
        let mut cfg = OfflineTtsConfig::default();
        cfg.model.kokoro = OfflineTtsKokoroModelConfig {
            model: p("model.int8.onnx"),
            voices: p("voices.bin"),
            tokens: p("tokens.txt"),
            ..Default::default()
        };
        cfg.model.num_threads = threads.clamp(1, 8) as i32;
        cfg.model.provider = Some("cpu".into());
        let tts = OfflineTts::create(&cfg).ok_or_else(|| {
            MelError::new(
                ErrorCode::EngineBroken,
                "the starter voices need a quick repair",
            )
            .with_fix(crate::error::FixAction::Repair {
                family: "native".into(),
            })
            .with_detail(format!(
                "sherpa-onnx failed to load kokoro from {}",
                model_dir.display()
            ))
        })?;
        let sample_rate = tts.sample_rate() as u32;
        // Warm the G2P so the first Speak is fast.
        let _ = g2p_for("en-us", 0);
        Ok(Self {
            tts: Mutex::new(tts),
            sample_rate,
        })
    }
}

impl Engine for KokoroEngine {
    fn synthesize(
        &self,
        req: &SynthRequest,
        should_stop: &dyn Fn() -> bool,
    ) -> MelResult<SynthOutput> {
        let VoiceTarget::Preset { preset } = &req.voice else {
            return Err(
                MelError::new(ErrorCode::Unsupported, "this voice can't be cloned")
                    .with_detail("kokoro reference"),
            );
        };
        if !g2p::native_language_supported(&req.lang) {
            return Err(MelError::new(
                ErrorCode::Unsupported,
                "this voice only speaks english for now",
            ));
        }
        let sid: i32 = preset
            .parse()
            .map_err(|_| MelError::internal(format!("bad kokoro preset {preset}")))?;
        let phonemes = g2p_for(&req.lang, sid).phonemize(&req.text);
        if phonemes.trim().is_empty() || should_stop() {
            return Ok(SynthOutput {
                pcm: Pcm::new(Vec::new(), self.sample_rate),
                watermarked: false,
            });
        }
        let gen = GenerationConfig {
            sid,
            speed: req.speed.clamp(0.5, 2.0),
            phoneme_codepoints: Some(vec![phonemes.chars().map(|c| c as i32).collect()]),
            ..Default::default()
        };
        let tts = self.tts.lock();
        let audio = tts
            .generate_with_config::<fn(&[f32], f32) -> bool>("", &gen, None)
            .ok_or_else(|| MelError::internal("kokoro generation returned nothing"))?;
        Ok(SynthOutput {
            pcm: Pcm::new(audio.samples().to_vec(), audio.sample_rate() as u32),
            watermarked: false,
        })
    }
}
