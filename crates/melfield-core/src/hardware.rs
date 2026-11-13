//! Hardware probe: decides which voices can run here and on which device.
//! Tiers decide which engines are offered and where they run (see docs/architecture.md).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    WindowsX64,
    WindowsArm64,
    MacosArm64,
    MacosX64,
    LinuxX64,
    LinuxArm64,
}

impl Platform {
    pub fn current() -> Self {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "aarch64") => Self::WindowsArm64,
            ("windows", _) => Self::WindowsX64,
            ("macos", "aarch64") => Self::MacosArm64,
            ("macos", _) => Self::MacosX64,
            ("linux", "aarch64") => Self::LinuxArm64,
            _ => Self::LinuxX64,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::WindowsX64 => "windows-x64",
            Self::WindowsArm64 => "windows-arm64",
            Self::MacosArm64 => "macos-arm64",
            Self::MacosX64 => "macos-x64",
            Self::LinuxX64 => "linux-x64",
            Self::LinuxArm64 => "linux-arm64",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gpu {
    pub vendor: GpuVendor,
    pub name: String,
    /// Dedicated memory in MB (unified memory for Apple).
    pub vram_mb: u64,
    /// CUDA compute capability "8.6" etc. for NVIDIA.
    pub compute_capability: Option<String>,
    pub driver: Option<String>,
}

/// Hardware tiers (docs/architecture.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    /// NVIDIA >= 8 GB
    A,
    /// NVIDIA 4–8 GB
    AMinus,
    /// Apple Silicon, macOS 14+
    B,
    /// Apple Silicon, macOS < 14
    BMinus,
    /// AMD / Intel GPU
    C,
    /// CPU only
    D,
    /// Intel Mac
    E,
}

/// Where a Python engine should run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Device {
    Cuda,
    Mps,
    Cpu,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub platform: Platform,
    pub os_version: String,
    pub cpu_name: String,
    pub cpu_cores: usize,
    pub ram_mb: u64,
    pub gpus: Vec<Gpu>,
    pub tier: Tier,
    /// Short human line for onboarding, e.g. "found your gpu — fast mode on".
    pub summary: String,
}

impl HardwareInfo {
    pub fn best_gpu(&self) -> Option<&Gpu> {
        self.gpus.iter().max_by_key(|g| g.vram_mb)
    }

    /// Device a Python engine should use, honouring the user's compute preference.
    pub fn python_device(&self, prefer_cpu: bool) -> Device {
        if prefer_cpu {
            return Device::Cpu;
        }
        match self.tier {
            Tier::A | Tier::AMinus => Device::Cuda,
            Tier::B => Device::Mps,
            _ => Device::Cpu,
        }
    }

    /// Whether PyTorch engines can run at all on this machine.
    pub fn python_engines_supported(&self) -> bool {
        !matches!(self.tier, Tier::E | Tier::BMinus)
    }

    pub fn has_fast_gpu(&self) -> bool {
        matches!(self.tier, Tier::A | Tier::AMinus | Tier::B)
    }

    pub fn vram_gb(&self) -> f64 {
        self.best_gpu()
            .map(|g| g.vram_mb as f64 / 1024.0)
            .unwrap_or(0.0)
    }
}

pub fn classify(platform: Platform, os_version: &str, gpus: &[Gpu]) -> Tier {
    match platform {
        Platform::MacosX64 => Tier::E,
        Platform::MacosArm64 => {
            let major: u32 = os_version
                .split('.')
                .next()
                .and_then(|m| m.parse().ok())
                .unwrap_or(0);
            if major >= 14 {
                Tier::B
            } else {
                Tier::BMinus
            }
        }
        _ => {
            let nvidia = gpus
                .iter()
                .filter(|g| g.vendor == GpuVendor::Nvidia)
                .map(|g| g.vram_mb)
                .max();
            match nvidia {
                Some(v) if v >= 7_800 => Tier::A,
                Some(v) if v >= 3_800 => Tier::AMinus,
                _ if gpus
                    .iter()
                    .any(|g| matches!(g.vendor, GpuVendor::Amd | GpuVendor::Intel)) =>
                {
                    Tier::C
                }
                _ => Tier::D,
            }
        }
    }
}

fn summary_line(tier: Tier) -> String {
    match tier {
        Tier::A | Tier::AMinus => "found your gpu — fast mode on",
        Tier::B => "found apple silicon — fast mode on",
        Tier::BMinus => "found apple silicon — update macos to unlock more voices",
        Tier::C | Tier::D => "running on your processor — every starter voice works",
        Tier::E => "running on your processor — starter voices ready",
    }
    .to_string()
}

