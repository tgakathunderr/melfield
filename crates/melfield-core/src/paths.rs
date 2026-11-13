//! On-disk layout. Everything Melfield writes lives under one relocatable root.

use std::path::{Path, PathBuf};

use crate::error::{MelError, MelResult};

#[derive(Debug, Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    /// Platform default: `%APPDATA%\melfield`, `~/Library/Application Support/melfield`,
    /// `~/.local/share/melfield`. `MELFIELD_HOME` overrides (tests, portable installs).
    pub fn default_root() -> MelResult<PathBuf> {
        if let Ok(home) = std::env::var("MELFIELD_HOME") {
            return Ok(PathBuf::from(home));
        }
        directories::ProjectDirs::from("app", "melfield", "melfield")
            .map(|d| d.data_dir().to_path_buf())
            .ok_or_else(|| MelError::internal("no home directory"))
    }

    pub fn new(root: impl Into<PathBuf>) -> MelResult<Self> {
        let paths = Self { root: root.into() };
        paths.ensure()?;
        Ok(paths)
    }

    fn ensure(&self) -> MelResult<()> {
        for dir in [
            self.root.clone(),
            self.logs(),
            self.models(),
            self.history_audio(),
            self.clones(),
            self.tmp(),
            self.registry_cache(),
            self.envs(),
            self.samples(),
        ] {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn db(&self) -> PathBuf {
        self.root.join("melfield.db")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn models(&self) -> PathBuf {
        self.root.join("models")
    }
    pub fn model_dir(&self, recipe_id: &str, revision: &str) -> PathBuf {
        let short = &revision[..revision.len().min(12)];
        self.models().join(recipe_id).join(short)
    }
    pub fn history_audio(&self) -> PathBuf {
        self.root.join("audio").join("history")
    }
    pub fn clones(&self) -> PathBuf {
        self.root.join("clones")
    }
    pub fn clone_dir(&self, voice_id: &str) -> PathBuf {
        self.clones().join(voice_id)
    }
    pub fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }
    pub fn registry_cache(&self) -> PathBuf {
        self.root.join("registry-cache")
    }
    pub fn samples(&self) -> PathBuf {
        self.root.join("samples")
    }
    pub fn python(&self) -> PathBuf {
        self.root.join("python")
    }
    pub fn uv_cache(&self) -> PathBuf {
        self.root.join("uv-cache")
    }
    pub fn envs(&self) -> PathBuf {
        self.root.join("envs")
    }
    pub fn env_dir(&self, family: &str) -> PathBuf {
        self.envs().join(family)
    }

    /// Bytes used under a directory (best effort; unreadable entries are skipped).
    pub fn dir_size(path: &Path) -> u64 {
        fn walk(p: &Path) -> u64 {
            let Ok(rd) = std::fs::read_dir(p) else {
                return 0;
            };
            rd.flatten()
                .map(|e| match e.metadata() {
                    Ok(m) if m.is_dir() => walk(&e.path()),
                    Ok(m) => m.len(),
                    Err(_) => 0,
                })
                .sum()
        }
        walk(path)
    }

    /// Free bytes on the volume holding `path`.
    pub fn free_space(path: &Path) -> u64 {
        fs2::available_space(path).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_layout_and_measures_size() {
        let dir = tempfile::tempdir().unwrap();
        let p = Paths::new(dir.path().join("mf")).unwrap();
        assert!(p.models().is_dir());
        std::fs::write(p.tmp().join("a.bin"), vec![0u8; 1000]).unwrap();
        assert_eq!(Paths::dir_size(&p.tmp()), 1000);
        assert!(p
            .model_dir("kokoro", "57adb37fc0e8d4bd430cfc1b")
            .ends_with("57adb37fc0e8"));
    }
}
