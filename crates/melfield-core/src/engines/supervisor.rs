//! Supervisor: lazily starts one worker per engine family, restarts a crashed
//! worker once, marks the family "needs repair" after a second failure, and
//! shuts idle workers down to give memory back.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use serde_json::{json, Value};

use super::envs::{venv_python, Backend, EnvManager};
use super::protocol::f32le_to_vec;
use super::worker::{WorkerProcess, WorkerSpec};
use super::{SynthOutput, SynthRequest, VoiceTarget};
use crate::audio::Pcm;
use crate::error::{ErrorCode, MelError, MelResult};
use crate::hardware::Device;
use crate::paths::Paths;
use crate::pull::{hf_cache_root, snapshot_dir};
use crate::registry::{EngineSpec, Recipe};

pub const IDLE_SHUTDOWN: Duration = Duration::from_secs(600);

struct Slot {
    worker: Option<WorkerProcess>,
    last_used: Instant,
    failures: u32,
}

pub struct Supervisor {
    pub envs: EnvManager,
    paths: Paths,
    slots: Mutex<HashMap<String, Arc<Mutex<Slot>>>>,
    hf_token: RwLock<Option<String>>,
}

impl Supervisor {
    pub fn new(envs: EnvManager, paths: Paths) -> Self {
        Self {
            envs,
            paths,
            slots: Mutex::new(HashMap::new()),
            hf_token: RwLock::new(None),
        }
    }

    pub fn set_hf_token(&self, token: Option<String>) {
        *self.hf_token.write() = token;
    }

    fn slot(&self, key: &str) -> Arc<Mutex<Slot>> {
        self.slots
            .lock()
            .entry(key.to_string())
            .or_insert_with(|| {
                Arc::new(Mutex::new(Slot {
                    worker: None,
                    last_used: Instant::now(),
                    failures: 0,
                }))
            })
            .clone()
    }

    fn spec(&self, family: &str, backend: Backend) -> WorkerSpec {
        WorkerSpec {
            family: family.to_string(),
            python: venv_python(&self.envs.env_dir(family, backend)),
            workers_dir: self.envs.workers_dir.clone(),
            hf_cache: hf_cache_root(&self.paths),
            log_file: self.paths.logs().join(format!("engine-{family}.log")),
            hf_token: self.hf_token.read().clone(),
        }
    }

    fn load_request(&self, recipe: &Recipe, device: Device) -> MelResult<Value> {
        let EngineSpec::Python { family, entry, .. } = &recipe.engine else {
            return Err(MelError::internal("native recipe sent to supervisor"));
        };
        let artifacts: Vec<Value> = recipe
            .artifacts
            .iter()
            .map(|a| json!({"repo": a.repo, "revision": a.revision, "dir": snapshot_dir(&self.paths, a)}))
            .collect();
        Ok(json!({
            "op": "load", "family": family, "entry": entry, "recipe": recipe.id,
            "artifacts": artifacts, "device": device, "sample_rate": recipe.sample_rate,
        }))
    }

