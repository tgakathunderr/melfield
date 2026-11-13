//! Storage: disk usage, moving the data folder, exporting logs.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Melfield;
use crate::error::{human_bytes, ErrorCode, MelError, MelResult};
use crate::paths::Paths;

const POINTER_FILE: &str = "location.json";

#[derive(Serialize, Deserialize)]
struct Pointer {
    root: PathBuf,
}

/// Default root, or the moved location recorded in a pointer file there.
pub(crate) fn resolve_root() -> MelResult<PathBuf> {
    let default = Paths::default_root()?;
    let pointer = default.join(POINTER_FILE);
    if let Ok(text) = std::fs::read_to_string(&pointer) {
        if let Ok(p) = serde_json::from_str::<Pointer>(&text) {
            if p.root.is_dir() {
                return Ok(p.root);
            }
            tracing::warn!(root = %p.root.display(), "moved storage not found; using default");
        }
    }
    Ok(default)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub voices_bytes: u64,
    pub engines_bytes: u64,
    pub history_bytes: u64,
    pub clones_bytes: u64,
    pub free_bytes: u64,
    pub total_label: String,
    pub location: String,
}

impl Melfield {
    pub fn disk_usage(&self) -> DiskUsage {
        let p = &self.paths;
        let voices = Paths::dir_size(&p.models());
        let engines = Paths::dir_size(&p.envs())
            + Paths::dir_size(&p.python())
            + Paths::dir_size(&p.uv_cache());
        let history = Paths::dir_size(&p.history_audio());
        let clones = Paths::dir_size(&p.clones());
        DiskUsage {
            voices_bytes: voices,
            engines_bytes: engines,
            history_bytes: history,
            clones_bytes: clones,
            free_bytes: Paths::free_space(p.root()),
            total_label: human_bytes(voices + engines + history + clones),
            location: p.root().display().to_string(),
        }
    }

    /// Copy everything to `dest`, then point the app at it. Takes effect on restart.
    /// Python envs are not copied (they embed absolute paths) — they rebuild on demand.
    pub fn move_storage(&self, dest: &Path) -> MelResult<()> {
        let dest = dest.join("melfield");
        if dest.starts_with(self.paths.root()) {
            return Err(MelError::new(
                ErrorCode::Unsupported,
                "pick a folder outside the current one",
            ));
        }
        let needed = Paths::dir_size(self.paths.root());
        std::fs::create_dir_all(&dest)?;
        if Paths::free_space(&dest) < needed + 100_000_000 {
            return Err(MelError::disk_full(needed + 100_000_000));
        }
        self.supervisor.shutdown_all();
        for sub in [
            "models",
            "audio",
            "clones",
            "registry-cache",
            "samples",
            "logs",
        ] {
            copy_dir(&self.paths.root().join(sub), &dest.join(sub))?;
        }
        std::fs::copy(self.paths.db(), dest.join("melfield.db"))?;
        let default = Paths::default_root()?;
        std::fs::create_dir_all(&default)?;
        std::fs::write(
            default.join(POINTER_FILE),
            serde_json::to_vec(&Pointer { root: dest })?,
        )?;
        Ok(())
    }

    /// Zip the log folder for "export logs". Hugging Face tokens never reach logs,
    /// but scrub anything token-shaped defensively.
    pub fn export_logs(&self, dest: &Path) -> MelResult<()> {
        let file = std::fs::File::create(dest)?;
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let token_re = regex::Regex::new(r"hf_[A-Za-z0-9]{20,}").map_err(MelError::internal)?;
        for entry in std::fs::read_dir(self.paths.logs())?.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let scrubbed = token_re.replace_all(&text, "hf_[redacted]");
            zip.start_file(entry.file_name().to_string_lossy(), opts)
                .map_err(MelError::internal)?;
            zip.write_all(scrubbed.as_bytes())?;
        }
        let hw = serde_json::to_string_pretty(&self.hardware)?;
        zip.start_file("hardware.json", opts)
            .map_err(MelError::internal)?;
        zip.write_all(hw.as_bytes())?;
        zip.finish().map_err(MelError::internal)?;
        Ok(())
    }
}

fn copy_dir(src: &Path, dst: &Path) -> MelResult<()> {
    if !src.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        let (s, d) = (entry.path(), dst.join(entry.file_name()));
        if s.is_dir() {
            copy_dir(&s, &d)?;
        } else {
            std::fs::copy(&s, &d)?;
        }
    }
    Ok(())
}
