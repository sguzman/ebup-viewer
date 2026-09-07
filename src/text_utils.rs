//! Text splitting helpers for TTS alignment.

use once_cell::sync::Lazy;
use regex::Regex;
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

fn split_sentences_with_abbreviations(
    text: &str,
    abbreviations: &AbbreviationTokenSet,
) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let protected_periods = protected_period_indices(text, abbreviations);

    for (idx, ch) in chars.iter().copied().enumerate() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?')
            && !(ch == '.' && protected_periods.contains(&idx))
            && !period_is_abbreviation(&chars, idx, abbreviations)
        {
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

fn period_is_abbreviation(
    chars: &[char],
    dot_idx: usize,
    abbreviations: &AbbreviationTokenSet,
) -> bool {
    if chars.get(dot_idx).copied() != Some('.') {
        return false;
    }

    // Keep decimal numbers intact (e.g. 3.14) so "." does not terminate sentence.
    if dot_idx > 0
        && dot_idx + 1 < chars.len()
        && chars[dot_idx - 1].is_ascii_digit()
        && chars[dot_idx + 1].is_ascii_digit()
    {
        return true;
    }

    if period_is_domain_tld(chars, dot_idx) {
        return true;
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

    let lookup_nocase = format!("{}.", token.to_ascii_lowercase());
    if abbreviations.nocase.contains(&lookup_nocase) {
        return true;
    }
    let lookup_case = format!("{token}.");
    if abbreviations.case.contains(&lookup_case) {
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

        // Handle middle initials in names like "James B. Allen".
        if next < chars.len() && chars[next].is_uppercase() {
            return true;
        }
    }

    false
}

fn protected_period_indices(text: &str, abbreviations: &AbbreviationTokenSet) -> HashSet<usize> {
    if abbreviations.regex.is_empty() {
        return HashSet::new();
    }

    let period_positions: Vec<(usize, usize)> = text
        .char_indices()
        .enumerate()
        .filter_map(|(char_idx, (byte_idx, ch))| (ch == '.').then_some((byte_idx, char_idx)))
        .collect();

    if period_positions.is_empty() {
        return HashSet::new();
    }

    let mut out = HashSet::new();
    for re in &abbreviations.regex {
        for mat in re.find_iter(text) {
            for (period_byte_idx, period_char_idx) in &period_positions {
                if *period_byte_idx >= mat.start() && *period_byte_idx < mat.end() {
                    out.insert(*period_char_idx);
                }
            }
        }
    }
    out
}

fn period_is_domain_tld(chars: &[char], dot_idx: usize) -> bool {
    if dot_idx == 0 || dot_idx + 1 >= chars.len() || chars[dot_idx] != '.' {
        return false;
    }
    if !chars[dot_idx + 1].is_ascii_alphabetic() {
        return false;
    }

    let mut label_start = dot_idx;
    while label_start > 0 {
        let ch = chars[label_start - 1];
        if ch.is_ascii_alphanumeric() || ch == '-' {
            label_start -= 1;
        } else {
            break;
        }
    }
    if label_start == dot_idx {
        return false;
    }

    let mut tld_end = dot_idx + 1;
    while tld_end < chars.len() && chars[tld_end].is_ascii_alphabetic() {
        tld_end += 1;
    }
    let tld = chars[dot_idx + 1..tld_end]
        .iter()
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        tld.as_str(),
        "net" | "com" | "gov" | "org" | "uk" | "mx" | "cn" | "de" | "ru" | "br"
    )
}

static ABBREVIATION_TOKENS: Lazy<AbbreviationTokenSet> = Lazy::new(load_abbreviation_tokens);

fn load_abbreviation_tokens() -> AbbreviationTokenSet {
    let normalizer_path = resolve_normalizer_config_path();
    let abbreviations_path = resolve_abbreviations_config_path(&normalizer_path);
    load_abbreviation_tokens_from_paths(&normalizer_path, &abbreviations_path)
}

fn load_abbreviation_tokens_from_paths(
    normalizer_path: &PathBuf,
    abbreviations_path: &PathBuf,
) -> AbbreviationTokenSet {
    let mut out_nocase = HashSet::new();
    let mut out_case = HashSet::new();
    let mut out_regex = Vec::new();
    for default in ["mr.", "ms.", "mrs.", "mass.", "st."] {
        out_nocase.insert(default.to_string());
    }

    if let Ok(contents) = fs::read_to_string(normalizer_path)
        && let Ok(file) = toml::from_str::<NormalizerFile>(&contents)
    {
        let merged = file.normalization.abbreviations.merged();
        for key in merged.nocase.keys() {
            let normalized = normalize_abbreviation_token(key);
            if !normalized.is_empty() {
                out_nocase.insert(normalized);
            }
        }
        for key in merged.case.keys() {
            let normalized = normalize_abbreviation_token_case(key);
            if !normalized.is_empty() {
                out_case.insert(normalized);
            }
        }
        for rule in &merged.regex {
            if let Some(compiled) = compile_abbreviation_regex(rule) {
                out_regex.push(compiled);
            }
        }
    }

    if let Ok(contents) = fs::read_to_string(abbreviations_path)
        && let Ok(file) = toml::from_str::<AbbreviationsFile>(&contents)
    {
        let merged = file.abbreviations.merged();
        for key in merged.nocase.keys() {
            let normalized = normalize_abbreviation_token(key);
            if !normalized.is_empty() {
                out_nocase.insert(normalized);
            }
        }
        for key in merged.case.keys() {
            let normalized = normalize_abbreviation_token_case(key);
            if !normalized.is_empty() {
                out_case.insert(normalized);
            }
        }
        for rule in &merged.regex {
            if let Some(compiled) = compile_abbreviation_regex(rule) {
                out_regex.push(compiled);
            }
        }
    }

    AbbreviationTokenSet {
        case: out_case,
        nocase: out_nocase,
        regex: out_regex,
    }
}

fn compile_abbreviation_regex(rule: &AbbreviationRegexRule) -> Option<Regex> {
    let pattern = rule.pattern.trim();
    if pattern.is_empty() {
        return None;
    }
    let effective = if rule.case_sensitive {
        pattern.to_string()
    } else {
        format!("(?i:{pattern})")
    };
    Regex::new(&effective).ok()
}

fn resolve_normalizer_config_path() -> PathBuf {
    const NORMALIZER_CONFIG_ENV: &str = "LANTERNLEAF_NORMALIZER_CONFIG_PATH";
    const NORMALIZER_CONFIG_REL_PATH: &str = "conf/normalizer.toml";

    if let Some(value) = std::env::var_os(NORMALIZER_CONFIG_ENV) {
        let candidate = PathBuf::from(value);
        if candidate.exists() {
            return candidate;
        }
    }

    let relative = PathBuf::from(NORMALIZER_CONFIG_REL_PATH);
    if relative.exists() {
        return relative;
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest_dir.ancestors() {
        let candidate = ancestor.join(NORMALIZER_CONFIG_REL_PATH);
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(NORMALIZER_CONFIG_REL_PATH)
}

fn resolve_abbreviations_config_path(normalizer_config_path: &PathBuf) -> PathBuf {
    const ABBREVIATIONS_CONFIG_ENV: &str = "LANTERNLEAF_ABBREVIATIONS_CONFIG_PATH";
    const ABBREVIATIONS_CONFIG_REL_PATH: &str = "conf/abbreviations.toml";

    if let Some(value) = std::env::var_os(ABBREVIATIONS_CONFIG_ENV) {
        let candidate = PathBuf::from(value);
        if candidate.exists() {
            return candidate;
        }
    }

    let relative = PathBuf::from(ABBREVIATIONS_CONFIG_REL_PATH);
    if relative.exists() {
        return relative;
    }

    if let Some(parent) = normalizer_config_path.parent() {
        let sibling = parent.join("abbreviations.toml");
        if sibling.exists() {
            return sibling;
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest_dir.ancestors() {
        let candidate = ancestor.join(ABBREVIATIONS_CONFIG_REL_PATH);
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(ABBREVIATIONS_CONFIG_REL_PATH)
}

fn normalize_abbreviation_token(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{}.", trimmed.to_ascii_lowercase())
    }
}

fn normalize_abbreviation_token_case(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}.")
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
    abbreviations: AbbreviationConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct AbbreviationsFile {
    abbreviations: AbbreviationConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct AbbreviationConfig {
    case: BTreeMap<String, String>,
    nocase: BTreeMap<String, String>,
    regex: Vec<AbbreviationRegexRule>,
    #[serde(flatten)]
    legacy: BTreeMap<String, String>,
}

impl AbbreviationConfig {
    fn merged(&self) -> Self {
        let mut merged_nocase = self.nocase.clone();
        for (token, replacement) in &self.legacy {
            merged_nocase.insert(token.clone(), replacement.clone());
        }
        Self {
            case: self.case.clone(),
            nocase: merged_nocase,
            regex: self.regex.clone(),
            legacy: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
struct AbbreviationRegexRule {
    pattern: String,
    replace: String,
    #[serde(default)]
    case_sensitive: bool,
}

#[derive(Debug, Default)]
struct AbbreviationTokenSet {
    case: HashSet<String>,
    nocase: HashSet<String>,
    regex: Vec<Regex>,
}

#[cfg(test)]
mod tests {
    use super::{
        load_abbreviation_tokens_from_paths, AbbreviationTokenSet, load_abbreviation_tokens,
        split_sentences,
        split_sentences_with_abbreviations,
    };
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn keeps_middle_initial_in_name_together() {
        let text = "James B. Allen wrote this. Next sentence.";
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

    #[test]
    fn loads_external_abbreviation_tokens() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lanternleaf-abbrevs-{nonce}.toml"));
        std::fs::write(&path, "[abbreviations.nocase]\n\"pp.\" = \"pages\"\n")
            .expect("abbreviations config should be written");

        let tokens = load_abbreviation_tokens_from_paths(&PathBuf::new(), &path);

        assert!(tokens.nocase.contains("pp."));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn case_sensitive_abbreviation_tokens_require_exact_case() {
        let only_case = AbbreviationTokenSet {
            case: ["Dr.".to_string()].into_iter().collect(),
            nocase: std::collections::HashSet::new(),
            regex: Vec::new(),
        };
        let exact = split_sentences_with_abbreviations("Dr. Smith stayed.", &only_case);
        let lower = split_sentences_with_abbreviations("dr. Smith stayed.", &only_case);
        assert_eq!(exact.len(), 1);
        assert_eq!(lower.len(), 2);
    }

    #[test]
    fn does_not_split_decimal_numbers() {
        let text = "Pi is about 3.14159 and e is 2.71828. Next sentence.";
        let sentences = split_sentences(text);
        assert_eq!(sentences.len(), 2);
        assert!(sentences[0].contains("3.14159"));
        assert!(sentences[0].contains("2.71828"));
    }

    #[test]
    fn does_not_split_known_domain_tlds() {
        let text = "Visit example.com and sample.org now. Next sentence.";
        let sentences = split_sentences(text);
        assert_eq!(sentences.len(), 2);
        assert!(sentences[0].contains("example.com"));
        assert!(sentences[0].contains("sample.org"));
    }

    #[test]
    fn does_not_split_at_filename_extension_dot() {
        let text = "Open cat.txt and dog.html before reading book.epub or paper.pdf with notes.md. Next sentence.";
        let sentences = split_sentences(text);
        assert_eq!(sentences.len(), 2);
        assert!(sentences[0].contains("cat.txt"));
        assert!(sentences[0].contains("dog.html"));
        assert!(sentences[0].contains("book.epub"));
        assert!(sentences[0].contains("paper.pdf"));
        assert!(sentences[0].contains("notes.md"));
    }

    #[test]
    fn live_config_includes_chap_and_sect_tokens() {
        let tokens = load_abbreviation_tokens();
        assert!(
            tokens.nocase.contains("chap."),
            "expected chap. abbreviation token to be loaded from config"
        );
        assert!(
            tokens.nocase.contains("sect."),
            "expected sect. abbreviation token to be loaded from config"
        );
    }

    #[test]
    fn live_config_includes_calif_token() {
        let tokens = load_abbreviation_tokens();
        assert!(
            tokens.nocase.contains("calif."),
            "expected calif. abbreviation token to be loaded from config"
        );
    }

    #[test]
    fn live_config_does_not_split_calif() {
        let text = "He moved to Calif. in 1990. Next sentence.";
        let sentences = split_sentences(text);
        assert_eq!(
            sentences.len(),
            2,
            "expected Calif. to be treated as a non-terminal abbreviation"
        );
    }
}
