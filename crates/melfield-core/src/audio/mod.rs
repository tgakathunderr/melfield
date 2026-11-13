//! Audio: DSP, codecs, spectrum, and native I/O (feature `audio-io`).

pub mod codec;
pub mod dsp;
pub mod spectrum;

#[cfg(feature = "audio-io")]
pub mod player;
#[cfg(feature = "audio-io")]
pub mod recorder;

pub use dsp::Pcm;
