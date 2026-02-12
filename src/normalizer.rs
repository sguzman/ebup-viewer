use crate::cache::normalized_dir;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_NORMALIZER_PATH: &str = "conf/normalizer.toml";
const SENTENCE_MARKER: &str = "\n<<__EBUP_SENTENCE_BOUNDARY__>>\n";

static RE_INLINE_CODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"`([^`]+)`").unwrap());
static RE_MARKDOWN_LINK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap());
static RE_NUMERIC_BRACKET_CITE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[\s*\d+(?:\s*,\s*\d+)*\s*\]").unwrap());
static RE_PARENTHETICAL_NUMERIC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\(\s*\d+(?:\s*,\s*\d+)*\s*\)").unwrap());
static RE_SUPERSCRIPT_CITE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[⁰¹²³⁴⁵⁶⁷⁸⁹]+").unwrap());
static RE_WORD_SUFFIX_FOOTNOTE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?P<prefix>\p{L})\d{1,3}\b").unwrap());
static RE_SQUARE_BRACKET_BLOCK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[[^\]]*\]").unwrap());
static RE_CURLY_BRACKET_BLOCK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\{[^}]*\}").unwrap());
static RE_HORIZONTAL_WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t\u{00A0}]+").unwrap());
static RE_SPACE_BEFORE_PUNCT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+([,.;:!?])").unwrap());