    /// Synthesize one chunk on a Python engine.
    pub fn synthesize(
        &self,
        recipe: &Recipe,
        device: Device,
        backend: Backend,
        req: &SynthRequest,
        should_stop: &dyn Fn() -> bool,
    ) -> MelResult<SynthOutput> {
        let EngineSpec::Python { family, python, .. } = &recipe.engine else {
            return Err(MelError::internal("native recipe sent to supervisor"));
        };
        if !self.envs.is_ready(family, backend, python) {
            return Err(MelError::new(
                ErrorCode::EngineStarting,
                "this voice is still getting ready",
            )
            .with_detail(format!("{family} env not ready")));
        }
        let slot = self.slot(family);
        let mut slot = slot.lock();
        slot.last_used = Instant::now();
        let mut attempts = 0;
        loop {
            attempts += 1;
            match self.try_synth(&mut slot, family, backend, recipe, device, req, should_stop) {
                Ok(out) => {
                    slot.failures = 0;
                    return Ok(out);
                }
                Err(e) if e.code == ErrorCode::EngineCrashed && attempts < 2 => {
                    tracing::warn!(family, detail = %e.detail, "worker crashed; restarting once");
                    slot.worker = None;
                }
                Err(e) if e.code == ErrorCode::EngineCrashed => {
                    slot.worker = None;
                    slot.failures += 1;
                    return Err(MelError::engine_broken(family).with_detail(e.detail));
                }
                Err(e) => return Err(e),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn try_synth(
        &self,
        slot: &mut Slot,
        family: &str,
        backend: Backend,
        recipe: &Recipe,
        device: Device,
        req: &SynthRequest,
        should_stop: &dyn Fn() -> bool,
    ) -> MelResult<SynthOutput> {
        if slot.worker.as_mut().is_some_and(|w| !w.is_alive()) {
            slot.worker = None;
        }
        if slot.worker.is_none() {
            slot.worker = Some(WorkerProcess::spawn(&self.spec(family, backend))?);
        }
        let worker = slot
            .worker
            .as_mut()
            .ok_or_else(|| MelError::internal("worker vanished"))?;
        if worker.loaded.as_deref() != Some(recipe.id.as_str()) {
            worker.call(self.load_request(recipe, device)?, &mut |_| !should_stop())?;
            worker.loaded = Some(recipe.id.clone());
        }
        let voice = match &req.voice {
            VoiceTarget::Preset { preset } => json!({"preset": preset}),
            VoiceTarget::Reference {
                ref_audio,
                ref_text,
            } => json!({"ref_audio": ref_audio, "ref_text": ref_text}),
        };
        let mut samples: Vec<f32> = Vec::new();
        let mut rate = recipe.sample_rate;
        let done = worker.call(
            json!({
                "op": "synthesize", "text": req.text, "lang": req.lang, "voice": voice,
                "speed": req.speed, "delivery": req.delivery, "watermark": req.watermark, "stream": true,
            }),
            &mut |m| {
                if let Some(bin) = &m.binary {
                    samples.extend(f32le_to_vec(bin));
                    rate = m.header.get("sample_rate").and_then(Value::as_u64).unwrap_or(rate as u64) as u32;
                }
                !should_stop()
            },
        )?;
        let watermarked = done
            .header
            .get("watermarked")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if req.watermark && !watermarked {
            return Err(MelError::new(
                ErrorCode::WatermarkUnavailable,
                "cloned voices need their safety watermark — repair this voice to restore it",
            )
            .with_fix(crate::error::FixAction::Repair {
                family: family.to_string(),
            })
            .with_detail("worker returned clone audio without watermark"));
        }
        Ok(SynthOutput {
            pcm: Pcm::new(samples, rate),
            watermarked,
        })
    }

    /// Run a non-synthesis op (e.g. "transcribe", "health") on a family's worker.
    pub fn call(&self, family: &str, backend: Backend, req: Value) -> MelResult<Value> {
        let slot = self.slot(family);
        let mut slot = slot.lock();
        if slot.worker.as_mut().is_none_or(|w| !w.is_alive()) {
            slot.worker = Some(WorkerProcess::spawn(&self.spec(family, backend))?);
        }
        slot.last_used = Instant::now();
        let worker = slot
            .worker
            .as_mut()
            .ok_or_else(|| MelError::internal("worker vanished"))?;
        Ok(worker.call(req, &mut |_| true)?.header)
    }

    /// Shut down workers idle longer than `IDLE_SHUTDOWN`. Call periodically.
    pub fn reap_idle(&self) {
        let slots: Vec<_> = self.slots.lock().values().cloned().collect();
        for slot in slots {
            if let Some(mut s) = slot.try_lock() {
                if s.worker.is_some() && s.last_used.elapsed() > IDLE_SHUTDOWN {
                    if let Some(w) = s.worker.take() {
                        tracing::info!(family = %w.family, "shutting down idle worker");
                        w.shutdown();
                    }
                }
            }
        }
    }

    pub fn shutdown_all(&self) {
        let slots: Vec<_> = self.slots.lock().values().cloned().collect();
        for slot in slots {
            if let Some(w) = slot.lock().worker.take() {
                w.shutdown();
            }
        }
    }

    pub fn running_families(&self) -> Vec<String> {
        self.slots
            .lock()
            .iter()
            .filter(|(_, s)| s.try_lock().is_some_and(|s| s.worker.is_some()))
            .map(|(k, _)| k.clone())
            .collect()
    }
}
