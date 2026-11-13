//! Speak: chunk → synthesize → stream to speakers → stitch → save to history.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::voices::VoiceView;
use super::{CoreEvent, Melfield};
use crate::audio::codec::{self, Format};
use crate::audio::dsp::{self, Pcm};
use crate::audio::spectrum::envelope;
use crate::engines::envs::EnvManager;
use crate::engines::{Delivery, SynthOutput, SynthRequest, VoiceTarget};
use crate::error::{ErrorCode, MelError, MelResult};
use crate::pull::CancelToken;
use crate::registry::{EngineSpec, Recipe};
use crate::store::{now, Generation};
use crate::text;

/// Output rate for stitched/saved audio (all engines are resampled to this).
pub const OUTPUT_RATE: u32 = 24_000;
const TARGET_LUFS: f64 = -16.0;
const PEAK_CEILING: f32 = 0.89;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakRequest {
    pub text: String,
    pub voice_id: String,
    #[serde(default = "one")]
    pub speed: f32,
    #[serde(default)]
    pub delivery: Delivery,
    /// Override the language (else detected from script / voice default).
    #[serde(default)]
    pub language: Option<String>,
    /// Play through the speakers while generating (desktop). API/CLI set false.
    #[serde(default = "yes")]
    pub play: bool,
}

