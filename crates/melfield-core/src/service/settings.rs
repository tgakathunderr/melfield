use serde::{Deserialize, Serialize};

use super::{CoreEvent, Melfield};
use crate::audio::codec::Format;
use crate::error::{ErrorCode, MelError, MelResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Compute {
    #[default]
    Auto,
    Gpu,
    Cpu,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub default_voice: String,
    pub compute: Compute,
    pub output_format: Format,
    pub api_enabled: bool,
    pub api_port: u16,
    pub api_require_token: bool,
    /// `None` = automatic (on for Linux, off elsewhere unless the field drops frames).
    pub reduced_effects: Option<bool>,
    pub check_updates: bool,
    pub keep_history: bool,
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_voice: "aria".into(),
            compute: Compute::Auto,
            output_format: Format::Wav,
            api_enabled: false,
            api_port: 5150,
            api_require_token: false,
            reduced_effects: None,
            check_updates: true,
            keep_history: true,
            onboarded: false,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> MelResult<()> {
        if self.api_port < 1024 {
            return Err(MelError::new(
                ErrorCode::Unsupported,
                "pick a port number above 1024",
            ));
        }
        Ok(())
    }
}

const KEY: &str = "settings";

impl Melfield {
    pub fn settings(&self) -> MelResult<Settings> {
        Ok(self.store.setting::<Settings>(KEY)?.unwrap_or_default())
    }

    pub fn update_settings(&self, next: Settings) -> MelResult<Settings> {
        next.validate()?;
        self.store.set_setting(KEY, &next)?;
        self.emit(CoreEvent::SettingsChanged);
        Ok(next)
    }

    pub fn prefer_cpu(&self) -> bool {
        matches!(self.settings().map(|s| s.compute), Ok(Compute::Cpu))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_json_fills_new_fields_with_defaults() {
        let s: Settings = serde_json::from_str(r#"{"defaultVoice":"mira"}"#).unwrap();
        assert_eq!(s.default_voice, "mira");
        assert_eq!(s.api_port, 5150);
        assert!(!s.api_enabled);
    }

    #[test]
    fn rejects_privileged_ports() {
        let s = Settings {
            api_port: 80,
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }
}
