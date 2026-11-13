//! Recipes and voices. A bundled copy ships in the binary; a signed remote
//! index can refresh it without an app update (docs/architecture.md).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{MelError, MelResult};
use crate::hardware::{HardwareInfo, Tier};

const BUNDLED_INDEX: &str = include_str!("../../../registry/index.json");

/// minisign public key for registry signatures. Empty = remote refresh disabled
/// (enabled once the project publishes its signing key).
pub const REGISTRY_PUBLIC_KEY: &str = "";
pub const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/tgakathunderr/melfield/main/registry/index.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndex {
    pub schema_version: u32,
    pub recipes: Vec<Recipe>,
    pub voices: Vec<VoiceDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineSpec {
    Native {
        runtime: String,
        model_type: String,
    },
    Python {
        family: String,
        entry: String,
        python: String,
        lock: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactFile {
    pub path: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub source: String,
    pub repo: String,
    pub revision: String,
    pub gated: bool,
    pub files: Vec<ArtifactFile>,
}

impl Artifact {
    pub fn url(&self, file: &ArtifactFile) -> String {
        // Full URLs are allowed for tests and self-hosted mirrors.
        let base = if self.repo.starts_with("http://") || self.repo.starts_with("https://") {
            self.repo.clone()
        } else {
            format!("https://huggingface.co/{}", self.repo)
        };
        format!("{base}/resolve/{}/{}", self.revision, file.path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    pub id: String,
    pub url: String,
    pub commercial: bool,
    pub attribution: Option<String>,
    pub accept_required: bool,
    pub personal_use: bool,
    pub usage_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    pub provider: String,
    pub repo: String,
    pub url: String,
    pub form: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefSeconds {
    pub min: f32,
    pub ideal: f32,
    pub max: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub clone: bool,
    pub clone_ref_seconds: RefSeconds,
    pub clone_needs_transcript: bool,
    pub voice_design: bool,
    pub emotion: Option<String>,
    pub tags: Vec<String>,
    pub speed: bool,
    pub delivery: Vec<String>,
    pub streaming: String,
    pub max_chunk_chars: usize,
    pub builtin_watermark: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareReq {
    pub devices: Vec<String>,
    pub min_ram_gb: f64,
    pub min_vram_gb: Option<f64>,
    pub os: Vec<String>,
    pub macos_min: Option<String>,
    #[serde(default)]
    pub realtime_hint: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub schema_version: u32,
    pub id: String,
    pub display_name: String,
    pub engine: EngineSpec,
    pub artifacts: Vec<Artifact>,
    pub download_bytes: u64,
    pub disk_bytes: u64,
    pub license: License,
    pub gate: Option<Gate>,
    pub capabilities: Capabilities,
    pub languages: Vec<String>,
    #[serde(default)]
    pub g2p: HashMap<String, String>,
    pub sample_rate: u32,
    pub hardware: HardwareReq,
    pub tier: String,
    #[serde(default)]
    pub notes_internal: String,
}

impl Recipe {
    pub fn is_native(&self) -> bool {
        matches!(self.engine, EngineSpec::Native { .. })
    }

    pub fn family(&self) -> Option<&str> {
        match &self.engine {
            EngineSpec::Python { family, .. } => Some(family),
            EngineSpec::Native { .. } => None,
        }
    }

    pub fn supports_language(&self, lang: &str) -> bool {
        let base = lang.split('-').next().unwrap_or(lang);
        self.languages.iter().any(|l| l == base)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Orb {
    pub c1: [u8; 3],
    pub c2: [u8; 3],
    pub k: u32,
    pub amp: f32,
    pub ring: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceDef {
    pub id: String,
    pub name: String,
    pub recipe: String,
    pub preset: String,
    pub language: String,
    pub languages: Vec<String>,
    pub tags: Vec<String>,
    pub description: String,
    pub starter: bool,
    pub orb: Orb,
}

/// Why a recipe can't run here, in human words. `None` = it can.
pub fn incompatibility(recipe: &Recipe, hw: &HardwareInfo) -> Option<String> {
    let platform = hw.platform.as_str();
    if !recipe.hardware.os.iter().any(|o| o == platform) {
        return Some(match hw.tier {
            Tier::E => "needs an apple silicon mac".into(),
            _ => "isn't available on this kind of computer yet".into(),
        });
    }
    if !recipe.is_native() && !hw.python_engines_supported() {
        return Some("needs macos 14 or newer".into());
    }
    if (hw.ram_mb as f64) < recipe.hardware.min_ram_gb * 1000.0 * 0.9 {
        return Some(format!(
            "needs {:.0} GB of memory",
            recipe.hardware.min_ram_gb
        ));
    }
    let gpu_only = !recipe.hardware.devices.iter().any(|d| d == "cpu");
    if gpu_only {
        let has_device = match hw.tier {
            Tier::A | Tier::AMinus => recipe.hardware.devices.iter().any(|d| d == "cuda"),
            Tier::B => recipe.hardware.devices.iter().any(|d| d == "mps"),
            _ => false,
        };
        if !has_device {
            return Some("needs a graphics card".into());
        }
        if let Some(min) = recipe.hardware.min_vram_gb {
            if hw.vram_gb() + 0.3 < min {
                return Some(format!("needs a graphics card with {min:.0} GB"));
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct Registry {
    index: RegistryIndex,
}

impl Registry {
    pub fn bundled() -> MelResult<Self> {
        Self::parse(BUNDLED_INDEX)
    }

    pub fn parse(json: &str) -> MelResult<Self> {
        let index: RegistryIndex = serde_json::from_str(json)?;
        if index.schema_version != 1 {
            return Err(MelError::internal(format!(
                "unsupported registry schema {}",
                index.schema_version
            )));
        }
        Ok(Self { index })
    }

    /// Load the cached remote index if present and valid, else the bundled one.
    pub fn load(cache_dir: &Path) -> MelResult<Self> {
        let cached = cache_dir.join("index.json");
        if let Ok(text) = std::fs::read_to_string(&cached) {
            match Self::parse(&text) {
                Ok(r) => return Ok(r),
                Err(e) => {
                    tracing::warn!(detail = %e.detail, "cached registry unreadable; using bundled")
                }
            }
        }
        Self::bundled()
    }

    /// Fetch + verify the signed remote index. Never fails loudly: callers keep
    /// whatever registry they had.
    pub async fn refresh(cache_dir: &Path, client: &reqwest::Client) -> MelResult<Option<Self>> {
        if REGISTRY_PUBLIC_KEY.is_empty() {
            return Ok(None);
        }
        let body = client
            .get(REGISTRY_URL)
            .send()
            .await
            .map_err(|_| MelError::offline())?
            .text()
            .await
            .map_err(|_| MelError::offline())?;
        let sig = client
            .get(format!("{REGISTRY_URL}.minisig"))
            .send()
            .await
            .map_err(|_| MelError::offline())?
            .text()
            .await
            .map_err(|_| MelError::offline())?;
        verify_signature(REGISTRY_PUBLIC_KEY, body.as_bytes(), &sig)?;
        let reg = Self::parse(&body)?;
        std::fs::write(cache_dir.join("index.json"), &body)?;
        Ok(Some(reg))
    }

    pub fn recipes(&self) -> &[Recipe] {
        &self.index.recipes
    }
    pub fn voices(&self) -> &[VoiceDef] {
        &self.index.voices
    }
    pub fn recipe(&self, id: &str) -> Option<&Recipe> {
        self.index.recipes.iter().find(|r| r.id == id)
    }
    pub fn voice(&self, id: &str) -> Option<&VoiceDef> {
        self.index.voices.iter().find(|v| v.id == id)
    }
    pub fn starter_recipes(&self) -> Vec<&Recipe> {
        self.index
            .recipes
            .iter()
            .filter(|r| r.tier == "starter")
            .collect()
    }
}

pub fn verify_signature(public_key: &str, data: &[u8], signature: &str) -> MelResult<()> {
    let pk = minisign_verify::PublicKey::from_base64(public_key)
        .map_err(|e| MelError::internal(format!("bad registry key: {e}")))?;
    let sig = minisign_verify::Signature::decode(signature)
        .map_err(|e| MelError::internal(format!("bad registry signature: {e}")))?;
    pk.verify(data, &sig, false)
        .map_err(|e| MelError::internal(format!("registry signature mismatch: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::{Gpu, GpuVendor, Platform};

    fn hw(tier: Tier, platform: Platform, vram_mb: u64, ram_mb: u64) -> HardwareInfo {
        let gpus = if vram_mb > 0 {
            vec![Gpu {
                vendor: GpuVendor::Nvidia,
                name: "x".into(),
                vram_mb,
                compute_capability: None,
                driver: None,
            }]
        } else {
            vec![]
        };
        HardwareInfo {
            platform,
            os_version: "14.0".into(),
            cpu_name: String::new(),
            cpu_cores: 8,
            ram_mb,
            gpus,
            tier,
            summary: String::new(),
        }
    }

    #[test]
    fn bundled_registry_parses_and_voices_resolve() {
        let r = Registry::bundled().unwrap();
        assert!(r.recipe("kokoro-82m").is_some());
        for v in r.voices() {
            assert!(
                r.recipe(&v.recipe).is_some(),
                "voice {} has no recipe",
                v.id
            );
        }
        assert_eq!(r.starter_recipes().len(), 1);
        assert_eq!(r.voices().iter().filter(|v| v.starter).count(), 3);
    }

    #[test]
    fn compatibility_rules_follow_tiers() {
        let r = Registry::bundled().unwrap();
        let kokoro = r.recipe("kokoro-82m").unwrap();
        let vox = r.recipe("voxcpm2").unwrap();
        let qwen = r.recipe("qwen3-tts-0.6b-base").unwrap();
        let intel_mac = hw(Tier::E, Platform::MacosX64, 0, 16_000);
        assert!(incompatibility(kokoro, &intel_mac).is_none());
        assert_eq!(
            incompatibility(qwen, &intel_mac).as_deref(),
            Some("needs an apple silicon mac")
        );
        let cpu_pc = hw(Tier::D, Platform::WindowsX64, 0, 16_000);
        assert_eq!(
            incompatibility(vox, &cpu_pc).as_deref(),
            Some("needs a graphics card")
        );
        assert!(incompatibility(qwen, &cpu_pc).is_none());
        let small_gpu = hw(Tier::AMinus, Platform::LinuxX64, 6_000, 32_000);
        assert_eq!(
            incompatibility(vox, &small_gpu).as_deref(),
            Some("needs a graphics card with 8 GB")
        );
        let big_gpu = hw(Tier::A, Platform::LinuxX64, 24_000, 32_000);
        assert!(incompatibility(vox, &big_gpu).is_none());
    }

    #[test]
    fn rejects_bad_signature() {
        // Key/signature from minisign-verify's own test vectors would pass; garbage must fail.
        assert!(verify_signature(
            "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3",
            b"x",
            "nope"
        )
        .is_err());
    }

    #[test]
    fn language_matching_uses_base_code() {
        let r = Registry::bundled().unwrap();
        let cb = r.recipe("chatterbox-multilingual-v3").unwrap();
        assert!(cb.supports_language("hi"));
        assert!(cb.supports_language("en-GB"));
        assert!(!cb.supports_language("ta"));
    }
}
