//! Human errors. Every failure that can reach a person carries a calm, lowercase
//! message and (where possible) a fix action. Technical detail goes to the log only.

use serde::Serialize;

/// Stable machine-readable error codes (also used by the worker protocol and API).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    DiskFull,
    Offline,
    DownloadFailed,
    ChecksumMismatch,
    VoiceNotInstalled,
    VoiceNotFound,
    EngineStarting,
    EngineCrashed,
    EngineBroken,
    OutOfMemory,
    Unsupported,
    NeedsSignIn,
    NeedsAccess,
    LicenseNotAccepted,
    ConsentRequired,
    WatermarkUnavailable,
    BadAudio,
    TooShort,
    TooLong,
    EmptyText,
    NoMicrophone,
    NoAudioOutput,
    Cancelled,
    NotFound,
    Busy,
    Internal,
}

/// What the UI should offer as the one-tap fix.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixAction {
    Retry,
    FreeSpace { needed_bytes: u64 },
    Repair { family: String },
    SignIn,
    OpenPage { url: String },
    GetVoice { voice_id: String },
    InstallRecipe { recipe_id: String },
    AcceptLicense { recipe_id: String },
    OpenSettings,
}

#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[error("{code:?}: {message} ({detail})")]
pub struct MelError {
    pub code: ErrorCode,
    /// Human, lowercase, never technical.
    pub message: String,
    pub fix: Option<FixAction>,
    /// Technical detail for the log file. Never shown in the UI.
    #[serde(skip)]
    pub detail: String,
}

pub type MelResult<T> = Result<T, MelError>;

impl MelError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            fix: None,
            detail: String::new(),
        }
    }

    pub fn with_fix(mut self, fix: FixAction) -> Self {
        self.fix = Some(fix);
        self
    }

    pub fn with_detail(mut self, detail: impl std::fmt::Display) -> Self {
        self.detail = detail.to_string();
        self
    }

    /// Wrap an unexpected internal failure. The person sees a calm generic line;
    /// the cause is logged.
    pub fn internal(detail: impl std::fmt::Display) -> Self {
        let e = Self::new(
            ErrorCode::Internal,
            "something went sideways — try that again",
        )
        .with_fix(FixAction::Retry)
        .with_detail(detail);
        tracing::error!(detail = %e.detail, "internal error");
        e
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorCode::Cancelled, "stopped")
    }

    pub fn disk_full(needed_bytes: u64) -> Self {
        Self::new(
            ErrorCode::DiskFull,
            format!(
                "that voice needs a bit more space — free up {} and try again",
                human_bytes(needed_bytes)
            ),
        )
        .with_fix(FixAction::FreeSpace { needed_bytes })
    }

    pub fn offline() -> Self {
        Self::new(
            ErrorCode::Offline,
            "can't reach the internet right now — your installed voices still work",
        )
        .with_fix(FixAction::Retry)
    }

    pub fn voice_not_installed(voice_id: &str, name: &str) -> Self {
        Self::new(
            ErrorCode::VoiceNotInstalled,
            format!("{} isn't on this computer yet", name.to_lowercase()),
        )
        .with_fix(FixAction::GetVoice {
            voice_id: voice_id.to_string(),
        })
    }

    pub fn engine_broken(family: &str) -> Self {
        Self::new(
            ErrorCode::EngineBroken,
            "this voice needs a quick repair — it only takes a moment",
        )
        .with_fix(FixAction::Repair {
            family: family.to_string(),
        })
    }

    pub fn out_of_memory() -> Self {
        Self::new(
            ErrorCode::OutOfMemory,
            "your computer ran out of room for this one — try a shorter line or close other apps",
        )
        .with_fix(FixAction::Retry)
    }
}

impl From<std::io::Error> for MelError {
    fn from(e: std::io::Error) -> Self {
        if is_disk_full(&e) {
            return MelError::disk_full(0).with_detail(e);
        }
        MelError::internal(e)
    }
}

impl From<rusqlite::Error> for MelError {
    fn from(e: rusqlite::Error) -> Self {
        MelError::internal(format!("db: {e}"))
    }
}

impl From<serde_json::Error> for MelError {
    fn from(e: serde_json::Error) -> Self {
        MelError::internal(format!("json: {e}"))
    }
}

fn is_disk_full(e: &std::io::Error) -> bool {
    // ENOSPC = 28 on unix, ERROR_DISK_FULL = 112 / ERROR_HANDLE_DISK_FULL = 39 on windows.
    matches!(e.raw_os_error(), Some(28) | Some(112) | Some(39))
}

/// Format bytes the way a person reads them: "1.2 GB", "340 MB".
pub fn human_bytes(bytes: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    const MB: f64 = 1_000_000.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.0} MB", (b / MB).max(1.0))
    } else {
        format!("{:.0} KB", (b / 1000.0).max(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_formats_like_a_person() {
        assert_eq!(human_bytes(1_200_000_000), "1.2 GB");
        assert_eq!(human_bytes(340_000_000), "340 MB");
        assert_eq!(human_bytes(12_000), "12 KB");
        assert_eq!(human_bytes(10), "1 KB");
    }

    #[test]
    fn disk_full_message_is_human_and_has_fix() {
        let e = MelError::disk_full(1_200_000_000);
        assert!(e.message.contains("free up 1.2 GB"));
        assert!(e.message.starts_with("that voice"));
        assert_eq!(
            e.fix,
            Some(FixAction::FreeSpace {
                needed_bytes: 1_200_000_000
            })
        );
    }

    #[test]
    fn detail_is_not_serialized() {
        let e = MelError::internal("panic at foo.rs:12");
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.contains("foo.rs"));
    }
}
