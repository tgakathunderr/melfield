use misaki_rs::fallback::FallbackError;
use misaki_rs::{Fallback, Language, Lexicon, G2P};

use super::{lts, normalize};

/// OOV fallback: compound split against the lexicon, then letter rules,
/// then letter spelling for all-caps acronyms.
struct OovFallback {
    lexicon: Lexicon,
}

impl OovFallback {
    fn known(&self, word: &str) -> Option<String> {
        self.lexicon
            .get_word(word, "NN", None, None)
            .map(|(p, _)| p)
            .filter(|p| !p.is_empty())
    }

    /// Try "melfield" -> known("mel")? + known("field"), longest known right part first.
    fn compound(&self, word: &str) -> Option<String> {
        let chars: Vec<char> = word.chars().collect();
        let n = chars.len();
        if n < 6 {
            return None;
        }
        for split in 2..=n - 3 {
            let left: String = chars[..split].iter().collect();
            let right: String = chars[split..].iter().collect();
            if let Some(r) = self.known(&right) {
                let l = self
                    .known(&left)
                    .unwrap_or_else(|| lts::word_to_phonemes(&left));
                if l.is_empty() {
                    continue;
                }
                let r = r.replace('ˈ', "ˌ");
                return Some(format!("{}{}", lts::add_primary_stress(&l), r));
            }
        }
        None
    }
}

impl Fallback for OovFallback {
    fn phonemize(&self, word: &str) -> Result<String, FallbackError> {
        let letters: String = word.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.is_empty() {
            return Err(FallbackError::NoPhonemes {
                word: word.to_string(),
            });
        }
        if letters.len() <= 4 && letters.chars().all(|c| c.is_ascii_uppercase()) {
            return Ok(lts::spell_letters(&letters));
        }
        let lower = letters.to_lowercase();
        let ph = self
            .compound(&lower)
            .unwrap_or_else(|| lts::word_to_phonemes(&lower));
        if ph.is_empty() {
            Err(FallbackError::NoPhonemes {
                word: word.to_string(),
            })
        } else {
            Ok(ph)
        }
    }
}

/// English (US/GB) text → Kokoro phoneme string.
pub struct EnglishG2p {
    g2p: G2P,
}

impl EnglishG2p {
    pub fn new(british: bool) -> Self {
        let lang = if british {
            Language::EnglishGB
        } else {
            Language::EnglishUS
        };
        let fallback = OovFallback {
            lexicon: Lexicon::new(lang),
        };
        Self {
            g2p: G2P::with_fallback(lang, Some(Box::new(fallback))),
        }
    }

    /// Phonemize one sentence/chunk. Unknown symbols are dropped rather than
    /// passed to the model (the model would skip them anyway).
    pub fn phonemize(&self, text: &str) -> String {
        let prepared = normalize::english(text);
        match self.g2p.g2p(&prepared) {
            Ok((ph, _)) => ph
                .replace('❓', "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            Err(e) => {
                tracing::warn!(error = %e, "g2p failed; falling back to letter rules");
                prepared
                    .split_whitespace()
                    .map(lts::word_to_phonemes)
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phonemizes_without_unknown_markers() {
        let g = EnglishG2p::new(false);
        let ph =
            g.phonemize("Welcome to Melfield, it's 3:45 pm on Nov. 11 and the TTS costs $12.50.");
        assert!(!ph.contains('❓'), "{ph}");
        assert!(ph.contains("mˈɛl"), "{ph}");
        assert!(ph.contains("fˈɔɹTi") || ph.contains("fˈɔɹti"), "{ph}");
    }
}
