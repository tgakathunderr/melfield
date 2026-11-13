use std::sync::Arc;

use serde::Serialize;

use super::{hf, CoreEvent, Melfield};
use crate::engines::envs::EnvManager;
use crate::engines::Engine;
use crate::error::{human_bytes, ErrorCode, FixAction, MelError, MelResult};
use crate::pull::{self, CancelToken, Puller};
use crate::registry::{incompatibility, EngineSpec, Orb, Recipe, VoiceDef};
use crate::store::{now, Install};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseView {
    pub id: String,
    pub url: String,
    pub commercial: bool,
    pub personal_use: bool,
    pub accept_required: bool,
    pub accepted: bool,
    pub attribution: Option<String>,
    pub usage_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceView {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Mono meta line, e.g. "EN · warm narrator".
    pub meta: String,
    pub language: String,
    pub languages: Vec<String>,
    pub tags: Vec<String>,
    pub orb: Orb,
    pub kind: String,
    pub recipe_id: String,
    pub installed: bool,
    pub installing: bool,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    /// Bytes still to download (0 when installed). Shown as "340 MB".
    pub download_bytes: u64,
    pub download_label: String,
    pub needs_sign_in: bool,
    /// Hugging Face page where the person agrees to the model's terms (gated recipes).
    pub gate_url: Option<String>,
    /// True when this voice will run well below real time here (big model, no GPU).
    pub slow_here: bool,
    pub license: LicenseView,
    pub has_sample: bool,
    pub can_speed: bool,
    pub delivery: Vec<String>,
    pub tags_supported: Vec<String>,
    pub starter: bool,
    /// Engine name — only for Settings › Advanced. Never shown on main screens.
    pub engine_name: String,
}

fn meta_line(languages: &[String], description: &str) -> String {
    let langs: Vec<String> = languages.iter().take(2).map(|l| l.to_uppercase()).collect();
    format!("{} · {}", langs.join(" · "), description)
}

/// Measured on-disk size of a Python engine environment (torch + deps), by backend.
/// Pocket CPU on Windows measured at 3.5 GB (11 Nov 2026); CUDA envs carry the
/// 2+ GB CUDA torch wheel plus its runtime libraries.
fn engine_env_bytes(backend: crate::engines::envs::Backend) -> u64 {
    use crate::engines::envs::Backend;
    match backend {
        Backend::Cuda => 6_500_000_000,
        Backend::Cpu => 3_500_000_000,
        Backend::Mac => 2_500_000_000,
    }
}

impl Melfield {
    /// Extra one-time bytes for a recipe's Python engine (0 when already set up).
    pub(crate) fn engine_setup_bytes(&self, recipe: &Recipe) -> u64 {
        match &recipe.engine {
            EngineSpec::Native { .. } => 0,
            EngineSpec::Python { family, python, .. } => {
                let backend = EnvManager::backend_for(&self.hardware, self.prefer_cpu());
                if self.supervisor.envs.is_ready(family, backend, python) {
                    0
                } else {
                    engine_env_bytes(backend)
                }
            }
        }
    }

    pub(crate) fn recipe_ready(&self, recipe: &Recipe) -> bool {
        if !pull::is_installed(&self.paths, recipe) {
            return false;
        }
        match &recipe.engine {
            EngineSpec::Native { .. } => true,
            EngineSpec::Python { family, python, .. } => {
                let backend = EnvManager::backend_for(&self.hardware, self.prefer_cpu());
                self.supervisor.envs.is_ready(family, backend, python)
            }
        }
    }

    pub(crate) fn any_voice_installed(&self) -> bool {
        let reg = self.registry.read();
        reg.voices()
            .iter()
            .any(|v| reg.recipe(&v.recipe).is_some_and(|r| self.recipe_ready(r)))
    }

    /// Sync the installs table with what is actually on disk.
    pub(crate) fn reconcile_installs(&self) {
        let reg = self.registry.read().clone();
        for r in reg.recipes() {
            let on_disk = pull::is_installed(&self.paths, r);
            let recorded = self.store.install(&r.id).ok().flatten().is_some();
            if on_disk && !recorded {
                let _ = self.store.put_install(&Install {
                    recipe_id: r.id.clone(),
                    revision: r
                        .artifacts
                        .first()
                        .map(|a| a.revision.clone())
                        .unwrap_or_default(),
                    dir: pull::hf_cache_root(&self.paths).display().to_string(),
                    bytes: r.disk_bytes,
                    installed_at: now(),
                });
            } else if !on_disk && recorded {
                let _ = self.store.delete_install(&r.id);
            }
        }
    }

    fn license_view(&self, r: &Recipe) -> LicenseView {
        LicenseView {
            id: r.license.id.clone(),
            url: r.license.url.clone(),
            commercial: r.license.commercial,
            personal_use: r.license.personal_use,
            accept_required: r.license.accept_required,
            accepted: self.store.license_accepted(&r.id).unwrap_or(false),
            attribution: r.license.attribution.clone(),
            usage_policy: r.license.usage_policy.clone(),
        }
    }

    fn view_for(&self, v: &VoiceDef, r: &Recipe, kind: &str) -> VoiceView {
        let installed = self.recipe_ready(r);
        let unavailable_reason = incompatibility(r, &self.hardware);
        let remaining = if installed {
            0
        } else {
            pull::remaining_bytes(&self.paths, r) + self.engine_setup_bytes(r)
        }
        .max(if installed { 0 } else { 1 });
        let mut tags = v.tags.clone();
        if kind == "clone" {
            tags.push("My clones".into());
        }
        VoiceView {
            id: v.id.clone(),
            name: v.name.clone(),
            description: v.description.clone(),
            meta: meta_line(&v.languages, &v.description),
            language: v.language.clone(),
            languages: v.languages.clone(),
            tags,
            orb: v.orb.clone(),
            kind: kind.into(),
            recipe_id: r.id.clone(),
            installed,
            installing: self.pulls.lock().contains_key(&r.id),
            available: unavailable_reason.is_none(),
            unavailable_reason,
            download_bytes: remaining,
            download_label: if installed {
                String::new()
            } else {
                human_bytes(remaining)
            },
            needs_sign_in: r.gate.is_some() && hf::load_token().is_none(),
            gate_url: r.gate.as_ref().map(|g| g.url.clone()),
            slow_here: !r.is_native()
                && r.hardware.devices.len() > 1
                && self.hardware.python_device(self.prefer_cpu()) == crate::hardware::Device::Cpu,
            license: self.license_view(r),
            has_sample: self.sample_path(&v.id).is_some(),
            can_speed: r.capabilities.speed,
            delivery: r.capabilities.delivery.clone(),
            tags_supported: r.capabilities.tags.clone(),
            starter: v.starter,
            engine_name: r.display_name.clone(),
        }
    }

    /// Every voice the person can see: stock voices that can run here (or are
    /// worth explaining), plus their clones.
    pub fn voices(&self) -> MelResult<Vec<VoiceView>> {
        let reg = self.registry.read().clone();
        let mut out: Vec<VoiceView> = reg
            .voices()
            .iter()
            .filter_map(|v| reg.recipe(&v.recipe).map(|r| self.view_for(v, r, "stock")))
            .filter(|v| v.available || v.installed)
            .collect();
        for c in self.store.clones()? {
            let Some(r) = reg.recipe(&c.recipe_id) else {
                continue;
            };
            let def = VoiceDef {
                id: c.voice_id.clone(),
                name: c.name.clone(),
                recipe: c.recipe_id.clone(),
                preset: String::new(),
                language: c.language.clone(),
                languages: vec![c.language.clone()],
                tags: vec![lang_tag(&c.language).into()],
                description: "your clone".into(),
                starter: false,
                orb: c.orb.clone(),
            };
            out.insert(0, self.view_for(&def, r, "clone"));
        }
        Ok(out)
    }

    pub fn voice(&self, id: &str) -> MelResult<VoiceView> {
        self.voices()?
            .into_iter()
            .find(|v| v.id == id)
            .ok_or_else(|| MelError::new(ErrorCode::VoiceNotFound, "that voice isn't here anymore"))
    }

    pub fn sample_path(&self, voice_id: &str) -> Option<std::path::PathBuf> {
        // Bundled apps ship samples at <resources>/samples; dev runs read registry/samples.
        let name = format!("{voice_id}.wav");
        [
            self.config.resources_dir.join("samples").join(&name),
            self.config
                .resources_dir
                .join("registry")
                .join("samples")
                .join(&name),
        ]
        .into_iter()
        .find(|p| p.exists())
    }

    pub fn accept_license(&self, recipe_id: &str) -> MelResult<()> {
        self.store.accept_license(recipe_id)?;
        self.emit(CoreEvent::VoicesChanged);
        Ok(())
    }

    /// Download a voice's recipe (and its engine) — the "Get" button.
    pub async fn get_voice(self: &Arc<Self>, voice_id: &str) -> MelResult<()> {
        let recipe_id = self.voice(voice_id)?.recipe_id;
        self.install_recipe(&recipe_id).await
    }

    pub async fn install_recipe(self: &Arc<Self>, recipe_id: &str) -> MelResult<()> {
        let recipe = self
            .registry
            .read()
            .recipe(recipe_id)
            .cloned()
            .ok_or_else(|| {
                MelError::new(
                    ErrorCode::VoiceNotFound,
                    "that voice isn't available anymore",
                )
            })?;
        if let Some(reason) = incompatibility(&recipe, &self.hardware) {
            return Err(MelError::new(
                ErrorCode::Unsupported,
                format!("this voice {reason}"),
            ));
        }
        if recipe.license.accept_required && !self.store.license_accepted(&recipe.id)? {
            return Err(MelError::new(
                ErrorCode::LicenseNotAccepted,
                "this voice is for personal use — accept its terms to continue",
            ));
        }
        let cancel = {
            let mut pulls = self.pulls.lock();
            if pulls.contains_key(&recipe.id) {
                return Ok(()); // already in progress; progress events keep flowing
            }
            let c = CancelToken::default();
            pulls.insert(recipe.id.clone(), c.clone());
            c
        };
        self.emit(CoreEvent::VoicesChanged);
        let result = self.install_inner(&recipe, &cancel).await;
        self.pulls.lock().remove(&recipe.id);
        if result.is_ok() {
            self.store.put_install(&Install {
                recipe_id: recipe.id.clone(),
                revision: recipe
                    .artifacts
                    .first()
                    .map(|a| a.revision.clone())
                    .unwrap_or_default(),
                dir: pull::hf_cache_root(&self.paths).display().to_string(),
                bytes: recipe.disk_bytes,
                installed_at: now(),
            })?;
        }
        self.emit(CoreEvent::VoicesChanged);
        result
    }

    async fn install_inner(
        self: &Arc<Self>,
        recipe: &Recipe,
        cancel: &CancelToken,
    ) -> MelResult<()> {
        let token = hf::load_token();
        let puller = Puller::new(self.http.clone(), self.paths.clone());
        let me = Arc::downgrade(self);
        puller
            .pull(
                recipe,
                token.as_deref(),
                cancel,
                Arc::new(move |p| {
                    if let Some(m) = me.upgrade() {
                        m.emit(CoreEvent::Pull(p));
                    }
                }),
            )
            .await?;
        if let EngineSpec::Python { family, python, .. } = &recipe.engine {
            let needed = self.engine_setup_bytes(recipe);
            let free = crate::paths::Paths::free_space(self.paths.root());
            if needed > 0 && free < needed + 500_000_000 {
                return Err(MelError::disk_full(needed + 500_000_000 - free));
            }
            let me = self.clone();
            let (family, python) = (family.clone(), python.clone());
            let backend = EnvManager::backend_for(&self.hardware, self.prefer_cpu());
            tokio::task::spawn_blocking(move || {
                me.supervisor
                    .envs
                    .ensure(&family, backend, &python, &|p| me.emit(CoreEvent::Env(p)))
            })
            .await
            .map_err(MelError::internal)??;
        }
        Ok(())
    }

    pub fn cancel_pull(&self, recipe_id: &str) {
        if let Some(c) = self.pulls.lock().get(recipe_id) {
            c.cancel();
        }
    }

    /// First launch: pull the starter voices.
    pub async fn start_setup(self: &Arc<Self>) -> MelResult<()> {
        let ids: Vec<String> = self
            .registry
            .read()
            .starter_recipes()
            .iter()
            .map(|r| r.id.clone())
            .collect();
        for id in ids {
            self.install_recipe(&id).await?;
        }
        let mut s = self.settings()?;
        s.onboarded = true;
        self.update_settings(s)?;
        Ok(())
    }

    /// Remove a voice's downloaded files. Voices sharing the same files go too.
    pub fn remove_voice(&self, voice_id: &str) -> MelResult<u64> {
        let v = self.voice(voice_id)?;
        if v.kind == "clone" {
            self.store.delete_clone(voice_id)?;
            let _ = std::fs::remove_dir_all(self.paths.clone_dir(voice_id));
            self.emit(CoreEvent::VoicesChanged);
            return Ok(0);
        }
        let reg = self.registry.read().clone();
        let recipe = reg
            .recipe(&v.recipe_id)
            .ok_or_else(|| MelError::internal("recipe vanished"))?;
        let keep: Vec<&Recipe> = reg
            .recipes()
            .iter()
            .filter(|r| r.id != recipe.id && pull::is_installed(&self.paths, r))
            .collect();
        let freed = pull::remove(&self.paths, recipe, &keep)?;
        self.store.delete_install(&recipe.id)?;
        self.native.lock().remove(&recipe.id);
        self.emit(CoreEvent::VoicesChanged);
        Ok(freed)
    }

    /// Voices that disappear together with `voice_id` (shared download).
    pub fn voices_sharing_files(&self, voice_id: &str) -> MelResult<Vec<String>> {
        let v = self.voice(voice_id)?;
        Ok(self
            .voices()?
            .into_iter()
            .filter(|o| o.recipe_id == v.recipe_id && o.id != v.id && o.kind == "stock")
            .map(|o| o.name)
            .collect())
    }

    /// Lazily load a native engine for a recipe.
    #[cfg(feature = "native-tts")]
    pub(crate) fn native_engine(&self, recipe: &Recipe) -> MelResult<Arc<dyn Engine>> {
        if let Some(e) = self.native.lock().get(&recipe.id) {
            return Ok(e.clone());
        }
        let artifact = recipe
            .artifacts
            .first()
            .ok_or_else(|| MelError::internal("recipe without artifacts"))?;
        let dir = pull::snapshot_dir(&self.paths, artifact);
        let engine: Arc<dyn Engine> = Arc::new(crate::engines::native::KokoroEngine::load(
            &dir,
            self.hardware.cpu_cores.saturating_sub(1).max(2),
        )?);
        self.native.lock().insert(recipe.id.clone(), engine.clone());
        Ok(engine)
    }

    #[cfg(not(feature = "native-tts"))]
    pub(crate) fn native_engine(&self, _recipe: &Recipe) -> MelResult<Arc<dyn Engine>> {
        Err(MelError::new(
            ErrorCode::Unsupported,
            "this build has no built-in voices",
        ))
    }

    pub(crate) fn ensure_ready(
        &self,
        recipe: &Recipe,
        name: &str,
        voice_id: &str,
    ) -> MelResult<()> {
        if self.recipe_ready(recipe) {
            return Ok(());
        }
        if self.pulls.lock().contains_key(&recipe.id) {
            return Err(MelError::new(
                ErrorCode::EngineStarting,
                format!(
                    "{} is still downloading — almost there",
                    name.to_lowercase()
                ),
            ));
        }
        Err(
            MelError::voice_not_installed(voice_id, name).with_fix(FixAction::GetVoice {
                voice_id: voice_id.into(),
            }),
        )
    }
}

pub(crate) fn lang_tag(lang: &str) -> &'static str {
    match lang {
        "en" => "English",
        "hi" => "Hindi",
        _ => "Multilingual",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_line_matches_design() {
        assert_eq!(
            meta_line(&["hi".into(), "en".into()], "calm"),
            "HI · EN · calm"
        );
        assert_eq!(
            meta_line(&["en".into()], "warm narrator"),
            "EN · warm narrator"
        );
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub family: String,
    pub label: String,
    pub recipes: Vec<String>,
    pub ready: bool,
    pub running: bool,
}

impl Melfield {
    /// Settings › Advanced: engine families that have something installed.
    pub fn engines_status(&self) -> Vec<EngineStatus> {
        let reg = self.registry.read().clone();
        let running = self.supervisor.running_families();
        let mut out: Vec<EngineStatus> = Vec::new();
        for r in reg
            .recipes()
            .iter()
            .filter(|r| pull::is_installed(&self.paths, r))
        {
            let family = r.family().unwrap_or("native").to_string();
            let ready = self.recipe_ready(r);
            match out.iter_mut().find(|e| e.family == family) {
                Some(e) => {
                    e.recipes.push(r.display_name.clone());
                    e.ready &= ready;
                }
                None => out.push(EngineStatus {
                    label: if family == "native" {
                        "built-in voices".into()
                    } else {
                        format!("{family} engine")
                    },
                    running: running.contains(&family),
                    family,
                    recipes: vec![r.display_name.clone()],
                    ready,
                }),
            }
        }
        out
    }

    /// "Repair": rebuild a Python engine env, or re-verify native model files.
    pub async fn repair_engine(self: &Arc<Self>, family: &str) -> MelResult<()> {
        let reg = self.registry.read().clone();
        let recipes: Vec<Recipe> = reg
            .recipes()
            .iter()
            .filter(|r| r.family().unwrap_or("native") == family)
            .cloned()
            .collect();
        if family == "native" {
            self.native.lock().clear();
            for r in recipes
                .iter()
                .filter(|r| self.store.install(&r.id).ok().flatten().is_some())
            {
                self.install_recipe(&r.id).await?;
            }
            return Ok(());
        }
        let Some(EngineSpec::Python { python, .. }) = recipes.first().map(|r| r.engine.clone())
        else {
            return Err(MelError::new(
                ErrorCode::NotFound,
                "nothing to repair there",
            ));
        };
        self.supervisor.shutdown_all();
        let me = self.clone();
        let family = family.to_string();
        let backend = EnvManager::backend_for(&self.hardware, self.prefer_cpu());
        tokio::task::spawn_blocking(move || {
            me.supervisor
                .envs
                .repair(&family, backend, &python, &|p| me.emit(CoreEvent::Env(p)))
        })
        .await
        .map_err(MelError::internal)??;
        self.emit(CoreEvent::VoicesChanged);
        Ok(())
    }
}
