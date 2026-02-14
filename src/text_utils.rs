//! Text splitting helpers for TTS alignment.

use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;

const MAX_DISPLAY_SENTENCE_CHARS: usize = 220;
const MAX_DISPLAY_SENTENCE_WORDS: usize = 36;

/// Very lightweight sentence splitter based on punctuation.
pub fn split_sentences(text: &str) -> Vec<String> {
    split_sentences_with_abbreviations(text, &ABBREVIATION_TOKENS)
}

fn split_sentences_with_abbreviations(text: &str, abbreviations: &HashSet<String>) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();

    for (idx, ch) in chars.iter().copied().enumerate() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?') && !period_is_abbreviation(&chars, idx, abbreviations) {
            push_sentence_with_soft_breaks(&mut sentences, &current);
            current.clear();
        }
    }

    push_sentence_with_soft_breaks(&mut sentences, &current);

    sentences
}

fn push_sentence_with_soft_breaks(out: &mut Vec<String>, sentence: &str) {
    if !sentence.chars().any(|c| !c.is_whitespace()) {
        return;
    }
    out.extend(split_oversized_sentence(
        sentence,
        MAX_DISPLAY_SENTENCE_CHARS,
        MAX_DISPLAY_SENTENCE_WORDS,
    ));
}

fn split_oversized_sentence(sentence: &str, max_chars: usize, max_words: usize) -> Vec<String> {
    if !exceeds_limits(sentence, max_chars, max_words) {
        return vec![sentence.to_string()];
    }

    let mut out = Vec::new();
    let mut current = String::new();
    for segment in split_on_soft_delimiters(sentence) {
        push_segment(&mut out, &mut current, &segment, max_chars, max_words);
    }
    if current.chars().any(|c| !c.is_whitespace()) {
        out.push(current);
    }

    if out.is_empty() {
        vec![sentence.to_string()]
    } else {
        out
    }
}

fn split_on_soft_delimiters(sentence: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    for ch in sentence.chars() {
        current.push(ch);
        if matches!(ch, ',' | ';' | ':' | '\n') {
            if current.chars().any(|c| !c.is_whitespace()) {
                segments.push(current.clone());
            }
            current.clear();
        }
    }
    if current.chars().any(|c| !c.is_whitespace()) {
        segments.push(current);
    }
    if segments.is_empty() {
        vec![sentence.to_string()]
    } else {
        segments
    }
}

fn push_segment(
    out: &mut Vec<String>,
    current: &mut String,
    segment: &str,
    max_chars: usize,
    max_words: usize,
) {
    if !segment.chars().any(|c| !c.is_whitespace()) {
        return;
    }

    if exceeds_limits(segment, max_chars, max_words) {
        if current.chars().any(|c| !c.is_whitespace()) {
            out.push(std::mem::take(current));
        }
        for chunk in split_segment_by_words(segment, max_chars, max_words) {
            out.push(chunk);
        }
        return;
    }

    let candidate = format!("{current}{segment}");
    if !current.is_empty() && exceeds_limits(&candidate, max_chars, max_words) {
        out.push(std::mem::take(current));
        current.push_str(segment);
    } else {
        *current = candidate;
    }
}

fn split_segment_by_words(segment: &str, max_chars: usize, max_words: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut current_words = 0usize;
    for word in segment.split_whitespace() {
        let word_chars = word.chars().count();
        let candidate_chars = if current.is_empty() {
            word_chars
        } else {
            current.chars().count() + 1 + word_chars
        };
        let candidate_words = current_words + 1;
        if !current.is_empty() && (candidate_chars > max_chars || candidate_words > max_words) {
            out.push(std::mem::take(&mut current));
            current_words = 0;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
        current_words += 1;
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn exceeds_limits(text: &str, max_chars: usize, max_words: usize) -> bool {
    text.chars().count() > max_chars || text.split_whitespace().count() > max_words
}

fn period_is_abbreviation(chars: &[char], dot_idx: usize, abbreviations: &HashSet<String>) -> bool {
    if chars.get(dot_idx).copied() != Some('.') {
        return false;
    }

    let mut start = dot_idx;
    while start > 0 && chars[start - 1].is_alphabetic() {
        start -= 1;
    }
    if start == dot_idx {
        return false;
    }

    let token: String = chars[start..dot_idx].iter().collect();
    if token.is_empty() {
        return false;
    }

    let lookup = format!("{}.", token.to_ascii_lowercase());
    if abbreviations.contains(&lookup) {
        return true;
    }

    if token.len() == 1 {
        // Treat interior periods in initialisms like "U.S." as non-terminal.
        if start >= 2 && chars[start - 1] == '.' && chars[start - 2].is_alphabetic() {
            return true;
        }

        // Also avoid splitting at the first period when another "X." follows.
        let mut next = dot_idx + 1;
        while next < chars.len() && chars[next].is_whitespace() {
            next += 1;
        }
        if next + 1 < chars.len() && chars[next].is_alphabetic() && chars[next + 1] == '.' {
            return true;
        }
    }

    false
}

static ABBREVIATION_TOKENS: Lazy<HashSet<String>> = Lazy::new(load_abbreviation_tokens);

fn load_abbreviation_tokens() -> HashSet<String> {
    let mut out = HashSet::new();
    for default in ["mr.", "ms.", "mrs.", "mass.", "st."] {
        out.insert(default.to_string());
    }

    let path = PathBuf::from("conf/normalizer.toml");
    let Ok(contents) = fs::read_to_string(&path) else {
        return out;
    };
    if let Ok(file) = toml::from_str::<NormalizerFile>(&contents) {
        for key in file.normalization.abbreviations.keys() {
            let normalized = normalize_abbreviation_token(key);
            if !normalized.is_empty() {
                out.insert(normalized);
            }
        }
    }
    out
}

fn normalize_abbreviation_token(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{}.", trimmed.to_ascii_lowercase())
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct NormalizerFile {
    normalization: NormalizationConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct NormalizationConfig {
    abbreviations: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::split_sentences;

    #[test]
    fn does_not_split_common_abbreviations() {
        let text = "Mr. Smith walked in. Mrs. Jones stayed.";
        let sentences = split_sentences(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn keeps_initialism_together() {
        let text = "This uses U.S. spelling. Next sentence.";
        let sentences = split_sentences(text);
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn splits_oversized_comma_heavy_sentence_for_display_alignment() {
        let text = "alpha, beta, gamma, delta, epsilon, zeta, eta, theta, iota, kappa, lambda, \
                    mu, nu, xi, omicron, pi, rho, sigma, tau, upsilon, phi, chi, psi, omega, \
                    alpha, beta, gamma, delta, epsilon, zeta, eta, theta, iota, kappa, lambda, \
                    mu, nu, xi, omicron, pi, rho, sigma, tau, upsilon, phi, chi, psi, omega.";
        let sentences = split_sentences(text);
        assert!(
            sentences.len() > 1,
            "long comma-heavy run should be split into multiple display sentences"
        );
        assert!(
            sentences
                .iter()
                .all(|s| s.chars().count() <= 220 && s.split_whitespace().count() <= 36),
            "split display sentences should stay within configured readability limits"
        );
    }

    #[test]
    fn keeps_short_comma_sentence_intact() {
        let text = "Alpha, beta, and gamma are fine.";
        let sentences = split_sentences(text);
        assert_eq!(sentences.len(), 1);
    }
}
