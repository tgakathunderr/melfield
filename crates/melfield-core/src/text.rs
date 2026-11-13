//! Long-text handling: split anything (a line, a script, a chapter) into
//! engine-sized chunks on natural boundaries so audio can stream and stitch.

use unicode_segmentation::UnicodeSegmentation;

/// Hard limit on a single request; beyond this we still work, chunk by chunk.
pub const MAX_TEXT_CHARS: usize = 2_000_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub index: usize,
    pub text: String,
    /// True when the chunk ends a paragraph — stitched with a longer pause.
    pub paragraph_end: bool,
}

/// Normalise whitespace and strip characters engines choke on, keeping meaning.
pub fn clean(text: &str) -> String {
    let replaced: String = text
        .chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{02BC}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{00AB}' | '\u{00BB}' => '"',
            '\u{2013}' | '\u{2014}' => ',',
            '\u{2026}' => '.',
            '\t' | '\u{00A0}' => ' ',
            c if c.is_control() && c != '\n' => ' ',
            c => c,
        })
        .collect();
    let mut out = String::with_capacity(replaced.len());
    for (i, para) in paragraphs(&replaced).iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&para.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    out
}

fn paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n")
        .flat_map(|p| p.split('\n'))
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Split into sentences, aware of Devanagari danda (। ॥) and CJK full stops.
pub fn sentences(paragraph: &str) -> Vec<String> {
    let mut out = Vec::new();
    for s in paragraph.split_sentence_bounds() {
        // unicode-segmentation already handles . ! ? and CJK; add danda splits.
        let mut start = 0;
        for (i, c) in s.char_indices() {
            if c == '।' || c == '॥' {
                let end = i + c.len_utf8();
                push_trimmed(&mut out, &s[start..end]);
                start = end;
            }
        }
        push_trimmed(&mut out, &s[start..]);
    }
    merge_tiny(out)
}

fn push_trimmed(out: &mut Vec<String>, s: &str) {
    let t = s.trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
}

/// "Dr." / "e.g." style fragments create tiny sentences; glue them forward.
fn merge_tiny(sentences: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut carry = String::new();
    for s in sentences {
        let candidate = if carry.is_empty() {
            s
        } else {
            format!("{carry} {s}")
        };
        if candidate.chars().filter(|c| c.is_alphanumeric()).count() < 4 {
            carry = candidate;
        } else {
            out.push(candidate);
            carry.clear();
        }
    }
    if !carry.is_empty() {
        match out.last_mut() {
            Some(last) => {
                last.push(' ');
                last.push_str(&carry);
            }
            None => out.push(carry),
        }
    }
    out
}

