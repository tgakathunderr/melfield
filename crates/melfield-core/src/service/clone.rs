//! Clone: capture or import a reference, pick the right engine automatically,
//! enforce consent, store the clone as a voice. Every clone output is
//! watermarked (enforced in `speak.rs` and the workers).

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::voices::VoiceView;
use super::{hf, CoreEvent, Melfield};
use crate::audio::codec;
use crate::audio::dsp::{self, Pcm};
use crate::audio::spectrum::envelope;
use crate::error::{human_bytes, ErrorCode, FixAction, MelError, MelResult};
use crate::hardware::Tier;
use crate::pull;
use crate::registry::{incompatibility, Orb, Recipe};
use crate::store::{now, CloneRecord};

pub const CONSENT_TEXT: &str = "This is my voice, or I have the speaker's permission to clone it.";
const REF_RATE: u32 = 24_000;
const MAX_REF_SECONDS: f32 = 30.0;
const MIN_REF_SECONDS: f32 = 3.0;
const INDIC: &[&str] = &[
    "hi", "bn", "ta", "te", "mr", "gu", "kn", "ml", "pa", "or", "as", "ur", "ne",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingInfo {
    pub id: String,
    pub duration_s: f64,
    pub waveform: Vec<f32>,
    /// A gentle hint ("a few more seconds will sound even better").
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClonePlan {
    pub language: String,
    pub supported: bool,
    pub reason: Option<String>,
    pub recipe_id: Option<String>,
    pub installed: bool,
    pub download_label: String,
    pub needs_sign_in: bool,
    pub gate_url: Option<String>,
    pub min_seconds: f32,
    pub ideal_seconds: f32,
    /// A line to read aloud while recording (its text becomes the transcript).
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRequest {
    pub name: String,
    pub recording_id: String,
    pub language: String,
    /// Transcript of the reference when known (the prompt line, if read).
    pub ref_text: Option<String>,
    pub consent: bool,
}

pub fn prompt_for(lang: &str) -> &'static str {
    match lang {
        "hi" => "नमस्ते! मैं मेलफ़ील्ड के लिए अपनी आवाज़ रिकॉर्ड कर रहा हूँ। मुझे लंबी सैर, गरम चाय और ऐसी कहानियाँ पसंद हैं जिनका अंत अच्छा हो। आज कुछ नया आज़माने के लिए अच्छा दिन है।",
        "es" => "¡Hola! Estoy grabando mi voz para Melfield. Me gustan los paseos largos, el té caliente y las historias que terminan bien. Hoy parece un buen día para probar algo nuevo.",
        "fr" => "Bonjour ! J'enregistre ma voix pour Melfield. J'aime les longues promenades, le thé bien chaud et les histoires qui finissent bien. Aujourd'hui, c'est un bon jour pour essayer quelque chose de nouveau.",
        "de" => "Hallo! Ich nehme meine Stimme für Melfield auf. Ich mag lange Spaziergänge, heißen Tee und Geschichten mit gutem Ende. Heute ist ein guter Tag, um etwas Neues auszuprobieren.",
        "pt" => "Olá! Estou gravando minha voz para o Melfield. Gosto de longas caminhadas, chá quente e histórias que terminam bem. Hoje parece um bom dia para tentar algo novo.",
        "it" => "Ciao! Sto registrando la mia voce per Melfield. Mi piacciono le lunghe passeggiate, il tè caldo e le storie che finiscono bene. Oggi sembra un buon giorno per provare qualcosa di nuovo.",
        _ => "Hi! I'm recording my voice for Melfield. I like long walks, warm tea, and stories that end well. Today feels like a good day to try something new.",
    }
}

/// Clean a raw take into a good reference: mono 24 kHz, rumble removed,
/// silence trimmed, capped at 30 s, levelled.
pub fn prepare_reference(raw: &Pcm) -> MelResult<Pcm> {
    let pcm = dsp::resample(raw, REF_RATE)?;
    let pcm = dsp::high_pass(&pcm, 80.0);
    let pcm = dsp::trim_silence(&pcm, 0.01, 0.15);
    if pcm.rms() < 0.002 {
        return Err(MelError::new(
            ErrorCode::BadAudio,
            "we couldn't hear much — move a little closer to the mic and try again",
        ));
    }
    let max = (MAX_REF_SECONDS * REF_RATE as f32) as usize;
    let pcm = Pcm::new(pcm.samples[..pcm.samples.len().min(max)].to_vec(), REF_RATE);
    if (pcm.duration_s() as f32) < MIN_REF_SECONDS {
        return Err(MelError::new(
            ErrorCode::TooShort,
            "that was a little short — try at least 10 seconds",
        ));
    }
    Ok(dsp::normalize_loudness(&pcm, -20.0, 0.9))
}

/// Deterministic, pleasant orb for a clone, in the green→cyan family of the design's "You".
pub fn clone_orb(name: &str) -> Orb {
    let h = Sha256::digest(name.as_bytes());
    let shift = |base: u8, delta: u8| base.saturating_sub(30).saturating_add(delta % 60);
    Orb {
        c1: [
            shift(124, h[0]),
            shift(242, h[1]).max(180),
            shift(154, h[2]),
        ],
        c2: [shift(63, h[3]), shift(182, h[4]), shift(255, h[5]).max(200)],
        k: 3 + (h[6] % 6) as u32,
        amp: 0.08 + (h[7] % 8) as f32 / 100.0,
        ring: h[8] % 3 == 0,
    }
}

impl Melfield {
    fn choose_clone_recipe(&self, lang: &str) -> Result<Recipe, String> {
        let reg = self.registry.read().clone();
        let ok = |id: &str| {
            reg.recipe(id)
                .filter(|r| {
                    incompatibility(r, &self.hardware).is_none() && r.supports_language(lang)
                })
                .cloned()
        };
        let big_gpu = matches!(self.hardware.tier, Tier::A)
            || (self.hardware.tier == Tier::B && self.hardware.ram_mb >= 15_000);
        let any_gpu = matches!(self.hardware.tier, Tier::A | Tier::AMinus | Tier::B);
        let candidates: Vec<&str> = if INDIC.contains(&lang) {
            vec!["chatterbox-multilingual-v3", "voxcpm2"]
        } else if any_gpu {
            let qwen = if big_gpu {
                "qwen3-tts-1.7b-base"
            } else {
                "qwen3-tts-0.6b-base"
            };
            vec![
                qwen,
                "qwen3-tts-0.6b-base",
                "pocket-tts-clone",
                "chatterbox-multilingual-v3",
            ]
        } else {
            vec!["pocket-tts-clone", "chatterbox-multilingual-v3"]
        };
        candidates.into_iter().find_map(ok).ok_or_else(|| {
            if !self.hardware.python_engines_supported() {
                "cloning needs an apple silicon mac on macos 14 or newer".to_string()
            } else {
                "that language can't be cloned on this computer yet".to_string()
            }
        })
    }

    pub fn clone_plan(&self, language: &str) -> ClonePlan {
        let lang = language
            .split('-')
            .next()
            .unwrap_or(language)
            .to_lowercase();
        let prompt = prompt_for(&lang).to_string();
        match self.choose_clone_recipe(&lang) {
            Ok(r) => {
                let installed = self.recipe_ready(&r);
                let remaining = if installed {
                    0
                } else {
                    (pull::remaining_bytes(&self.paths, &r) + self.engine_setup_bytes(&r)).max(1)
                };
                ClonePlan {
                    language: lang,
                    supported: true,
                    reason: None,
                    recipe_id: Some(r.id.clone()),
                    installed,
                    download_label: if installed {
                        String::new()
                    } else {
                        human_bytes(remaining)
                    },
                    needs_sign_in: r.gate.is_some() && hf::load_token().is_none(),
                    gate_url: r.gate.as_ref().map(|g| g.url.clone()),
                    min_seconds: r.capabilities.clone_ref_seconds.min.max(MIN_REF_SECONDS),
                    ideal_seconds: r.capabilities.clone_ref_seconds.ideal.max(10.0),
                    prompt,
                }
            }
            Err(reason) => ClonePlan {
                language: lang,
                supported: false,
                reason: Some(reason),
                recipe_id: None,
                installed: false,
                download_label: String::new(),
                needs_sign_in: false,
                gate_url: None,
                min_seconds: MIN_REF_SECONDS,
                ideal_seconds: 10.0,
                prompt,
            },
        }
    }

    fn store_recording(&self, pcm: &Pcm) -> MelResult<RecordingInfo> {
        let prepared = prepare_reference(pcm)?;
        let id = uuid::Uuid::new_v4().to_string();
        let path = self.paths.tmp().join(format!("ref-{id}.wav"));
        codec::write_wav(&path, &prepared)?;
        self.recordings.lock().insert(id.clone(), path);
        let secs = prepared.duration_s();
        Ok(RecordingInfo {
            id,
            duration_s: secs,
            waveform: envelope(&prepared.samples, 40),
            hint: (secs < 8.0).then(|| "a few more seconds will sound even better".to_string()),
        })
    }

    /// Import a dropped file (wav/mp3/m4a/flac/ogg).
    pub fn import_audio(&self, path: &Path) -> MelResult<RecordingInfo> {
        let raw = codec::decode_file(path)?;
        self.store_recording(&raw)
    }

    #[cfg(feature = "audio-io")]
    pub fn start_recording(&self) -> MelResult<()> {
        let mut slot = self.recording.lock();
        if slot.is_some() {
            return Ok(());
        }
        let sink = self.sink.clone();
        *slot = Some(crate::audio::recorder::Recording::start(Arc::new(
            move |level, seconds| sink.emit(CoreEvent::RecordLevel { level, seconds }),
        ))?);
        Ok(())
    }

    #[cfg(feature = "audio-io")]
    pub fn stop_recording(&self) -> MelResult<RecordingInfo> {
        let rec = self
            .recording
            .lock()
            .take()
            .ok_or_else(|| MelError::new(ErrorCode::NotFound, "nothing was recording"))?;
        let raw = rec.stop();
        self.store_recording(&raw)
    }

    /// Play back a take before creating the clone.
    #[cfg(feature = "audio-io")]
    pub fn play_recording(&self, recording_id: &str) -> MelResult<()> {
        let path = self
            .recordings
            .lock()
            .get(recording_id)
            .cloned()
            .ok_or_else(|| {
                MelError::new(ErrorCode::NotFound, "that take is gone — record again")
            })?;
        self.play_file(&format!("take:{recording_id}"), &path)
    }

    pub fn create_clone(self: &Arc<Self>, req: CloneRequest) -> MelResult<VoiceView> {
        if !req.consent {
            return Err(MelError::new(
                ErrorCode::ConsentRequired,
                "turn on the permission switch first",
            ));
        }
        let name = req.name.trim();
        if name.is_empty() {
            return Err(MelError::new(
                ErrorCode::Unsupported,
                "give this voice a name",
            ));
        }
        if name.chars().count() > 40 {
            return Err(MelError::new(
                ErrorCode::Unsupported,
                "that name is a bit long — keep it under 40 characters",
            ));
        }
        let lang = req
            .language
            .split('-')
            .next()
            .unwrap_or("en")
            .to_lowercase();
        let recipe = self
            .choose_clone_recipe(&lang)
            .map_err(|r| MelError::new(ErrorCode::Unsupported, r))?;
        if !self.recipe_ready(&recipe) {
            return Err(MelError::new(
                ErrorCode::VoiceNotInstalled,
                "cloning needs a one-time download first",
            )
            .with_fix(FixAction::InstallRecipe {
                recipe_id: recipe.id.clone(),
            }));
        }
        let src = self
            .recordings
            .lock()
            .get(&req.recording_id)
            .cloned()
            .ok_or_else(|| {
                MelError::new(ErrorCode::NotFound, "that take is gone — record again")
            })?;
        let voice_id = format!("clone-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        let dir = self.paths.clone_dir(&voice_id);
        std::fs::create_dir_all(&dir)?;
        let ref_path = dir.join("ref.wav");
        std::fs::copy(&src, &ref_path)?;
        let ref_text = req
            .ref_text
            .as_ref()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        let record = CloneRecord {
            voice_id: voice_id.clone(),
            name: name.to_string(),
            recipe_id: recipe.id.clone(),
            language: lang,
            ref_audio: ref_path.display().to_string(),
            ref_text,
            orb: clone_orb(name),
            consent_at: now(),
            consent_text: CONSENT_TEXT.to_string(),
            consent_app_version: self.config.app_version.clone(),
            watermark: "perth".into(),
            created_at: now(),
        };
        std::fs::write(dir.join("meta.json"), serde_json::to_vec_pretty(&record)?)?;
        self.store.put_clone(&record)?;
        let _ = std::fs::remove_file(&src);
        self.recordings.lock().remove(&req.recording_id);
        self.emit(CoreEvent::VoicesChanged);
        self.voice(&voice_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speechish(secs: f32) -> Pcm {
        let rate = 48_000;
        let n = (secs * rate as f32) as usize;
        let mut s = vec![0.0; rate as usize / 2];
        s.extend((0..n).map(|i| 0.2 * ((i as f32 * 0.03).sin() + 0.5 * (i as f32 * 0.011).sin())));
        s.extend(vec![0.0; rate as usize / 2]);
        Pcm::new(s, rate)
    }

    #[test]
    fn prepares_reference_and_caps_length() {
        let p = prepare_reference(&speechish(12.0)).unwrap();
        assert_eq!(p.sample_rate, REF_RATE);
        assert!((p.duration_s() - 12.3).abs() < 0.3, "{}", p.duration_s());
        let long = prepare_reference(&speechish(45.0)).unwrap();
        assert!(long.duration_s() <= 30.01);
    }

    #[test]
    fn rejects_short_and_silent_takes() {
        assert_eq!(
            prepare_reference(&speechish(1.5)).unwrap_err().code,
            ErrorCode::TooShort
        );
        assert_eq!(
            prepare_reference(&Pcm::new(vec![0.0; 48_000 * 5], 48_000))
                .unwrap_err()
                .code,
            ErrorCode::BadAudio
        );
    }

    #[test]
    fn clone_orbs_are_stable_and_distinct() {
        assert_eq!(clone_orb("Sam").c1, clone_orb("Sam").c1);
        let (a, b) = (clone_orb("Sam"), clone_orb("Priya"));
        assert!(a.c1 != b.c1 || a.k != b.k);
    }

    #[test]
    fn prompts_exist_for_hindi_and_fallback() {
        assert!(prompt_for("hi").contains('।'));
        assert!(prompt_for("xx").starts_with("Hi!"));
    }
}