fn one() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Started,
    Progress,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationView {
    pub id: String,
    pub voice_id: String,
    pub voice_name: String,
    pub text: String,
    pub duration_s: f64,
    pub created_at: String,
    pub favourite: bool,
    pub watermarked: bool,
    /// 28 columns of 0..1 for the dot waveform.
    pub waveform: Vec<f32>,
    pub orb: Option<crate::registry::Orb>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEvent {
    pub job_id: String,
    pub state: JobState,
    pub chunks_done: usize,
    pub chunks_total: usize,
    pub generation: Option<GenerationView>,
    pub error: Option<MelError>,
}

struct Resolved {
    voice: VoiceView,
    recipe: Recipe,
    target: VoiceTarget,
    lang: String,
    clone: bool,
}

impl Melfield {
    fn resolve(&self, req: &SpeakRequest) -> MelResult<Resolved> {
        let voice = self.voice(&req.voice_id)?;
        let recipe = self
            .registry
            .read()
            .recipe(&voice.recipe_id)
            .cloned()
            .ok_or_else(|| MelError::internal("recipe vanished"))?;
        self.ensure_ready(&recipe, &voice.name, &voice.id)?;
        let detected = text::detect_script_language(&req.text);
        let lang = req
            .language
            .clone()
            .or(detected.map(String::from))
            .unwrap_or_else(|| voice.language.clone());
        let speaks = voice.languages.iter().any(|l| l == &lang)
            || (voice.kind == "clone" && recipe.supports_language(&lang));
        if !speaks {
            let suggestion = self
                .voices()?
                .into_iter()
                .find(|v| v.installed && v.languages.iter().any(|l| l == &lang))
                .map(|v| format!(" — try {}", v.name.to_lowercase()))
                .unwrap_or_default();
            return Err(MelError::new(
                ErrorCode::Unsupported,
                format!(
                    "{} doesn't speak that language yet{suggestion}",
                    voice.name.to_lowercase()
                ),
            ));
        }
        let (target, clone) = if voice.kind == "clone" {
            let rec = self
                .store
                .clone_record(&voice.id)?
                .ok_or_else(|| MelError::internal("clone record missing"))?;
            (
                VoiceTarget::Reference {
                    ref_audio: PathBuf::from(rec.ref_audio),
                    ref_text: rec.ref_text,
                },
                true,
            )
        } else {
            let preset = self
                .registry
                .read()
                .voice(&voice.id)
                .map(|v| v.preset.clone())
                .unwrap_or_default();
            (VoiceTarget::Preset { preset }, false)
        };
        Ok(Resolved {
            voice,
            recipe,
            target,
            lang,
            clone,
        })
    }

    pub(crate) fn synth_chunk(
        &self,
        recipe: &Recipe,
        req: &SynthRequest,
        stop: &dyn Fn() -> bool,
    ) -> MelResult<SynthOutput> {
        match &recipe.engine {
            EngineSpec::Native { .. } => self.native_engine(recipe)?.synthesize(req, stop),
            EngineSpec::Python { .. } => {
                let device = self.hardware.python_device(self.prefer_cpu());
                let backend = EnvManager::backend_for(&self.hardware, self.prefer_cpu());
                self.supervisor
                    .synthesize(recipe, device, backend, req, stop)
            }
        }
    }

    /// Start a speak job. Returns the job id immediately; progress via events.
    pub fn speak(self: &Arc<Self>, req: SpeakRequest) -> MelResult<String> {
        let trimmed = req.text.trim();
        if trimmed.is_empty() {
            return Err(MelError::new(ErrorCode::EmptyText, "type something first"));
        }
        if trimmed.chars().count() > text::MAX_TEXT_CHARS {
            return Err(MelError::new(
                ErrorCode::TooLong,
                "that's a lot of text — split it into a few parts",
            ));
        }
        let resolved = self.resolve(&req)?;
        let job_id = uuid::Uuid::new_v4().to_string();
        let cancel = CancelToken::default();
        self.jobs.lock().insert(job_id.clone(), cancel.clone());
        let me = self.clone();
        let jid = job_id.clone();
        self.rt.spawn_blocking(move || {
            let result = me.run_job(&jid, &req, &resolved, &cancel);
            me.jobs.lock().remove(&jid);
            let (state, error, generation) = match result {
                Ok(g) => (JobState::Done, None, Some(g)),
                Err(e) if e.code == ErrorCode::Cancelled => (JobState::Cancelled, None, None),
                Err(e) => (JobState::Failed, Some(e), None),
            };
            me.emit(CoreEvent::Job(JobEvent {
                job_id: jid,
                state,
                chunks_done: 0,
                chunks_total: 0,
                generation,
                error,
            }));
        });
        Ok(job_id)
    }

    /// Blocking synthesis of a whole text into one PCM (API/CLI path).
    pub fn synthesize_blocking(
        &self,
        req: &SpeakRequest,
        cancel: &CancelToken,
    ) -> MelResult<(Pcm, bool)> {
        let resolved = self.resolve(req)?;
        let mut stitched: Vec<f32> = Vec::new();
        let mut watermarked = true;
        for chunk in text::chunk(&req.text, resolved.recipe.capabilities.max_chunk_chars) {
            if cancel.is_cancelled() {
                return Err(MelError::cancelled());
            }
            let (pcm, wm) = self.render_chunk(&resolved, req, &chunk.text, cancel)?;
            watermarked &= wm;
            dsp::stitch(
                &mut stitched,
                &pcm.samples,
                OUTPUT_RATE,
                if chunk.index == 0 { 0.0 } else { 0.12 },
                0.02,
            );
        }
        Ok((
            Pcm::new(stitched, OUTPUT_RATE),
            resolved.clone && watermarked,
        ))
    }

    fn render_chunk(
        &self,
        r: &Resolved,
        req: &SpeakRequest,
        text: &str,
        cancel: &CancelToken,
    ) -> MelResult<(Pcm, bool)> {
        let synth_req = SynthRequest {
            recipe_id: r.recipe.id.clone(),
            text: text.to_string(),
            lang: r.lang.clone(),
            voice: r.target.clone(),
            speed: req.speed,
            delivery: req.delivery,
            watermark: r.clone,
        };
        let out = self.synth_chunk(&r.recipe, &synth_req, &|| cancel.is_cancelled())?;
        if cancel.is_cancelled() {
            return Err(MelError::cancelled());
        }
        if r.clone && !out.watermarked {
            return Err(MelError::new(
                ErrorCode::WatermarkUnavailable,
                "cloned voices need their safety watermark — repair this voice to restore it",
            ));
        }
        let pcm = dsp::resample(&out.pcm, OUTPUT_RATE)?;
        let pcm = dsp::trim_silence(&pcm, 0.003, 0.06);
        Ok((
            dsp::normalize_loudness(&pcm, TARGET_LUFS, PEAK_CEILING),
            out.watermarked,
        ))
    }

    fn run_job(
        &self,
        job_id: &str,
        req: &SpeakRequest,
        r: &Resolved,
        cancel: &CancelToken,
    ) -> MelResult<GenerationView> {
        let chunks = text::chunk(&req.text, r.recipe.capabilities.max_chunk_chars);
        let total = chunks.len();
        let emit = |state: JobState, done: usize| {
            self.emit(CoreEvent::Job(JobEvent {
                job_id: job_id.into(),
                state,
                chunks_done: done,
                chunks_total: total,
                generation: None,
                error: None,
            }));
        };
        emit(JobState::Started, 0);
        #[cfg(feature = "audio-io")]
        let player = if req.play && !self.config.headless {
            self.player().ok()
        } else {
            None
        };
        #[cfg(feature = "audio-io")]
        if let Some(p) = &player {
            p.begin(job_id, true);
        }
        let mut stitched: Vec<f32> = Vec::new();
        let mut watermarked = true;
        for chunk in &chunks {
            let (pcm, wm) = match self.render_chunk(r, req, &chunk.text, cancel) {
                Ok(v) => v,
                Err(e) => {
                    #[cfg(feature = "audio-io")]
                    if let Some(p) = &player {
                        p.end_stream();
                    }
                    return Err(e);
                }
            };
            watermarked &= wm;
            let gap = if chunk.index == 0 {
                0.0
            } else if chunks[chunk.index - 1].paragraph_end {
                0.45
            } else {
                0.12
            };
            let before = stitched.len();
            dsp::stitch(&mut stitched, &pcm.samples, OUTPUT_RATE, gap, 0.02);
            // The crossfade lands inside the gap (gap >= fade), so everything before
            // `before` is final and only the new tail needs streaming.
            #[cfg(feature = "audio-io")]
            if let Some(p) = &player {
                let _ = p.append(&Pcm::new(stitched[before..].to_vec(), OUTPUT_RATE));
            }
            #[cfg(not(feature = "audio-io"))]
            let _ = before;
            emit(JobState::Progress, chunk.index + 1);
        }
        #[cfg(feature = "audio-io")]
        if let Some(p) = &player {
            p.end_stream();
        }
        let pcm = Pcm::new(stitched, OUTPUT_RATE);
        self.save_generation(req, r, pcm, r.clone && watermarked)
    }

    fn save_generation(
        &self,
        req: &SpeakRequest,
        r: &Resolved,
        pcm: Pcm,
        watermarked: bool,
    ) -> MelResult<GenerationView> {
        let id = uuid::Uuid::new_v4().to_string();
        let month = chrono::Utc::now().format("%Y-%m").to_string();
        let dir = self.paths.history_audio().join(month);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{id}.wav"));
        codec::write_wav(&path, &pcm)?;
        let g = Generation {
            id: id.clone(),
            voice_id: r.voice.id.clone(),
            voice_name: r.voice.name.clone(),
            text: req.text.clone(),
            settings: serde_json::json!({"speed": req.speed, "delivery": req.delivery, "language": r.lang}),
            duration_s: pcm.duration_s(),
            audio_path: path.display().to_string(),
            sample_rate: pcm.sample_rate,
            watermarked,
            recipe_id: r.recipe.id.clone(),
            favourite: false,
            created_at: now(),
        };
        if self.settings()?.keep_history {
            self.store.put_generation(&g)?;
        }
        Ok(self.generation_view(&g, Some(&pcm)))
    }

    pub(crate) fn generation_view(&self, g: &Generation, pcm: Option<&Pcm>) -> GenerationView {
        let waveform = match pcm {
            Some(p) => envelope(&p.samples, 28),
            None => codec::read_wav(Path::new(&g.audio_path))
                .map(|p| envelope(&p.samples, 28))
                .unwrap_or_else(|_| vec![0.0; 28]),
        };
        let orb = self.voice(&g.voice_id).ok().map(|v| v.orb);
        GenerationView {
            id: g.id.clone(),
            voice_id: g.voice_id.clone(),
            voice_name: g.voice_name.clone(),
            text: g.text.clone(),
            duration_s: g.duration_s,
            created_at: g.created_at.clone(),
            favourite: g.favourite,
            watermarked: g.watermarked,
            waveform,
            orb,
        }
    }

    pub fn cancel_job(&self, job_id: &str) {
        if let Some(c) = self.jobs.lock().get(job_id) {
            c.cancel();
        }
        #[cfg(feature = "audio-io")]
        if let Some(p) = self.player.lock().as_ref() {
            if p.current_id() == job_id {
                p.stop();
            }
        }
    }

    // ---- history ----
    pub fn history(
        &self,
        query: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> MelResult<Vec<GenerationView>> {
        Ok(self
            .store
            .generations(query, limit.min(200), offset)?
            .iter()
            .map(|g| self.generation_view(g, None))
            .collect())
    }

    pub fn delete_generation(&self, id: &str) -> MelResult<()> {
        if let Some(path) = self.store.delete_generation(id)? {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    pub fn set_favourite(&self, id: &str, favourite: bool) -> MelResult<()> {
        self.store.set_favourite(id, favourite)
    }

    fn generation_pcm(&self, id: &str) -> MelResult<(Generation, Pcm)> {
        let g = self.store.generation(id)?.ok_or_else(|| {
            MelError::new(
                ErrorCode::NotFound,
                "that clip isn't in your history anymore",
            )
        })?;
        let pcm = codec::read_wav(Path::new(&g.audio_path)).map_err(|e| {
            MelError::new(ErrorCode::NotFound, "that clip's audio file is missing")
                .with_detail(e.detail)
        })?;
        Ok((g, pcm))
    }

    /// Save a generation somewhere the person chose.
    pub fn export_generation(&self, id: &str, dest: &Path, format: Format) -> MelResult<()> {
        let (_, pcm) = self.generation_pcm(id)?;
        codec::save(dest, &pcm, format)
    }

    // ---- playback ----
    #[cfg(feature = "audio-io")]
    pub(crate) fn player(&self) -> MelResult<Arc<crate::audio::player::Player>> {
        let mut slot = self.player.lock();
        if let Some(p) = slot.as_ref() {
            return Ok(p.clone());
        }
        let sink = self.sink.clone();
        let p = Arc::new(crate::audio::player::Player::new(Arc::new(move |f| {
            sink.emit(CoreEvent::Player(f))
        }))?);
        *slot = Some(p.clone());
        Ok(p)
    }

    #[cfg(feature = "audio-io")]
    pub fn play_generation(&self, id: &str) -> MelResult<()> {
        let (g, pcm) = self.generation_pcm(id)?;
        self.player()?.play(&g.id, &pcm)
    }

    #[cfg(feature = "audio-io")]
    pub fn play_file(&self, id: &str, path: &Path) -> MelResult<()> {
        let pcm = codec::decode_file(path)?;
        self.player()?.play(id, &pcm)
    }

    #[cfg(feature = "audio-io")]
    pub fn player_control(&self, action: &str, seconds: Option<f64>) -> MelResult<()> {
        let p = self.player()?;
        match action {
            "pause" => p.pause(),
            "resume" => p.resume(),
            "stop" => p.stop(),
            "seek" => p.seek(seconds.unwrap_or(0.0)),
            _ => {
                return Err(MelError::internal(format!(
                    "unknown player action {action}"
                )))
            }
        }
        Ok(())
    }

    /// Hover-to-hear: bundled pre-rendered sample, else a short live line for
    /// installed native voices.
    #[cfg(feature = "audio-io")]
    pub fn preview_voice(self: &Arc<Self>, voice_id: &str) -> MelResult<()> {
        let preview_id = format!("preview:{voice_id}");
        if let Some(path) = self.sample_path(voice_id) {
            return self.play_file(&preview_id, &path);
        }
        let v = self.voice(voice_id)?;
        if !v.installed {
            return Err(MelError::new(
                ErrorCode::VoiceNotInstalled,
                format!("get {} to hear it", v.name.to_lowercase()),
            ));
        }
        let line = format!("Hi, I'm {}. What should I say?", v.name);
        let req = SpeakRequest {
            text: line,
            voice_id: voice_id.into(),
            speed: 1.0,
            delivery: Delivery::Natural,
            language: None,
            play: true,
        };
        let me = self.clone();
        self.rt.spawn_blocking(move || {
            match me.synthesize_blocking(&req, &CancelToken::default()) {
                Ok((pcm, _)) => {
                    if let Ok(p) = me.player() {
                        let _ = p.play(&preview_id, &pcm);
                    }
                }
                Err(e) => tracing::warn!(detail = %e.detail, "preview failed"),
            }
        });
        Ok(())
    }
}