/// Break an over-long sentence at clause punctuation, then at spaces.
fn split_long(sentence: &str, max_chars: usize) -> Vec<String> {
    if sentence.chars().count() <= max_chars {
        return vec![sentence.to_string()];
    }
    let mut parts = Vec::new();
    let mut current = String::new();
    for word in sentence.split_inclusive([',', ';', ':', '،', '、', '，']) {
        if current.chars().count() + word.chars().count() > max_chars && !current.is_empty() {
            parts.push(current.trim().to_string());
            current.clear();
        }
        current.push_str(word);
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
        .into_iter()
        .flat_map(|p| {
            if p.chars().count() <= max_chars {
                return vec![p];
            }
            let mut words_out = Vec::new();
            let mut buf = String::new();
            for w in p.split_whitespace() {
                if buf.chars().count() + w.chars().count() + 1 > max_chars && !buf.is_empty() {
                    words_out.push(std::mem::take(&mut buf));
                }
                if !buf.is_empty() {
                    buf.push(' ');
                }
                buf.push_str(w);
            }
            if !buf.is_empty() {
                words_out.push(buf);
            }
            words_out
        })
        .collect()
}

/// Turn any text into chunks. The first chunk is kept short so the first
/// sound arrives fast; later chunks pack sentences up to `max_chars`.
pub fn chunk(text: &str, max_chars: usize) -> Vec<Chunk> {
    let max_chars = max_chars.max(40);
    let first_max = (max_chars / 2).max(60);
    let cleaned = clean(text);
    let mut chunks: Vec<Chunk> = Vec::new();
    for para in cleaned.split("\n\n") {
        let pieces: Vec<String> = sentences(para)
            .iter()
            .flat_map(|s| split_long(s, max_chars))
            .collect();
        let mut buf = String::new();
        for piece in pieces {
            let limit = if chunks.is_empty() {
                first_max
            } else {
                max_chars
            };
            if !buf.is_empty() && buf.chars().count() + piece.chars().count() + 1 > limit {
                chunks.push(Chunk {
                    index: chunks.len(),
                    text: std::mem::take(&mut buf),
                    paragraph_end: false,
                });
            }
            if !buf.is_empty() {
                buf.push(' ');
            }
            buf.push_str(&piece);
        }
        if !buf.is_empty() {
            chunks.push(Chunk {
                index: chunks.len(),
                text: buf,
                paragraph_end: true,
            });
        }
    }
    chunks
}

/// Guess the language from the dominant script. Latin text returns `None`
/// (the voice's own language is used). Devanagari defaults to Hindi.
pub fn detect_script_language(text: &str) -> Option<&'static str> {
    let mut counts: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::new();
    let mut latin = 0usize;
    for c in text.chars() {
        let lang = match c as u32 {
            0x0900..=0x097F => "hi",
            0x0980..=0x09FF => "bn",
            0x0A00..=0x0A7F => "pa",
            0x0A80..=0x0AFF => "gu",
            0x0B00..=0x0B7F => "or",
            0x0B80..=0x0BFF => "ta",
            0x0C00..=0x0C7F => "te",
            0x0C80..=0x0CFF => "kn",
            0x0D00..=0x0D7F => "ml",
            0x0600..=0x06FF => "ar",
            0x0400..=0x04FF => "ru",
            0x0370..=0x03FF => "el",
            0x0590..=0x05FF => "he",
            0x3040..=0x30FF => "ja",
            0xAC00..=0xD7AF => "ko",
            0x4E00..=0x9FFF => "zh",
            _ => {
                if c.is_ascii_alphabetic() {
                    latin += 1;
                }
                continue;
            }
        };
        *counts.entry(lang).or_default() += 1;
    }
    // Kana anywhere means Japanese even when kanji dominate.
    if counts.contains_key("ja") {
        return Some("ja");
    }
    counts
        .into_iter()
        .filter(|(_, n)| *n > latin)
        .max_by_key(|(_, n)| *n)
        .map(|(l, _)| l)
}

#[cfg(test)]
mod tests {
    #[test]
    fn detects_scripts() {
        assert_eq!(detect_script_language("नमस्ते, आप कैसे हैं?"), Some("hi"));
        assert_eq!(detect_script_language("வணக்கம்"), Some("ta"));
        assert_eq!(detect_script_language("hello there"), None);
        assert_eq!(detect_script_language("今日はいい天気"), Some("ja"));
        assert_eq!(
            detect_script_language("Hello नमस्ते my friend how are you"),
            None
        );
    }

    use super::*;

    #[test]
    fn splits_english_sentences_and_keeps_abbreviations() {
        let s = sentences("Dr. Rao went home. He slept! Did he dream?");
        assert_eq!(s, vec!["Dr. Rao went home.", "He slept!", "Did he dream?"]);
    }

    #[test]
    fn splits_hindi_on_danda() {
        let s = sentences("नमस्ते दुनिया। आप कैसे हैं।");
        assert_eq!(s.len(), 2);
        assert!(s[0].ends_with('।'));
    }

    #[test]
    fn chunks_respect_limits_and_first_chunk_is_short() {
        let text = "This is a sentence that has some words in it. ".repeat(40);
        let chunks = chunk(&text, 300);
        assert!(chunks.len() > 3);
        assert!(chunks[0].text.chars().count() <= 150);
        for c in &chunks {
            assert!(
                c.text.chars().count() <= 300,
                "chunk too long: {}",
                c.text.len()
            );
        }
        let joined: String = chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            joined.split_whitespace().count(),
            text.split_whitespace().count()
        );
    }

    #[test]
    fn very_long_sentence_without_punctuation_is_split_on_spaces() {
        let text = "word ".repeat(500);
        let chunks = chunk(&text, 200);
        assert!(chunks.iter().all(|c| c.text.chars().count() <= 200));
        let words: usize = chunks
            .iter()
            .map(|c| c.text.split_whitespace().count())
            .sum();
        assert_eq!(words, 500);
    }

    #[test]
    fn paragraphs_are_marked() {
        let chunks = chunk("First para here.\n\nSecond para here.", 400);
        assert_eq!(chunks.len(), 2);
        assert!(chunks.iter().all(|c| c.paragraph_end));
    }

    #[test]
    fn clean_normalises_quotes_and_whitespace() {
        assert_eq!(clean("“Hi”\t—  there…"), "\"Hi\" , there.");
        assert_eq!(chunk("   \n  ", 300), vec![]);
    }
}
