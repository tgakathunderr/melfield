//! Rule-based English letter-to-sound for words no dictionary knows
//! (names, brands, coinages). Output uses misaki's US phoneme alphabet so it
//! can be fed straight to Kokoro/Kitten. Deliberately simple and GPL-free.

/// Ordered grapheme rules: (pattern, phonemes). Longest patterns first.
const RULES: &[(&str, &str)] = &[
    ("tion", "ʃən"),
    ("sion", "ʒən"),
    ("ture", "ʧəɹ"),
    ("eigh", "A"),
    ("ough", "O"),
    ("augh", "ɔ"),
    ("tch", "ʧ"),
    ("dge", "ʤ"),
    ("igh", "I"),
    ("sch", "sk"),
    ("ch", "ʧ"),
    ("sh", "ʃ"),
    ("th", "θ"),
    ("ph", "f"),
    ("wh", "w"),
    ("ck", "k"),
    ("ng", "ŋ"),
    ("qu", "kw"),
    ("gh", "ɡ"),
    ("ee", "i"),
    ("ea", "i"),
    ("ie", "i"),
    ("oo", "u"),
    ("ou", "W"),
    ("ow", "O"),
    ("ai", "A"),
    ("ay", "A"),
    ("ei", "A"),
    ("ey", "A"),
    ("oi", "Y"),
    ("oy", "Y"),
    ("au", "ɔ"),
    ("aw", "ɔ"),
    ("oa", "O"),
    ("ue", "u"),
    ("ew", "ju"),
    ("ar", "ɑɹ"),
    ("er", "əɹ"),
    ("ir", "ɜɹ"),
    ("ur", "ɜɹ"),
    ("or", "ɔɹ"),
];

const VOWELS: &[char] = &['a', 'e', 'i', 'o', 'u'];
/// Phoneme characters that count as syllable nuclei (for stress placement).
const NUCLEI: &[char] = &[
    'A', 'I', 'O', 'W', 'Y', 'a', 'æ', 'ɑ', 'ɔ', 'ə', 'ɛ', 'ɜ', 'ɪ', 'ʊ', 'ʌ', 'i', 'u',
];

fn is_vowel(c: char) -> bool {
    VOWELS.contains(&c)
}

fn short_vowel(c: char) -> &'static str {
    match c {
        'a' => "æ",
        'e' => "ɛ",
        'i' => "ɪ",
        'o' => "ɑ",
        _ => "ʌ",
    }
}

fn long_vowel(c: char) -> &'static str {
    match c {
        'a' => "A",
        'e' => "i",
        'i' => "I",
        'o' => "O",
        _ => "u",
    }
}

