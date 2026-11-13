//! Grapheme-to-phoneme frontends for native voices. GPL-free by design
//! (espeak-ng is GPL-3.0, so it is never linked): English uses misaki-rs (MIT, embedded
//! lexicons + POS tagger) with our own out-of-vocabulary fallback.

pub mod lts;
pub mod normalize;

#[cfg(feature = "native-tts")]
mod english;
#[cfg(feature = "native-tts")]
pub use english::EnglishG2p;

/// Languages we can phonemize natively today.
pub fn native_language_supported(lang: &str) -> bool {
    matches!(lang, "en" | "en-us" | "en-gb")
}
