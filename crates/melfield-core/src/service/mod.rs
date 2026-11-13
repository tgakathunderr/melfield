//! `Melfield`: the one object the desktop app, local API and CLI talk to.
//! Everything here is safe to call from any thread; long work runs on tasks
//! and reports through the `EventSink`.

mod clone;
mod hf;
mod settings;
mod speak;
mod storage;
mod voices;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde::Serialize;

pub use clone::{ClonePlan, CloneRequest, RecordingInfo};
pub use settings::{Compute, Settings};
pub use speak::{GenerationView, JobEvent, JobState, SpeakRequest};
pub use storage::DiskUsage;
pub use voices::{EngineStatus, LicenseView, VoiceView};

use crate::engines::envs::{EnvManager, EnvProgress};
use crate::engines::supervisor::Supervisor;
use crate::engines::Engine;
use crate::error::MelResult;
use crate::hardware::{self, HardwareInfo};
use crate::paths::Paths;
use crate::pull::{CancelToken, PullProgress};
use crate::registry::Registry;
use crate::store::Store;

/// Everything the UI can be told about, as one tagged enum.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)] // events are short-lived; boxing adds noise at every emit site
pub enum CoreEvent {
    Pull(PullProgress),
    Env(EnvProgress),
    Job(JobEvent),
    #[cfg(feature = "audio-io")]
    Player(crate::audio::player::PlayerFrame),
    RecordLevel {
        level: f32,
        seconds: f64,
    },
    VoicesChanged,
    SettingsChanged,
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: CoreEvent);
}

/// A sink that drops everything (CLI, tests).
pub struct NullSink;
impl EventSink for NullSink {
    fn emit(&self, _event: CoreEvent) {}
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Data root; `None` = platform default (honouring a moved-storage pointer).
    pub root: Option<PathBuf>,
    /// Bundled resources: `workers/` (Python adapters) and `samples/` (previews).
    pub resources_dir: PathBuf,
    /// Bundled `uv` binary.
    pub uv_path: PathBuf,
    pub app_version: String,
    /// Disable native audio devices (headless API/CLI/tests).
    pub headless: bool,
}

pub struct Melfield {
    pub(crate) config: Config,
    pub(crate) paths: Paths,
    pub(crate) store: Store,
    pub(crate) registry: RwLock<Registry>,
    pub(crate) hardware: HardwareInfo,
    pub(crate) http: reqwest::Client,
    pub(crate) sink: Arc<dyn EventSink>,
    pub(crate) supervisor: Supervisor,
    pub(crate) native: Mutex<HashMap<String, Arc<dyn Engine>>>,
    pub(crate) pulls: Mutex<HashMap<String, CancelToken>>,
    pub(crate) jobs: Mutex<HashMap<String, CancelToken>>,
    pub(crate) recordings: Mutex<HashMap<String, PathBuf>>,
    #[cfg(feature = "audio-io")]
    pub(crate) player: Mutex<Option<Arc<crate::audio::player::Player>>>,
    #[cfg(feature = "audio-io")]
    pub(crate) recording: Mutex<Option<crate::audio::recorder::Recording>>,
    pub(crate) rt: tokio::runtime::Handle,
}

impl Melfield {
    /// Must be called from within a tokio runtime.
    pub fn new(config: Config, sink: Arc<dyn EventSink>) -> MelResult<Arc<Self>> {
        let root = match &config.root {
            Some(r) => r.clone(),
            None => storage::resolve_root()?,
        };
        let paths = Paths::new(root)?;
        let store = Store::open(&paths.db())?;
        let registry = Registry::load(&paths.registry_cache())?;
        let hardware = hardware::probe();
        let http = reqwest::Client::builder()
            .user_agent(concat!("melfield/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(crate::error::MelError::internal)?;
        let envs = EnvManager::new(
            config.uv_path.clone(),
            config.resources_dir.join("workers"),
            paths.clone(),
        );
        let supervisor = Supervisor::new(envs, paths.clone());
        let me = Arc::new(Self {
            config,
            paths,
            store,
            registry: RwLock::new(registry),
            hardware,
            http,
            sink,
            supervisor,
            native: Mutex::new(HashMap::new()),
            pulls: Mutex::new(HashMap::new()),
            jobs: Mutex::new(HashMap::new()),
            recordings: Mutex::new(HashMap::new()),
            #[cfg(feature = "audio-io")]
            player: Mutex::new(None),
            #[cfg(feature = "audio-io")]
            recording: Mutex::new(None),
            rt: tokio::runtime::Handle::current(),
        });
        me.supervisor.set_hf_token(hf::load_token());
        me.reconcile_installs();
        me.spawn_housekeeping();
        Ok(me)
    }

    pub fn hardware(&self) -> &HardwareInfo {
        &self.hardware
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub(crate) fn emit(&self, e: CoreEvent) {
        self.sink.emit(e);
    }

    fn spawn_housekeeping(self: &Arc<Self>) {
        let me = Arc::downgrade(self);
        self.rt.spawn(async move {
            // Refresh the signed registry once at startup (no-op until keys exist).
            if let Some(m) = me.upgrade() {
                if let Ok(Some(reg)) = Registry::refresh(&m.paths.registry_cache(), &m.http).await {
                    *m.registry.write() = reg;
                    m.emit(CoreEvent::VoicesChanged);
                }
            }
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                let Some(m) = me.upgrade() else { break };
                m.supervisor.reap_idle();
            }
        });
    }

    /// Clean shutdown: stop workers, stop audio.
    pub fn shutdown(&self) {
        self.supervisor.shutdown_all();
        #[cfg(feature = "audio-io")]
        if let Some(p) = self.player.lock().as_ref() {
            p.stop();
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub first_run: bool,
    pub hardware: HardwareInfo,
    pub settings: Settings,
    pub app_version: String,
    pub hf_signed_in: bool,
    pub data_dir: String,
}

impl Melfield {
    pub fn app_state(&self) -> MelResult<AppState> {
        let settings = self.settings()?;
        Ok(AppState {
            first_run: !settings.onboarded || !self.any_voice_installed(),
            hardware: self.hardware.clone(),
            settings,
            app_version: self.config.app_version.clone(),
            hf_signed_in: hf::load_token().is_some(),
            data_dir: self.paths.root().display().to_string(),
        })
    }
}