#[derive(Debug, Clone)]
pub struct TextNormalizer {
    config: NormalizerConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
struct NormalizerFile {
    normalization: NormalizerConfig,
}

impl Default for NormalizerFile {
    fn default() -> Self {
        Self {
            normalization: NormalizerConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
struct NormalizerConfig {
    enabled: bool,
    mode: NormalizationMode,
    collapse_whitespace: bool,
    remove_space_before_punctuation: bool,
    strip_inline_code: bool,
    strip_markdown_links: bool,
    drop_numeric_bracket_citations: bool,
    drop_parenthetical_numeric_citations: bool,
    drop_superscript_citations: bool,
    drop_word_suffix_numeric_footnotes: bool,
    drop_square_bracket_text: bool,
    drop_curly_brace_text: bool,
    min_sentence_chars: usize,
    require_alphanumeric: bool,
    replacements: BTreeMap<String, String>,
    abbreviations: BTreeMap<String, String>,
    drop_tokens: Vec<String>,
    acronyms: AcronymConfig,
    pronunciation: PronunciationConfig,
}

impl Default for NormalizerConfig {
    fn default() -> Self {
        let mut replacements = BTreeMap::new();
        replacements.insert("#".to_string(), " ".to_string());

        Self {
            enabled: true,
            mode: NormalizationMode::Sentence,
            collapse_whitespace: true,
            remove_space_before_punctuation: true,
            strip_inline_code: true,
            strip_markdown_links: true,
            drop_numeric_bracket_citations: true,
            drop_parenthetical_numeric_citations: true,
            drop_superscript_citations: true,
            drop_word_suffix_numeric_footnotes: true,
            drop_square_bracket_text: true,
            drop_curly_brace_text: true,
            min_sentence_chars: 2,
            require_alphanumeric: true,
            replacements,
            abbreviations: default_abbreviations(),
            drop_tokens: Vec::new(),
            acronyms: AcronymConfig::default(),
            pronunciation: PronunciationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
enum NormalizationMode {
    #[default]
    Page,
    Sentence,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
struct AcronymConfig {
    enabled: bool,
    tokens: Vec<String>,
    letter_separator: String,
    digit_separator: String,
    letter_sounds: BTreeMap<String, String>,
}

impl Default for AcronymConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tokens: vec![
                "CSS".to_string(),
                "HTML".to_string(),
                "HTTP".to_string(),
                "HTTPS".to_string(),
                "URL".to_string(),
                "API".to_string(),
                "CPU".to_string(),
                "GPU".to_string(),
                "JSON".to_string(),
                "SQL".to_string(),
                "XML".to_string(),
                "TTS".to_string(),
                "XTTS".to_string(),
                "LLM".to_string(),
            ],
            letter_separator: " ".to_string(),
            digit_separator: " point ".to_string(),
            letter_sounds: default_letter_sounds(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
struct PronunciationConfig {
    year_mode: YearMode,
    number_separator: String,
    insert_and: bool,
    enable_brand_map: bool,
    brand_map: BTreeMap<String, String>,
    custom_pronunciations: BTreeMap<String, String>,
}

impl Default for PronunciationConfig {
    fn default() -> Self {
        let mut brand_map = BTreeMap::new();
        brand_map.insert("MySQL".to_string(), "My S Q L".to_string());
        brand_map.insert("Mysql".to_string(), "My S Q L".to_string());
        brand_map.insert("SQLite".to_string(), "S Q Lite".to_string());
        brand_map.insert("SQLITE".to_string(), "S Q Lite".to_string());
        brand_map.insert("PostCSS".to_string(), "Post C S S".to_string());

        Self {
            year_mode: YearMode::American,
            number_separator: " ".to_string(),
            insert_and: false,
            enable_brand_map: true,
            brand_map,
            custom_pronunciations: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
enum YearMode {
    #[default]
    American,
    None,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PageNormalization {
    pub audio_sentences: Vec<String>,
    pub display_to_audio: Vec<Option<usize>>,
    pub audio_to_display: Vec<usize>,
}

impl TextNormalizer {
    pub fn load_default() -> Self {
        Self::load(Path::new(DEFAULT_NORMALIZER_PATH))
    }

    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(contents) => match toml::from_str::<NormalizerFile>(&contents) {
                Ok(file) => {
                    tracing::info!(path = %path.display(), "Loaded text normalizer config");
                    Self {
                        config: file.normalization,
                    }
                }
                Err(err) => {
                    tracing::warn!(path = %path.display(), "Invalid normalizer config TOML: {err}");
                    Self::default()
                }
            },
            Err(err) => {
                tracing::warn!(path = %path.display(), "Falling back to default normalizer config: {err}");
                Self::default()
            }
        }
    }

    pub fn plan_page_cached(
        &self,
        epub_path: &Path,
        page_idx: usize,
        display_sentences: &[String],
    ) -> PageNormalization {
        if self.config.mode == NormalizationMode::Sentence {
            return self.plan_page_cached_sentence_mode(epub_path, display_sentences);
        }
        self.plan_page_cached_page_mode(epub_path, page_idx, display_sentences)
    }

    pub fn first_speakable_sentence_cached(
        &self,
        epub_path: &Path,
        display_sentences: &[String],
        requested_display_idx: usize,
    ) -> Option<(usize, usize, String)> {
        if display_sentences.is_empty() {
            return None;
        }
        let config_hash = self.config_hash();
        let clamped = requested_display_idx.min(display_sentences.len().saturating_sub(1));

        let mut prefix_audio = vec![0usize; display_sentences.len() + 1];
        for (i, sentence) in display_sentences.iter().enumerate() {
            let speakable = self
                .normalize_sentence_cached(epub_path, &config_hash, sentence)
                .is_some() as usize;
            prefix_audio[i + 1] = prefix_audio[i] + speakable;
        }

        for (idx, sentence) in display_sentences.iter().enumerate().skip(clamped) {
            if let Some(cleaned) = self.normalize_sentence_cached(epub_path, &config_hash, sentence)
            {
                let audio_idx = prefix_audio[idx];
                return Some((idx, audio_idx, cleaned));
            }
        }
        for (idx, sentence) in display_sentences
            .iter()
            .enumerate()
            .take(clamped + 1)
            .rev()
        {
            if let Some(cleaned) = self.normalize_sentence_cached(epub_path, &config_hash, sentence)
            {
                let audio_idx = prefix_audio[idx];
                return Some((idx, audio_idx, cleaned));
            }
        }

        None
    }

    fn plan_page_cached_page_mode(
        &self,
        epub_path: &Path,
        page_idx: usize,
        display_sentences: &[String],
    ) -> PageNormalization {
        let source_hash = hash_sentences(display_sentences);
        let config_hash = self.config_hash();
        let cache_path =
            self.normalized_cache_path(epub_path, page_idx, &source_hash, &config_hash);

        if let Ok(contents) = fs::read_to_string(&cache_path) {
            if let Ok(cached) = toml::from_str::<PageNormalization>(&contents) {
                tracing::debug!(
                    path = %cache_path.display(),
                    page = page_idx + 1,
                    "Loaded normalized page cache"
                );
                return cached;
            }
        }

        let plan = self.plan_page(display_sentences);
        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match toml::to_string(&plan) {
            Ok(serialized) => {
                if let Err(err) = fs::write(&cache_path, serialized) {
                    tracing::warn!(
                        path = %cache_path.display(),
                        "Failed to write normalized page cache: {err}"
                    );
                } else {
                    tracing::debug!(
                        path = %cache_path.display(),
                        page = page_idx + 1,
                        "Stored normalized page cache"
                    );
                }
            }
            Err(err) => {
                tracing::warn!("Failed to serialize normalized page cache: {err}");
            }
        }
        plan
    }

    fn plan_page_cached_sentence_mode(
        &self,
        epub_path: &Path,
        display_sentences: &[String],
    ) -> PageNormalization {
        if display_sentences.is_empty() {
            return PageNormalization {
                audio_sentences: Vec::new(),
                display_to_audio: Vec::new(),
                audio_to_display: Vec::new(),
            };
        }

        let config_hash = self.config_hash();
        let mut audio_sentences = Vec::with_capacity(display_sentences.len());
        let mut display_to_audio = vec![None; display_sentences.len()];
        let mut audio_to_display = Vec::new();

        for (display_idx, sentence) in display_sentences.iter().enumerate() {
            if let Some(cleaned) = self.normalize_sentence_cached(epub_path, &config_hash, sentence)
            {
                let audio_idx = audio_sentences.len();
                audio_sentences.push(cleaned);
                display_to_audio[display_idx] = Some(audio_idx);
                audio_to_display.push(display_idx);
            }
        }

        PageNormalization {
            audio_sentences,
            display_to_audio,
            audio_to_display,
        }
    }

    pub fn plan_page(&self, display_sentences: &[String]) -> PageNormalization {
        if display_sentences.is_empty() {
            return PageNormalization {
                audio_sentences: Vec::new(),
                display_to_audio: Vec::new(),
                audio_to_display: Vec::new(),
            };
        }

        if !self.config.enabled {
            let audio_sentences = display_sentences.to_vec();
            let display_to_audio = (0..display_sentences.len()).map(Some).collect();
            let audio_to_display = (0..display_sentences.len()).collect();
            return PageNormalization {
                audio_sentences,
                display_to_audio,
                audio_to_display,
            };
        }

        let cleaned_sentences = match self.config.mode {
            NormalizationMode::Page => self.normalize_page_mode(display_sentences),
            NormalizationMode::Sentence => display_sentences
                .iter()
                .map(|sentence| self.clean_text_core(sentence))
                .collect(),
        };

        let mut audio_sentences = Vec::with_capacity(cleaned_sentences.len());
        let mut display_to_audio = vec![None; cleaned_sentences.len()];
        let mut audio_to_display = Vec::new();

        for (display_idx, sentence) in cleaned_sentences.into_iter().enumerate() {
            if let Some(cleaned) = self.finalize_sentence(&sentence) {
                let audio_idx = audio_sentences.len();
                audio_sentences.push(cleaned);
                display_to_audio[display_idx] = Some(audio_idx);
                audio_to_display.push(display_idx);
            }
        }

        PageNormalization {
            audio_sentences,
            display_to_audio,
            audio_to_display,
        }
    }

    fn normalize_page_mode(&self, display_sentences: &[String]) -> Vec<String> {
        let joined = display_sentences.join(SENTENCE_MARKER);
        let cleaned = self.clean_text_core(&joined);
        let split: Vec<String> = cleaned
            .split(SENTENCE_MARKER)
            .map(|part| part.to_string())
            .collect();

        if split.len() == display_sentences.len() {
            split
        } else {
            tracing::debug!(
                expected = display_sentences.len(),
                actual = split.len(),
                "Normalizer marker split mismatch; falling back to sentence mode"
            );
            display_sentences
                .iter()
                .map(|sentence| self.clean_text_core(sentence))
                .collect()
        }
    }

    fn clean_text_core(&self, input: &str) -> String {
        let mut text = input.to_string();

        if self.config.strip_markdown_links {
            text = RE_MARKDOWN_LINK.replace_all(&text, "$1").to_string();
        }

        if self.config.strip_inline_code {
            text = RE_INLINE_CODE.replace_all(&text, "$1").to_string();
        }

        if self.config.drop_numeric_bracket_citations {
            text = RE_NUMERIC_BRACKET_CITE.replace_all(&text, " ").to_string();
        }

        if self.config.drop_parenthetical_numeric_citations {
            text = RE_PARENTHETICAL_NUMERIC.replace_all(&text, " ").to_string();
        }

        if self.config.drop_superscript_citations {
            text = RE_SUPERSCRIPT_CITE.replace_all(&text, " ").to_string();
        }

        if self.config.drop_word_suffix_numeric_footnotes {
            text = RE_WORD_SUFFIX_FOOTNOTE
                .replace_all(&text, "$prefix")
                .to_string();
        }

        if self.config.drop_square_bracket_text {
            text = RE_SQUARE_BRACKET_BLOCK.replace_all(&text, " ").to_string();
        }

        if self.config.drop_curly_brace_text {
            text = RE_CURLY_BRACKET_BLOCK.replace_all(&text, " ").to_string();
        }

        if !self.config.abbreviations.is_empty() {
            text = apply_abbreviation_map(&text, &self.config.abbreviations);
        }

        if !self.config.replacements.is_empty() {
            let mut entries: Vec<_> = self.config.replacements.iter().collect();
            entries.sort_by_key(|(from, _)| Reverse(from.len()));
            for (from, to) in entries {
                text = text.replace(from.as_str(), to.as_str());
            }
        }

        if !self.config.drop_tokens.is_empty() {
            for token in &self.config.drop_tokens {
                if !token.is_empty() {
                    text = text.replace(token, " ");
                }
            }
        }

        if self.config.pronunciation.enable_brand_map
            && !self.config.pronunciation.brand_map.is_empty()
        {
            text = apply_brand_map(&text, &self.config.pronunciation.brand_map);
        }

        if !self.config.pronunciation.custom_pronunciations.is_empty() {
            text = apply_brand_map(&text, &self.config.pronunciation.custom_pronunciations);
        }

        if self.config.pronunciation.year_mode != YearMode::None {
            text = apply_year_pronunciation(&text, &self.config.pronunciation);
        }

        if self.config.acronyms.enabled && !self.config.acronyms.tokens.is_empty() {
            text = apply_acronym_expansion(&text, &self.config.acronyms);
        }

        if self.config.collapse_whitespace {
            text = RE_HORIZONTAL_WS.replace_all(&text, " ").to_string();
        }

        if self.config.remove_space_before_punctuation {
            text = RE_SPACE_BEFORE_PUNCT.replace_all(&text, "$1").to_string();
        }

        text.trim().to_string()
    }

    fn finalize_sentence(&self, sentence: &str) -> Option<String> {
        let trimmed = sentence.trim();
        if trimmed.is_empty() {
            return None;
        }

        if self.config.require_alphanumeric && !trimmed.chars().any(|ch| ch.is_alphanumeric()) {
            return None;
        }

        if trimmed.chars().count() < self.config.min_sentence_chars.max(1) {
            return None;
        }

        Some(trimmed.to_string())
    }

    fn config_hash(&self) -> String {
        let serialized = toml::to_string(&self.config).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(serialized.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn normalized_cache_path(
        &self,
        epub_path: &Path,
        page_idx: usize,
        source_hash: &str,
        config_hash: &str,
    ) -> PathBuf {
        let file_name = format!("p{}-{}-{}.toml", page_idx, source_hash, config_hash);
        normalized_dir(epub_path).join(file_name)
    }

    fn normalized_sentence_cache_path(
        &self,
        epub_path: &Path,
        sentence_hash: &str,
        config_hash: &str,
    ) -> PathBuf {
        let file_name = format!("s-{}-{}.toml", sentence_hash, config_hash);
        normalized_dir(epub_path).join(file_name)
    }

    fn normalize_sentence_cached(
        &self,
        epub_path: &Path,
        config_hash: &str,
        sentence: &str,
    ) -> Option<String> {
        let source_hash = hash_sentence(sentence);
        let cache_path = self.normalized_sentence_cache_path(epub_path, &source_hash, config_hash);

        if let Ok(contents) = fs::read_to_string(&cache_path) {
            if let Ok(cached) = toml::from_str::<NormalizedSentenceCache>(&contents) {
                return cached.normalized;
            }
        }

        let cleaned = self.clean_text_core(sentence);
        let normalized = self.finalize_sentence(&cleaned);
        let cached = NormalizedSentenceCache {
            normalized: normalized.clone(),
        };

        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match toml::to_string(&cached) {
            Ok(serialized) => {
                if let Err(err) = fs::write(&cache_path, serialized) {
                    tracing::warn!(
                        path = %cache_path.display(),
                        "Failed to write normalized sentence cache: {err}"
                    );
                }
            }
            Err(err) => {
                tracing::warn!("Failed to serialize normalized sentence cache: {err}");
            }
        }

        normalized
    }
}

impl Default for TextNormalizer {
    fn default() -> Self {
        Self {
            config: NormalizerConfig::default(),
        }
    }
}

fn default_letter_sounds() -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (letter, sound) in [
        ('A', "ay"),
        ('B', "bee"),
        ('C', "see"),
        ('D', "dee"),
        ('E', "ee"),
        ('F', "eff"),
        ('G', "jee"),
        ('H', "aitch"),
        ('I', "eye"),
        ('J', "jay"),
        ('K', "kay"),
        ('L', "el"),
        ('M', "em"),
        ('N', "en"),
        ('O', "oh"),
        ('P', "pee"),
        ('Q', "cue"),
        ('R', "ar"),
        ('S', "ess"),
        ('T', "tee"),
        ('U', "you"),
        ('V', "vee"),
        ('W', "double you"),
        ('X', "ex"),
        ('Y', "why"),
        ('Z', "zee"),
    ] {
        map.insert(letter.to_string(), sound.to_string());
    }
    for (digit, word) in [
        ('0', "zero"),
        ('1', "one"),
        ('2', "two"),
        ('3', "three"),
        ('4', "four"),
        ('5', "five"),
        ('6', "six"),
        ('7', "seven"),
        ('8', "eight"),
        ('9', "nine"),
    ] {
        map.insert(digit.to_string(), word.to_string());
    }
    map
}

fn apply_brand_map(text: &str, brand_map: &BTreeMap<String, String>) -> String {
    let mut out = text.to_string();
    let mut entries: Vec<_> = brand_map.iter().collect();
    entries.sort_by_key(|(token, _)| Reverse(token.len()));

    for (token, replacement) in entries {
        let pattern = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(token))).unwrap();
        out = pattern.replace_all(&out, replacement.as_str()).to_string();
    }

    out
}

fn default_abbreviations() -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    map.insert("Mr.".to_string(), "Mister".to_string());
    map.insert("Ms.".to_string(), "Miss".to_string());
    map.insert("Mrs.".to_string(), "Misses".to_string());
    map.insert("Mass.".to_string(), "Massachusetts".to_string());
    map.insert("St.".to_string(), "Saint".to_string());
    map
}

fn apply_abbreviation_map(text: &str, abbreviation_map: &BTreeMap<String, String>) -> String {
    let mut out = text.to_string();
    let mut entries: Vec<_> = abbreviation_map.iter().collect();
    entries.sort_by_key(|(token, _)| Reverse(token.len()));

    for (token, replacement) in entries {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }

        let pattern = if let Some(base) = trimmed.strip_suffix('.') {
            format!(r"(?i)\b{}\.", regex::escape(base))
        } else {
            format!(r"(?i)\b{}\b", regex::escape(trimmed))
        };

        if let Ok(re) = Regex::new(&pattern) {
            out = re.replace_all(&out, replacement.as_str()).to_string();
        }
    }

    out
}

fn apply_year_pronunciation(text: &str, cfg: &PronunciationConfig) -> String {
    let re = Regex::new(r"\b(1\d{3}|20\d{2})\b").unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        let year = caps[1].parse::<usize>().unwrap_or(0);
        year_to_words(year, cfg)
    })
    .to_string()
}

fn year_to_words(year: usize, cfg: &PronunciationConfig) -> String {
    if year < 1000 || year > 2099 {
        return year.to_string();
    }

    let ones = [
        "", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    let teens = [
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    let tens = [
        "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];

    let thousands = year / 1000;
    let hundreds = (year / 100) % 10;
    let remainder = year % 100;

    let mut parts = Vec::new();
    if thousands > 0 {
        parts.push(format!("{} thousand", ones[thousands]));
    }
    if hundreds > 0 {
        parts.push(format!("{} hundred", ones[hundreds]));
    }

    if remainder > 0 {
        let remainder_str = if remainder < 10 {
            ones[remainder].to_string()
        } else if remainder < 20 {
            teens[remainder - 10].to_string()
        } else {
            let mut s = tens[remainder / 10].to_string();
            if remainder % 10 > 0 {
                s.push(' ');
                s.push_str(ones[remainder % 10]);
            }
            s
        };

        if hundreds > 0 && cfg.insert_and {
            parts.push(format!("and {remainder_str}"));
        } else {
            parts.push(remainder_str);
        }
    }

    parts.join(&cfg.number_separator)
}

fn apply_acronym_expansion(text: &str, cfg: &AcronymConfig) -> String {
    let mut out = text.to_string();

    for token in &cfg.tokens {
        let pattern = format!(
            r"(?i)\b{}(?P<digits>\d+(?:\.\d+)*)?\b",
            regex::escape(token)
        );
        let re = Regex::new(&pattern).unwrap();
        out = re
            .replace_all(&out, |caps: &regex::Captures| {
                let letters = caps[0]
                    .chars()
                    .filter(|ch| ch.is_ascii_alphabetic())
                    .map(|ch| {
                        let key = ch.to_ascii_uppercase().to_string();
                        cfg.letter_sounds.get(&key).cloned().unwrap_or_else(|| key)
                    })
                    .collect::<Vec<_>>();

                let mut spelled = letters.join(&cfg.letter_separator);
                if let Some(digits) = caps.name("digits") {
                    let digits = digits.as_str().trim();
                    if !digits.is_empty() {
                        let spoken_digits = digits
                            .split('.')
                            .map(|group| {
                                group
                                    .chars()
                                    .filter(|ch| ch.is_ascii_digit())
                                    .map(|ch| {
                                        cfg.letter_sounds
                                            .get(&ch.to_string())
                                            .cloned()
                                            .unwrap_or_else(|| ch.to_string())
                                    })
                                    .collect::<Vec<_>>()
                                    .join(&cfg.letter_separator)
                            })
                            .filter(|group| !group.is_empty())
                            .collect::<Vec<_>>()
                            .join(&cfg.digit_separator);

                        if !spoken_digits.is_empty() {
                            if !spelled.is_empty() {
                                spelled.push(' ');
                            }
                            spelled.push_str(&spoken_digits);
                        }
                    }
                }

                if spelled.is_empty() {
                    caps[0].to_string()
                } else {
                    spelled
                }
            })
            .to_string();
    }

    out
}

fn hash_sentences(sentences: &[String]) -> String {
    let mut hasher = Sha256::new();
    for sentence in sentences {
        hasher.update(sentence.as_bytes());
        hasher.update([0u8]);
    }
    format!("{:x}", hasher.finalize())
}

fn hash_sentence(sentence: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(sentence.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct NormalizedSentenceCache {
    normalized: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn strips_superscript_and_expands_abbreviation() {
        let normalizer = TextNormalizer::default();
        let page = vec!["Mr. Hale wrote this².".to_string()];
        let plan = normalizer.plan_page(&page);
        assert_eq!(plan.audio_sentences.len(), 1);
        assert_eq!(plan.audio_sentences[0], "Mister Hale wrote this.");
    }

    #[test]
    fn sentence_mode_cache_reused_across_page_indices() {
        let normalizer = TextNormalizer::default();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let epub_path = std::env::temp_dir().join(format!("ebup-normalizer-{nonce}.epub"));
        let cache_root = normalized_dir(&epub_path);
        let _ = fs::remove_dir_all(&cache_root);

        let page_a = vec!["Alpha sentence.".to_string(), "Beta sentence.".to_string()];
        let page_b = vec!["Beta sentence.".to_string(), "Gamma sentence.".to_string()];

        let _ = normalizer.plan_page_cached(&epub_path, 0, &page_a);
        let files_after_first: Vec<String> = fs::read_dir(&cache_root)
            .expect("cache dir should exist")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        let first_sentence_files = files_after_first
            .iter()
            .filter(|name| name.starts_with("s-"))
            .count();
        assert_eq!(first_sentence_files, 2);

        let _ = normalizer.plan_page_cached(&epub_path, 99, &page_b);
        let files_after_second: Vec<String> = fs::read_dir(&cache_root)
            .expect("cache dir should exist")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        let second_sentence_files = files_after_second
            .iter()
            .filter(|name| name.starts_with("s-"))
            .count();
        assert_eq!(second_sentence_files, 3);
        assert!(
            !files_after_second.iter().any(|name| name.starts_with("p")),
            "sentence mode should not create page-level normalization cache files"
        );

        let _ = fs::remove_dir_all(&cache_root);
    }
}
