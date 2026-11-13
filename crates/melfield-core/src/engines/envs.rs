//! Per-family Python environments managed by a bundled `uv`. The person never
//! sees any of this: they see "getting ready… 1.4 GB of 2.1 GB".

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::{ErrorCode, FixAction, MelError, MelResult};
use crate::hardware::{Device, HardwareInfo, Platform};
use crate::paths::Paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Cuda,
    Cpu,
    Mac,
}

impl Backend {
    pub fn for_device(device: Device, platform: Platform) -> Self {
        match (device, platform) {
            (_, Platform::MacosArm64 | Platform::MacosX64) => Self::Mac,
            (Device::Cuda, _) => Self::Cuda,
            _ => Self::Cpu,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Cpu => "cpu",
            Self::Mac => "mac",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvProgress {
    pub family: String,
    pub stage: String,
    pub detail: String,
}

pub struct EnvManager {
    pub uv: PathBuf,
    pub workers_dir: PathBuf,
    paths: Paths,
}

/// Python interpreter inside a venv.
pub fn venv_python(env: &Path) -> PathBuf {
    if cfg!(windows) {
        env.join("Scripts").join("python.exe")
    } else {
        env.join("bin").join("python")
    }
}

impl EnvManager {
    pub fn new(uv: PathBuf, workers_dir: PathBuf, paths: Paths) -> Self {
        Self {
            uv,
            workers_dir,
            paths,
        }
    }

    pub fn requirements(&self, family: &str, backend: Backend) -> PathBuf {
        self.workers_dir
            .join("families")
            .join(family)
            .join(format!("requirements-{}.txt", backend.as_str()))
    }

    pub fn env_dir(&self, family: &str, backend: Backend) -> PathBuf {
        self.paths
            .env_dir(&format!("{family}-{}", backend.as_str()))
    }

    fn lock_hash(&self, family: &str, backend: Backend, python: &str) -> MelResult<String> {
        let req = std::fs::read(self.requirements(family, backend)).map_err(|e| {
            MelError::new(
                ErrorCode::Unsupported,
                "this voice isn't available on this computer yet",
            )
            .with_detail(e)
        })?;
        let mut h = Sha256::new();
        h.update(&req);
        h.update(python.as_bytes());
        if let Ok(o) = std::fs::read(
            self.workers_dir
                .join("families")
                .join(family)
                .join("overrides.txt"),
        ) {
            h.update(&o);
        }
        Ok(hex::encode(h.finalize())[..16].to_string())
    }

    pub fn is_ready(&self, family: &str, backend: Backend, python: &str) -> bool {
        let env = self.env_dir(family, backend);
        let Ok(hash) = self.lock_hash(family, backend, python) else {
            return false;
        };
        std::fs::read_to_string(env.join(".melfield-ready"))
            .map(|s| s.trim() == hash)
            .unwrap_or(false)
            && venv_python(&env).exists()
    }

    fn base_command(&self) -> Command {
        let mut cmd = Command::new(&self.uv);
        cmd.env("UV_NO_CONFIG", "1")
            .env("UV_PYTHON_PREFERENCE", "only-managed")
            .env("UV_PYTHON_INSTALL_DIR", self.paths.python())
            .env("UV_CACHE_DIR", self.paths.uv_cache())
            .env("UV_NO_PROGRESS", "1")
            .env("UV_SYSTEM_CERTS", "1")
            .env_remove("VIRTUAL_ENV")
            .env_remove("PYTHONPATH")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        cmd
    }

    fn run(
        &self,
        mut cmd: Command,
        family: &str,
        stage: &str,
        on_progress: &dyn Fn(EnvProgress),
    ) -> MelResult<()> {
        let mut child = cmd.spawn().map_err(|e| {
            MelError::new(
                ErrorCode::EngineBroken,
                "melfield's helper is missing — reinstalling melfield will fix it",
            )
            .with_detail(format!("spawn uv {}: {e}", self.uv.display()))
        })?;
        let stderr = child.stderr.take();
        let mut log = Vec::new();
        if let Some(err) = stderr {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if let Some(detail) = human_progress(&line) {
                    on_progress(EnvProgress {
                        family: family.into(),
                        stage: stage.into(),
                        detail,
                    });
                }
                log.push(line);
            }
        }
        let status = child.wait()?;
        if status.success() {
            return Ok(());
        }
        let tail = log
            .iter()
            .rev()
            .take(30)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let offline = tail.contains("dns error")
            || tail.contains("Failed to fetch")
            || tail.contains("connect");
        let e = if offline {
            MelError::offline()
        } else if tail.contains("No space left") || tail.contains("os error 112") {
            MelError::disk_full(0)
        } else {
            MelError::new(
                ErrorCode::EngineBroken,
                "setting up this voice didn't finish — try again in a moment",
            )
            .with_fix(FixAction::Repair {
                family: family.to_string(),
            })
        };
        Err(e.with_detail(format!("uv {stage} failed: {status}\n{tail}")))
    }

    /// Create/repair the env for a family. Idempotent and resumable (uv caches wheels).
    pub fn ensure(
        &self,
        family: &str,
        backend: Backend,
        python: &str,
        on_progress: &dyn Fn(EnvProgress),
    ) -> MelResult<PathBuf> {
        let env = self.env_dir(family, backend);
        if self.is_ready(family, backend, python) {
            return Ok(env);
        }
        let hash = self.lock_hash(family, backend, python)?;
        on_progress(EnvProgress {
            family: family.into(),
            stage: "python".into(),
            detail: "getting ready".into(),
        });
        let mut venv = self.base_command();
        venv.args(["venv", "--clear", "--seed", "--python", python])
            .arg(&env);
        self.run(venv, family, "python", on_progress)?;
        let mut install = self.base_command();
        install
            .args(["pip", "install", "--python"])
            .arg(venv_python(&env))
            .arg("-r")
            .arg(self.requirements(family, backend))
            // Packages come from PyPI and the PyTorch index; pick the best match across both.
            .args(["--index-strategy", "unsafe-best-match"]);
        let overrides = self
            .workers_dir
            .join("families")
            .join(family)
            .join("overrides.txt");
        if overrides.exists() {
            install.arg("--override").arg(overrides);
        }
        self.run(install, family, "packages", on_progress)?;
        std::fs::write(env.join(".melfield-ready"), hash)?;
        Ok(env)
    }

    /// Delete and rebuild ("Repair").
    pub fn repair(
        &self,
        family: &str,
        backend: Backend,
        python: &str,
        on_progress: &dyn Fn(EnvProgress),
    ) -> MelResult<PathBuf> {
        let env = self.env_dir(family, backend);
        if env.exists() {
            std::fs::remove_dir_all(&env)?;
        }
        self.ensure(family, backend, python, on_progress)
    }

    pub fn backend_for(hw: &HardwareInfo, prefer_cpu: bool) -> Backend {
        Backend::for_device(hw.python_device(prefer_cpu), hw.platform)
    }
}

/// Turn uv's stderr into calm progress text ("downloading 2.1 GB").
fn human_progress(line: &str) -> Option<String> {
    let l = line.trim();
    if let Some(rest) = l.strip_prefix("Downloading ") {
        let size = rest.rsplit_once('(').map(|(_, s)| {
            s.trim_end_matches(')')
                .replace("GiB", " GB")
                .replace("MiB", " MB")
        });
        return Some(match size {
            Some(s) => format!("downloading {s}"),
            None => "downloading".into(),
        });
    }
    if l.starts_with("Installed") || l.starts_with("Prepared") {
        return Some("almost there".into());
    }
    if l.starts_with("Downloaded") {
        return Some("unpacking".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lines_are_human() {
        assert_eq!(
            human_progress("Downloading torch (2.1GiB)").as_deref(),
            Some("downloading 2.1 GB")
        );
        assert_eq!(
            human_progress("Installed 52 packages in 3.2s").as_deref(),
            Some("almost there")
        );
        assert_eq!(human_progress("Resolved 52 packages"), None);
    }

    #[test]
    fn backend_selection() {
        assert_eq!(
            Backend::for_device(Device::Cuda, Platform::WindowsX64),
            Backend::Cuda
        );
        assert_eq!(
            Backend::for_device(Device::Mps, Platform::MacosArm64),
            Backend::Mac
        );
        assert_eq!(
            Backend::for_device(Device::Cpu, Platform::LinuxX64),
            Backend::Cpu
        );
    }

    #[test]
    fn missing_requirements_is_unsupported_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("mf")).unwrap();
        let m = EnvManager::new(PathBuf::from("uv"), dir.path().join("workers"), paths);
        assert!(!m.is_ready("nope", Backend::Cpu, "3.12"));
        let e = m.ensure("nope", Backend::Cpu, "3.12", &|_| {}).unwrap_err();
        assert_eq!(e.code, ErrorCode::Unsupported);
    }
}
