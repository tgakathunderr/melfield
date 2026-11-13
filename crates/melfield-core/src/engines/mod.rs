//! Engines turn a chunk of text + a voice into audio. Native engines run
//! in-process (sherpa-onnx); Python engines run in supervised worker processes.

pub mod envs;
#[cfg(feature = "native-tts")]
pub mod native;
pub mod protocol;
pub mod supervisor;
pub mod worker;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::audio::Pcm;
use crate::error::MelResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Delivery {
    Calm,
    #[default]
    Natural,
    Expressive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VoiceTarget {
    /// Built-in speaker id/name of the recipe.
    Preset { preset: String },
    /// A clone: reference recording (+ transcript when known).
    Reference {
        ref_audio: PathBuf,
        ref_text: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthRequest {
    pub recipe_id: String,
    pub text: String,
    pub lang: String,
    pub voice: VoiceTarget,
    pub speed: f32,
    pub delivery: Delivery,
    /// Must be true for clone voices; engines refuse to return unwatermarked clone audio.
    pub watermark: bool,
}

#[derive(Debug, Clone)]
pub struct SynthOutput {
    pub pcm: Pcm,
    pub watermarked: bool,
}

/// Blocking engine interface. Called from a blocking thread, one chunk at a time.
pub trait Engine: Send + Sync {
    fn synthesize(
        &self,
        req: &SynthRequest,
        should_stop: &dyn Fn() -> bool,
    ) -> MelResult<SynthOutput>;
}