/// Convert a single lowercase alphabetic word to phonemes (with primary stress).
pub fn word_to_phonemes(word: &str) -> String {
    let w: Vec<char> = word
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect();
    if w.is_empty() {
        return String::new();
    }
    let n = w.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        // Silent initial clusters.
        if i == 0 && n > 2 {
            let pair: String = w[0..2].iter().collect();
            if pair == "kn" || pair == "gn" {
                out.push('n');
                i = 2;
                continue;
            }
            if pair == "wr" {
                out.push('ɹ');
                i = 2;
                continue;
            }
        }
        // Final "le" after a consonant: "-tle" -> "təl".
        if i + 2 == n && w[i] == 'l' && w[i + 1] == 'e' && i > 0 && !is_vowel(w[i - 1]) {
            out.push_str("əl");
            break;
        }
        // Silent final e (but not for tiny words like "be").
        if i == n - 1 && w[i] == 'e' && n > 2 {
            break;
        }
        if let Some((pat, ph)) = RULES.iter().find(|(pat, _)| {
            let len = pat.chars().count();
            let matches = i + len <= n && w[i..i + len].iter().copied().eq(pat.chars());
            // r-coloured vowels only before a consonant or word end ("mira" keeps ɪ+ɹ).
            let r_before_vowel = pat.ends_with('r') && w.get(i + len).is_some_and(|c| is_vowel(*c));
            matches && !r_before_vowel
        }) {
            out.push_str(ph);
            i += pat.chars().count();
            continue;
        }
        let c = w[i];
        let next = w.get(i + 1).copied();
        if is_vowel(c) {
            // Magic e: vowel + single consonant + final e -> long vowel.
            let magic = i + 2 == n - 1 && w[n - 1] == 'e' && !is_vowel(w[i + 1]);
            let final_a = c == 'a' && i == n - 1 && n > 2;
            out.push_str(if magic {
                long_vowel(c)
            } else if final_a {
                "ə"
            } else {
                short_vowel(c)
            });
            i += 1;
            continue;
        }
        // Skip doubled consonants.
        if next == Some(c) {
            i += 1;
            continue;
        }
        let ph = match c {
            'c' if matches!(next, Some('e' | 'i' | 'y')) => "s",
            'c' => "k",
            'g' if matches!(next, Some('e' | 'i' | 'y')) && i > 0 => "ʤ",
            'g' => "ɡ",
            'j' => "ʤ",
            'x' if i == 0 => "z",
            'x' => "ks",
            'y' if i == 0 => "j",
            'y' if i == n - 1 => "i",
            'y' => "ɪ",
            'r' => "ɹ",
            's' if i == n - 1 && i > 0 && !matches!(w[i - 1], 'p' | 't' | 'k' | 'f') => "z",
            'q' => "k",
            other => {
                out.push(other);
                i += 1;
                continue;
            }
        };
        out.push_str(ph);
        i += 1;
    }
    add_primary_stress(&out)
}

/// Put ˈ before the first syllable nucleus (good enough for most names).
pub fn add_primary_stress(phonemes: &str) -> String {
    if phonemes.contains('ˈ') {
        return phonemes.to_string();
    }
    match phonemes.char_indices().find(|(_, c)| NUCLEI.contains(c)) {
        Some((idx, _)) => format!("{}ˈ{}", &phonemes[..idx], &phonemes[idx..]),
        None => phonemes.to_string(),
    }
}

/// Spell out letters for acronyms the dictionary doesn't know ("XQZ" -> "ɛks kju zi").
pub fn spell_letters(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| match c.to_ascii_lowercase() {
            'a' => "ˈA",
            'b' => "bˈi",
            'c' => "sˈi",
            'd' => "dˈi",
            'e' => "ˈi",
            'f' => "ˈɛf",
            'g' => "ʤˈi",
            'h' => "ˈAʧ",
            'i' => "ˈI",
            'j' => "ʤˈA",
            'k' => "kˈA",
            'l' => "ˈɛl",
            'm' => "ˈɛm",
            'n' => "ˈɛn",
            'o' => "ˈO",
            'p' => "pˈi",
            'q' => "kjˈu",
            'r' => "ˈɑɹ",
            's' => "ˈɛs",
            't' => "tˈi",
            'u' => "jˈu",
            'v' => "vˈi",
            'w' => "dˈʌbəljˌu",
            'x' => "ˈɛks",
            'y' => "wˈI",
            _ => "zˈi",
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_names_and_coinages() {
        assert_eq!(word_to_phonemes("melfield"), "mˈɛlfild");
        assert_eq!(word_to_phonemes("kabir"), "kˈæbɜɹ");
        assert_eq!(word_to_phonemes("mira"), "mˈɪɹə");
        assert_eq!(word_to_phonemes("blake"), "blˈAk");
        assert_eq!(word_to_phonemes("knight"), "nˈIt");
        assert_eq!(word_to_phonemes("station"), "stˈæʃən");
    }

    #[test]
    fn never_panics_on_odd_input() {
        assert_eq!(word_to_phonemes(""), "");
        assert_eq!(word_to_phonemes("123"), "");
        let _ = word_to_phonemes("e");
        let _ = word_to_phonemes("le");
        let _ = word_to_phonemes("xyzzy");
    }

    #[test]
    fn spells_acronyms() {
        assert_eq!(spell_letters("TTS"), "tˈi tˈi ˈɛs");
    }
}