/// Probe the machine. Never fails: anything we can't read is simply absent.
pub fn probe() -> HardwareInfo {
    let platform = Platform::current();
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();
    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_default();
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let ram_mb = sys.total_memory() / 1_048_576;
    let os_version = sysinfo::System::os_version().unwrap_or_default();

    let mut gpus = probe_nvidia();
    gpus.extend(probe_other_gpus(platform, ram_mb, &gpus));
    let tier = classify(platform, &os_version, &gpus);
    tracing::info!(?platform, %os_version, ?tier, gpus = gpus.len(), "hardware probed");
    HardwareInfo {
        platform,
        os_version,
        cpu_name,
        cpu_cores,
        ram_mb,
        gpus,
        tier,
        summary: summary_line(tier),
    }
}

#[cfg(not(target_os = "macos"))]
fn probe_nvidia() -> Vec<Gpu> {
    use nvml_wrapper::Nvml;
    let Ok(nvml) = Nvml::init() else {
        return vec![];
    };
    let driver = nvml.sys_driver_version().ok();
    let count = nvml.device_count().unwrap_or(0);
    (0..count)
        .filter_map(|i| nvml.device_by_index(i).ok())
        .map(|d| Gpu {
            vendor: GpuVendor::Nvidia,
            name: d.name().unwrap_or_else(|_| "nvidia gpu".into()),
            vram_mb: d.memory_info().map(|m| m.total / 1_048_576).unwrap_or(0),
            compute_capability: d
                .cuda_compute_capability()
                .ok()
                .map(|c| format!("{}.{}", c.major, c.minor)),
            driver: driver.clone(),
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn probe_nvidia() -> Vec<Gpu> {
    vec![]
}

fn probe_other_gpus(platform: Platform, ram_mb: u64, found: &[Gpu]) -> Vec<Gpu> {
    match platform {
        Platform::MacosArm64 => vec![Gpu {
            vendor: GpuVendor::Apple,
            name: "apple silicon".into(),
            vram_mb: ram_mb,
            compute_capability: None,
            driver: None,
        }],
        Platform::LinuxX64 | Platform::LinuxArm64 => linux_drm_gpus(found),
        _ => vec![],
    }
}

/// Read PCI vendor ids from /sys/class/drm so AMD/Intel GPUs are at least recognised.
fn linux_drm_gpus(found: &[Gpu]) -> Vec<Gpu> {
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else {
        return vec![];
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let vendor =
            std::fs::read_to_string(entry.path().join("device/vendor")).unwrap_or_default();
        let vendor = match vendor.trim() {
            "0x1002" => GpuVendor::Amd,
            "0x8086" => GpuVendor::Intel,
            _ => continue,
        };
        let vram_mb = std::fs::read_to_string(entry.path().join("device/mem_info_vram_total"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|b| b / 1_048_576)
            .unwrap_or(0);
        if !found.iter().any(|g| g.vendor == vendor) {
            out.push(Gpu {
                vendor,
                name: format!("{vendor:?} gpu").to_lowercase(),
                vram_mb,
                compute_capability: None,
                driver: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nvidia(vram_mb: u64) -> Gpu {
        Gpu {
            vendor: GpuVendor::Nvidia,
            name: "rtx".into(),
            vram_mb,
            compute_capability: None,
            driver: None,
        }
    }

    #[test]
    fn classifies_tiers() {
        assert_eq!(
            classify(Platform::WindowsX64, "11", &[nvidia(12_288)]),
            Tier::A
        );
        assert_eq!(
            classify(Platform::LinuxX64, "22.04", &[nvidia(6_144)]),
            Tier::AMinus
        );
        assert_eq!(
            classify(Platform::WindowsX64, "11", &[nvidia(2_048)]),
            Tier::D
        );
        assert_eq!(classify(Platform::WindowsX64, "11", &[]), Tier::D);
        assert_eq!(classify(Platform::MacosArm64, "14.5", &[]), Tier::B);
        assert_eq!(classify(Platform::MacosArm64, "13.6", &[]), Tier::BMinus);
        assert_eq!(classify(Platform::MacosX64, "13.6", &[]), Tier::E);
        let amd = Gpu {
            vendor: GpuVendor::Amd,
            name: "rx".into(),
            vram_mb: 8000,
            compute_capability: None,
            driver: None,
        };
        assert_eq!(classify(Platform::LinuxX64, "24.04", &[amd]), Tier::C);
    }

    #[test]
    fn probe_never_panics() {
        let hw = probe();
        assert!(hw.cpu_cores >= 1);
        assert!(!hw.summary.is_empty());
    }

    #[test]
    fn device_respects_cpu_preference() {
        let hw = HardwareInfo {
            platform: Platform::WindowsX64,
            os_version: "11".into(),
            cpu_name: String::new(),
            cpu_cores: 8,
            ram_mb: 16_000,
            gpus: vec![nvidia(12_000)],
            tier: Tier::A,
            summary: String::new(),
        };
        assert_eq!(hw.python_device(false), Device::Cuda);
        assert_eq!(hw.python_device(true), Device::Cpu);
    }
}
