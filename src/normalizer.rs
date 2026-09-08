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
const NORMALIZER_CONFIG_ENV: &str = "LANTERNLEAF_NORMALIZER_CONFIG_PATH";
const DEFAULT_ABBREVIATIONS_PATH: &str = "conf/abbreviations.toml";
const ABBREVIATIONS_CONFIG_ENV: &str = "LANTERNLEAF_ABBREVIATIONS_CONFIG_PATH";
const SENTENCE_MARKER: &str = "\n<<__EBUP_SENTENCE_BOUNDARY__>>\n";
const NORMALIZER_PIPELINE_REV: &str = "normalizer-v2-fancy-brackets";

static RE_INLINE_CODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"`([^`]+)`").unwrap());
static RE_MARKDOWN_LINK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap());
static RE_NUMERIC_BRACKET_CITE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[\s*\d+(?:\s*,\s*\d+)*\s*\]").unwrap());
static RE_PARENTHETICAL_NUMERIC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\(\s*\d+(?:\s*,\s*\d+)*\s*\)").unwrap());
static RE_SUPERSCRIPT_CITE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[⁰¹²³⁴⁵⁶⁷⁸⁹]+").unwrap());
static RE_WORD_SUFFIX_FOOTNOTE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?P<prefix>\p{L})\d{1,3}\b").unwrap());
static RE_SQUARE_BRACKET_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[[^\]]*\]|【[^】]*】|［[^］]*］").unwrap());
static RE_CURLY_BRACKET_BLOCK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\{[^}]*\}").unwrap());
static RE_HORIZONTAL_WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t\u{00A0}]+").unwrap());
static RE_SPACE_BEFORE_PUNCT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+([,.;:!?])").unwrap());
static RE_SOFT_BREAK_WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());

#[derive(Debug, Clone)]
pub struct TextNormalizer {
    config: NormalizerConfig,
    compiled: CompiledNormalizer,
}

#[derive(Debug, Clone)]
struct CompiledNormalizer {
    abbreviation_regex: Vec<CompiledReplacement>,
    abbreviation_case: Vec<CompiledReplacement>,
    abbreviation_nocase: Vec<CompiledReplacement>,
    brand_map: Vec<CompiledReplacement>,
    custom_pronunciations: Vec<CompiledReplacement>,
    acronym_tokens: Vec<(Regex, AcronymConfig)>,
    year: Regex,
}

#[derive(Debug, Clone)]
struct CompiledReplacement {
    regex: Regex,
    replacement: String,
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
    chunk_long_sentences: bool,
    max_audio_chars_per_chunk: usize,
    max_audio_words_per_chunk: usize,
    min_sentence_chars: usize,
    require_alphanumeric: bool,
    replacements: BTreeMap<String, String>,
    abbreviations: AbbreviationConfig,
    drop_tokens: Vec<String>,
    acronyms: AcronymConfig,
    pronunciation: PronunciationConfig,
}

impl Default for NormalizerConfig {
    fn default() -> Self {
        let mut replacements = BTreeMap::new();
        replacements.insert("#".to_string(), " ".to_string());
        replacements.insert("%".to_string(), " percent ".to_string());

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
            chunk_long_sentences: true,
            max_audio_chars_per_chunk: 180,
            max_audio_words_per_chunk: 32,
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

impl CompiledNormalizer {
    fn compile(config: &NormalizerConfig) -> Self {
        let abbreviations = config.abbreviations.merged();
        let mut abbreviation_regex = Vec::new();
        for rule in &abbreviations.regex {
            if rule.pattern.trim().is_empty() {
                continue;
            }
            let pattern = if rule.case_sensitive {
                rule.pattern.clone()
            } else {
                format!("(?i){}", rule.pattern)
            };
            match Regex::new(&pattern) {
                Ok(regex) => abbreviation_regex.push(CompiledReplacement {
                    regex,
                    replacement: rule.replace.clone(),
                }),
                Err(err) => {
                    tracing::warn!(pattern = %rule.pattern, "Skipping invalid abbreviation regex rule: {err}")
                }
            }
        }

        let mut abbreviation_case = Vec::new();
        for (token, replacement) in abbreviations.case.iter() {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                continue;
            }
            let pattern = if let Some(base) = trimmed.strip_suffix('.') {
                format!(r"\b{}\.", regex::escape(base))
            } else {
                format!(r"\b{}\b", regex::escape(trimmed))
            };
            if let Ok(regex) = Regex::new(&pattern) {
                abbreviation_case.push(CompiledReplacement {
                    regex,
                    replacement: replacement.clone(),
                });
            }
        }
        abbreviation_case.sort_by_key(|entry| std::cmp::Reverse(entry.regex.as_str().len()));

        let mut abbreviation_nocase = Vec::new();
        for (token, replacement) in build_nocase_abbreviation_entries(&abbreviations.nocase) {
            let pattern = if let Some(base) = token.strip_suffix('.') {
                format!(r"(?i)\b{}\.", regex::escape(base))
            } else {
                format!(r"(?i)\b{}\b", regex::escape(&token))
            };
            if let Ok(regex) = Regex::new(&pattern) {
                abbreviation_nocase.push(CompiledReplacement { regex, replacement });
            }
        }

        fn compile_map(map: &BTreeMap<String, String>) -> Vec<CompiledReplacement> {
            let mut entries = map
                .iter()
                .filter_map(|(token, replacement)| {
                    Regex::new(&format!(r"(?i)\b{}\b", regex::escape(token)))
                        .ok()
                        .map(|regex| CompiledReplacement {
                            regex,
                            replacement: replacement.clone(),
                        })
                })
                .collect::<Vec<_>>();
            entries.sort_by_key(|entry| std::cmp::Reverse(entry.regex.as_str().len()));
            entries
        }

        let acronym_tokens = config
            .acronyms
            .tokens
            .iter()
            .filter_map(|token| {
                Regex::new(&format!(
                    r"(?i)\b{}(?P<digits>\d+(?:\.\d+)*)?\b",
                    regex::escape(token)
                ))
                .ok()
                .map(|regex| (regex, config.acronyms.clone()))
            })
            .collect();

        Self {
            abbreviation_regex,
            abbreviation_case,
            abbreviation_nocase,
            brand_map: compile_map(&config.pronunciation.brand_map),
            custom_pronunciations: compile_map(&config.pronunciation.custom_pronunciations),
            acronym_tokens,
            year: Regex::new(r"\b(1\d{3}|20\d{2})\b").expect("valid year regex"),
        }
    }
}

impl TextNormalizer {
    fn from_config(config: NormalizerConfig) -> Self {
        let compiled = CompiledNormalizer::compile(&config);
        Self { config, compiled }
    }

    pub fn load_default() -> Self {
        let path = resolve_default_normalizer_path();
        Self::load(path.as_path())
    }

    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(contents) => match toml::from_str::<NormalizerFile>(&contents) {
                Ok(file) => {
                    let mut config = file.normalization;
                    config
                        .abbreviations
                        .extend(load_external_abbreviations(path));
                    tracing::info!(path = %path.display(), "Loaded text normalizer config");
                    Self::from_config(config)
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
            return self.plan_page_cached_sentence_mode(epub_path, page_idx, display_sentences);
        }
        self.plan_page_cached_page_mode(epub_path, page_idx, display_sentences)
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

        if let Some(cached) = self.read_page_plan_cache(&cache_path, page_idx) {
            return cached;
        }

        let plan = self.plan_page(display_sentences);
        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        self.write_page_plan_cache(&cache_path, page_idx, &plan);
        plan
    }

    fn plan_page_cached_sentence_mode(
        &self,
        epub_path: &Path,
        page_idx: usize,
        display_sentences: &[String],
    ) -> PageNormalization {
        if display_sentences.is_empty() {
            return PageNormalization {
                audio_sentences: Vec::new(),
                display_to_audio: Vec::new(),
                audio_to_display: Vec::new(),
            };
        }

        let source_hash = hash_sentences(display_sentences);
        let config_hash = self.config_hash();
        let page_cache_path =
            self.normalized_cache_path(epub_path, page_idx, &source_hash, &config_hash);
        if let Some(cached) = self.read_page_plan_cache(&page_cache_path, page_idx) {
            return cached;
        }

        let mut audio_sentences = Vec::with_capacity(display_sentences.len());
        let mut display_to_audio = vec![None; display_sentences.len()];
        let mut audio_to_display = Vec::new();

        for (display_idx, sentence) in display_sentences.iter().enumerate() {
            if let Some(chunks) =
                self.normalize_sentence_chunks_cached(epub_path, &config_hash, sentence)
            {
                let first_audio_idx = audio_sentences.len();
                display_to_audio[display_idx] = Some(first_audio_idx);
                for chunk in chunks {
                    audio_to_display.push(display_idx);
                    audio_sentences.push(chunk);
                }
            }
        }

        let plan = PageNormalization {
            audio_sentences,
            display_to_audio,
            audio_to_display,
        };

        if let Some(parent) = page_cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        self.write_page_plan_cache(&page_cache_path, page_idx, &plan);

        plan
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
                let chunks = self.chunk_sentence_for_tts(&cleaned);
                if chunks.is_empty() {
                    continue;
                }
                let first_audio_idx = audio_sentences.len();
                display_to_audio[display_idx] = Some(first_audio_idx);
                for chunk in chunks {
                    audio_to_display.push(display_idx);
                    audio_sentences.push(chunk);
                }
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

    fn apply_compiled_abbreviation_map(&self, text: &str) -> String {
        let mut out = text.to_string();
        for rule in self
            .compiled
            .abbreviation_regex
            .iter()
            .chain(self.compiled.abbreviation_case.iter())
            .chain(self.compiled.abbreviation_nocase.iter())
        {
            out = rule
                .regex
                .replace_all(&out, rule.replacement.as_str())
                .to_string();
        }
        out
    }

    fn apply_compiled_brand_map(&self, text: &str, rules: &[CompiledReplacement]) -> String {
        let mut out = text.to_string();
        for rule in rules {
            out = rule
                .regex
                .replace_all(&out, rule.replacement.as_str())
                .to_string();
        }
        out
    }

    fn apply_compiled_year_pronunciation(&self, text: &str) -> String {
        self.compiled
            .year
            .replace_all(text, |caps: &regex::Captures| {
                let year = caps[1].parse::<usize>().unwrap_or(0);
                year_to_words(year, &self.config.pronunciation)
            })
            .to_string()
    }

    fn apply_compiled_acronym_expansion(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (regex, cfg) in &self.compiled.acronym_tokens {
            out = regex
                .replace_all(&out, |caps: &regex::Captures| {
                    expand_acronym_match(caps, cfg)
                })
                .to_string();
        }
        out
    }

    fn clean_text_core(&self, input: &str) -> String {
        let mut text = normalize_unicode_punctuation(input);
        text = text.replace('"', "");

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
            text = self.apply_compiled_abbreviation_map(&text);
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
            text = self.apply_compiled_brand_map(&text, &self.compiled.brand_map);
        }

        if !self.config.pronunciation.custom_pronunciations.is_empty() {
            text = self.apply_compiled_brand_map(&text, &self.compiled.custom_pronunciations);
        }

        if self.config.pronunciation.year_mode != YearMode::None {
            text = self.apply_compiled_year_pronunciation(&text);
        }

        if self.config.acronyms.enabled && !self.config.acronyms.tokens.is_empty() {
            text = self.apply_compiled_acronym_expansion(&text);
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
        let trimmed = trim_boundary_noise(sentence);
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

    fn chunk_sentence_for_tts(&self, sentence: &str) -> Vec<String> {
        let cleaned = trim_boundary_noise(sentence);
        if cleaned.is_empty() {
            return Vec::new();
        }
        if !self.config.chunk_long_sentences {
            return vec![cleaned.to_string()];
        }

        let max_chars = self.config.max_audio_chars_per_chunk.max(40);
        let max_words = self.config.max_audio_words_per_chunk.max(8);
        if !exceeds_chunk_limits(cleaned, max_chars, max_words) {
            return vec![cleaned.to_string()];
        }

        let mut chunks = Vec::new();
        let mut current = String::new();
        for segment in split_for_chunking(cleaned) {
            self.push_segment_into_chunks(
                &mut chunks,
                &mut current,
                &segment,
                max_chars,
                max_words,
            );
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        if chunks.is_empty() {
            chunks.push(cleaned.to_string());
        }

        tracing::debug!(
            original_chars = cleaned.chars().count(),
            original_words = cleaned.split_whitespace().count(),
            chunk_count = chunks.len(),
            max_chars,
            max_words,
            "Split oversized normalized sentence into multiple TTS chunks"
        );

        for chunk in &chunks {
            if exceeds_chunk_limits(chunk, max_chars, max_words) {
                tracing::warn!(
                    chunk_chars = chunk.chars().count(),
                    chunk_words = chunk.split_whitespace().count(),
                    max_chars,
                    max_words,
                    "Generated TTS chunk still exceeds configured limits"
                );
            }
        }

        chunks
    }

    fn push_segment_into_chunks(
        &self,
        chunks: &mut Vec<String>,
        current: &mut String,
        segment: &str,
        max_chars: usize,
        max_words: usize,
    ) {
        let segment = trim_boundary_noise(segment);
        if segment.is_empty() {
            return;
        }

        if exceeds_chunk_limits(segment, max_chars, max_words) {
            if !current.is_empty() {
                chunks.push(std::mem::take(current));
            }
            for sub in split_segment_by_words(segment, max_chars, max_words) {
                chunks.push(sub);
            }
            return;
        }

        let candidate = if current.is_empty() {
            segment.to_string()
        } else {
            format!("{current} {segment}")
        };
        if !exceeds_chunk_limits(&candidate, max_chars, max_words) {
            *current = candidate;
            return;
        }

        if !current.is_empty() {
            chunks.push(std::mem::take(current));
        }
        *current = segment.to_string();
    }

    fn config_hash(&self) -> String {
        let serialized = toml::to_string(&self.config).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(NORMALIZER_PIPELINE_REV.as_bytes());
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

    fn normalize_sentence_chunks_cached(
        &self,
        epub_path: &Path,
        config_hash: &str,
        sentence: &str,
    ) -> Option<Vec<String>> {
        let source_hash = hash_sentence(sentence);
        let cache_path = self.normalized_sentence_cache_path(epub_path, &source_hash, config_hash);

        if let Ok(contents) = fs::read_to_string(&cache_path) {
            if let Ok(cached) = toml::from_str::<NormalizedSentenceCache>(&contents) {
                if let Some(chunks) = cached.chunks {
                    return if chunks.is_empty() {
                        None
                    } else {
                        Some(chunks)
                    };
                }
                if let Some(normalized) = cached.normalized {
                    let chunks = self.chunk_sentence_for_tts(&normalized);
                    let upgraded = NormalizedSentenceCache {
                        normalized: Some(normalized),
                        chunks: Some(chunks.clone()),
                    };
                    self.write_normalized_sentence_cache(&cache_path, &upgraded);
                    return if chunks.is_empty() {
                        None
                    } else {
                        Some(chunks)
                    };
                }
                return None;
            }
        }

        let cleaned = self.clean_text_core(sentence);
        let normalized = self.finalize_sentence(&cleaned);
        let chunks = normalized
            .as_deref()
            .map(|text| self.chunk_sentence_for_tts(text))
            .unwrap_or_default();
        let cached = NormalizedSentenceCache {
            normalized,
            chunks: Some(chunks.clone()),
        };
        self.write_normalized_sentence_cache(&cache_path, &cached);

        if chunks.is_empty() {
            None
        } else {
            Some(chunks)
        }
    }

    fn write_normalized_sentence_cache(&self, cache_path: &Path, cached: &NormalizedSentenceCache) {
        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match toml::to_string(cached) {
            Ok(serialized) => {
                if let Err(err) = fs::write(cache_path, serialized) {
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
    }

    fn read_page_plan_cache(
        &self,
        cache_path: &Path,
        page_idx: usize,
    ) -> Option<PageNormalization> {
        let contents = fs::read_to_string(cache_path).ok()?;
        if let Ok(cached) = toml::from_str::<PageNormalization>(&contents) {
            tracing::debug!(
                path = %cache_path.display(),
                page = page_idx + 1,
                "Loaded normalized page cache"
            );
            return Some(cached);
        }
        if let Ok(cached) = toml::from_str::<PageNormalizationCache>(&contents) {
            let plan = cached.into_plan();
            tracing::debug!(
                path = %cache_path.display(),
                page = page_idx + 1,
                "Loaded normalized page cache"
            );
            return Some(plan);
        }
        None
    }

    fn write_page_plan_cache(&self, cache_path: &Path, page_idx: usize, plan: &PageNormalization) {
        let serializable = PageNormalizationCache::from_plan(plan);
        match toml::to_string(&serializable) {
            Ok(serialized) => {
                if let Err(err) = fs::write(cache_path, serialized) {
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
    }
}

fn resolve_default_normalizer_path() -> PathBuf {
    if let Some(value) = std::env::var_os(NORMALIZER_CONFIG_ENV) {
        let candidate = PathBuf::from(value);
        if candidate.exists() {
            return candidate;
        }
    }

    let relative = PathBuf::from(DEFAULT_NORMALIZER_PATH);
    if relative.exists() {
        return relative;
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest_dir.ancestors() {
        let candidate = ancestor.join(DEFAULT_NORMALIZER_PATH);
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(DEFAULT_NORMALIZER_PATH)
}

fn load_external_abbreviations(normalizer_config_path: &Path) -> AbbreviationConfig {
    let path = resolve_abbreviations_path(normalizer_config_path);
    let Ok(contents) = fs::read_to_string(&path) else {
        return AbbreviationConfig::default();
    };
    match toml::from_str::<AbbreviationsFile>(&contents) {
        Ok(file) => {
            let merged = file.abbreviations.merged();
            tracing::info!(
                path = %path.display(),
                count = merged.case.len() + merged.nocase.len(),
                "Loaded external abbreviations config"
            );
            merged
        }
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                "Invalid abbreviations config TOML: {err}"
            );
            AbbreviationConfig::default()
        }
    }
}

fn resolve_abbreviations_path(normalizer_config_path: &Path) -> PathBuf {
    if let Some(value) = std::env::var_os(ABBREVIATIONS_CONFIG_ENV) {
        let candidate = PathBuf::from(value);
        if candidate.exists() {
            return candidate;
        }
    }

    let relative = PathBuf::from(DEFAULT_ABBREVIATIONS_PATH);
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
        let candidate = ancestor.join(DEFAULT_ABBREVIATIONS_PATH);
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(DEFAULT_ABBREVIATIONS_PATH)
}

impl Default for TextNormalizer {
    fn default() -> Self {
        Self::from_config(NormalizerConfig::default())
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

fn default_abbreviations() -> AbbreviationConfig {
    let mut nocase = BTreeMap::new();
    nocase.insert("Mr.".to_string(), "Mister".to_string());
    nocase.insert("Ms.".to_string(), "Miss".to_string());
    nocase.insert("Mrs.".to_string(), "Misses".to_string());
    nocase.insert("Mass.".to_string(), "Massachusetts".to_string());
    nocase.insert("St.".to_string(), "Saint".to_string());
    AbbreviationConfig {
        case: BTreeMap::new(),
        nocase,
        regex: Vec::new(),
        legacy: BTreeMap::new(),
    }
}

fn apply_abbreviation_map(text: &str, abbreviations: &AbbreviationConfig) -> String {
    let mut out = text.to_string();
    let merged = abbreviations.merged();
    tracing::debug!(
        regex_rules = merged.regex.len(),
        case_rules = merged.case.len(),
        nocase_rules = merged.nocase.len(),
        "Applying abbreviation map"
    );

    for rule in &merged.regex {
        if rule.pattern.trim().is_empty() {
            continue;
        }
        let pattern = if rule.case_sensitive {
            rule.pattern.clone()
        } else {
            format!("(?i){}", rule.pattern)
        };
        if let Ok(re) = Regex::new(&pattern) {
            let hits = re.find_iter(&out).count();
            if hits > 0 {
                tracing::trace!(
                    pattern = %rule.pattern,
                    replacement = %rule.replace,
                    case_sensitive = rule.case_sensitive,
                    hits,
                    "Applied regex abbreviation rule"
                );
                out = re.replace_all(&out, rule.replace.as_str()).to_string();
            }
        } else {
            tracing::warn!(pattern = %rule.pattern, "Skipping invalid abbreviation regex rule");
        }
    }

    let mut case_entries: Vec<_> = merged.case.iter().collect();
    case_entries.sort_by_key(|(token, _)| Reverse(token.len()));
    let nocase_entries = build_nocase_abbreviation_entries(&merged.nocase);

    for (token, replacement) in case_entries {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }

        let pattern = if let Some(base) = trimmed.strip_suffix('.') {
            format!(r"\b{}\.", regex::escape(base))
        } else {
            format!(r"\b{}\b", regex::escape(trimmed))
        };

        if let Ok(re) = Regex::new(&pattern) {
            let hits = re.find_iter(&out).count();
            if hits > 0 {
                tracing::trace!(
                    token = %trimmed,
                    replacement = %replacement,
                    hits,
                    "Applied case-sensitive abbreviation rule"
                );
                out = re.replace_all(&out, replacement.as_str()).to_string();
            }
        } else {
            tracing::warn!(token = %trimmed, "Skipping invalid case-sensitive abbreviation token");
        }
    }

    for (token, replacement) in nocase_entries {
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
            let hits = re.find_iter(&out).count();
            if hits > 0 {
                tracing::trace!(
                    token = %trimmed,
                    replacement = %replacement,
                    hits,
                    "Applied case-insensitive abbreviation rule"
                );
                out = re.replace_all(&out, replacement.as_str()).to_string();
            }
        } else {
            tracing::warn!(
                token = %trimmed,
                "Skipping invalid case-insensitive abbreviation token"
            );
        }
    }

    out
}

fn build_nocase_abbreviation_entries(nocase: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut deduped: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (token, replacement) in nocase {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let folded = trimmed.to_ascii_lowercase();
        match deduped.get(&folded) {
            Some((existing_token, _)) => {
                if prefer_nocase_token(existing_token, trimmed) {
                    tracing::debug!(
                        folded = %folded,
                        previous_token = %existing_token,
                        chosen_token = %trimmed,
                        "Resolved case-insensitive abbreviation token collision"
                    );
                    deduped.insert(folded, (trimmed.to_string(), replacement.clone()));
                } else {
                    tracing::debug!(
                        folded = %folded,
                        previous_token = %existing_token,
                        skipped_token = %trimmed,
                        "Kept existing case-insensitive abbreviation token on collision"
                    );
                }
            }
            None => {
                deduped.insert(folded, (trimmed.to_string(), replacement.clone()));
            }
        }
    }

    let mut entries: Vec<(String, String)> = deduped.into_values().collect();
    entries.sort_by_key(|(token, _)| Reverse(token.len()));
    entries
}

fn prefer_nocase_token(existing: &str, candidate: &str) -> bool {
    let existing_lower = token_is_ascii_lowercase(existing);
    let candidate_lower = token_is_ascii_lowercase(candidate);
    if existing_lower != candidate_lower {
        return candidate_lower;
    }
    candidate.len() > existing.len()
}

fn token_is_ascii_lowercase(token: &str) -> bool {
    token
        .chars()
        .all(|ch| !ch.is_ascii_alphabetic() || ch.is_ascii_lowercase())
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

fn expand_acronym_match(caps: &regex::Captures, cfg: &AcronymConfig) -> String {
    let letters = caps[0]
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .map(|ch| {
            let key = ch.to_ascii_uppercase().to_string();
            cfg.letter_sounds.get(&key).cloned().unwrap_or(key)
        })
        .collect::<Vec<_>>();

    let mut spelled = letters.join(&cfg.letter_separator);
    if let Some(digits) = caps.name("digits") {
        let spoken_digits = digits
            .as_str()
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
    if spelled.is_empty() {
        caps[0].to_string()
    } else {
        spelled
    }
}

fn hash_sentences(sentences: &[String]) -> String {
    let mut hasher = Sha256::new();
    for sentence in sentences {
        hasher.update(sentence.as_bytes());
        hasher.update([0u8]);
    }
    format!("{:x}", hasher.finalize())
}

fn normalize_unicode_punctuation(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\u{2018}' | '\u{2019}' => out.push('\''),
            '\u{201C}' | '\u{201D}' => out.push('"'),
            '\u{2013}' | '\u{2014}' => {
                out.push(' ');
                out.push('-');
                out.push(' ');
            }
            '\u{2026}' => out.push_str("..."),
            _ => out.push(ch),
        }
    }
    out
}

fn trim_boundary_noise(input: &str) -> &str {
    input.trim_matches(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '"' | '\''
                    | '\u{2018}'
                    | '\u{2019}'
                    | '\u{201C}'
                    | '\u{201D}'
                    | '\u{00AB}'
                    | '\u{00BB}'
            )
    })
}

fn exceeds_chunk_limits(text: &str, max_chars: usize, max_words: usize) -> bool {
    text.chars().count() > max_chars || text.split_whitespace().count() > max_words
}

fn split_for_chunking(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, ',' | ';' | ':' | '.' | '!' | '?' | '\n') {
            let segment = RE_SOFT_BREAK_WS
                .replace_all(trim_boundary_noise(&current), " ")
                .to_string();
            if !segment.is_empty() {
                segments.push(segment);
            }
            current.clear();
        }
    }

    let tail = RE_SOFT_BREAK_WS
        .replace_all(trim_boundary_noise(&current), " ")
        .to_string();
    if !tail.is_empty() {
        segments.push(tail);
    }
    if segments.is_empty() {
        vec![text.trim().to_string()]
    } else {
        segments
    }
}

fn split_segment_by_words(segment: &str, max_chars: usize, max_words: usize) -> Vec<String> {
    let mut chunks = Vec::new();
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
            chunks.push(std::mem::take(&mut current));
            current_words = 0;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
        current_words += 1;
    }

    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn hash_sentence(sentence: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(sentence.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct NormalizedSentenceCache {
    #[serde(default)]
    normalized: Option<String>,
    #[serde(default)]
    chunks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
struct AbbreviationsFile {
    abbreviations: AbbreviationConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
struct AbbreviationConfig {
    case: BTreeMap<String, String>,
    nocase: BTreeMap<String, String>,
    regex: Vec<AbbreviationRegexRule>,
    #[serde(flatten)]
    legacy: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
struct AbbreviationRegexRule {
    pattern: String,
    replace: String,
    #[serde(default)]
    case_sensitive: bool,
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

    fn extend(&mut self, other: Self) {
        let other = other.merged();
        for (k, v) in other.case {
            self.case.insert(k, v);
        }
        for (k, v) in other.nocase {
            self.nocase.insert(k, v);
        }
        self.regex.extend(other.regex);
    }

    fn is_empty(&self) -> bool {
        self.case.is_empty()
            && self.nocase.is_empty()
            && self.regex.is_empty()
            && self.legacy.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct PageNormalizationCache {
    audio_sentences: Vec<String>,
    // TOML cannot encode `None` in arrays directly; use `-1` sentinel.
    display_to_audio: Vec<isize>,
    audio_to_display: Vec<usize>,
}

impl PageNormalizationCache {
    fn from_plan(plan: &PageNormalization) -> Self {
        Self {
            audio_sentences: plan.audio_sentences.clone(),
            display_to_audio: plan
                .display_to_audio
                .iter()
                .map(|entry| entry.map(|idx| idx as isize).unwrap_or(-1))
                .collect(),
            audio_to_display: plan.audio_to_display.clone(),
        }
    }

    fn into_plan(self) -> PageNormalization {
        PageNormalization {
            audio_sentences: self.audio_sentences,
            display_to_audio: self
                .display_to_audio
                .into_iter()
                .map(|idx| if idx < 0 { None } else { Some(idx as usize) })
                .collect(),
            audio_to_display: self.audio_to_display,
        }
    }
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
    fn loads_external_abbreviations_file_for_expansion() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lanternleaf-normalizer-test-{nonce}"));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let normalizer_path = dir.join("normalizer.toml");
        let abbreviations_path = dir.join("abbreviations.toml");
        fs::write(&normalizer_path, "[normalization]\nmode = \"sentence\"\n")
            .expect("normalizer config should be written");
        fs::write(
            &abbreviations_path,
            "[abbreviations.nocase]\n\"pp.\" = \"pages\"\n",
        )
        .expect("abbreviations config should be written");

        let normalizer = TextNormalizer::load(&normalizer_path);

        let plan = normalizer.plan_page(&["See pp. 10.".to_string()]);
        assert!(
            plan.audio_sentences.iter().any(|s| s.contains("pages")),
            "expected external abbreviation expansion to be applied"
        );

        let _ = fs::remove_file(&normalizer_path);
        let _ = fs::remove_file(&abbreviations_path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn external_case_sensitive_abbreviation_only_matches_exact_case() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lanternleaf-normalizer-test-{nonce}"));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let normalizer_path = dir.join("normalizer.toml");
        let abbreviations_path = dir.join("abbreviations.toml");
        fs::write(&normalizer_path, "[normalization]\nmode = \"sentence\"\n")
            .expect("normalizer config should be written");
        fs::write(
            &abbreviations_path,
            "[abbreviations.case]\n\"US.\" = \"United States\"\n",
        )
        .expect("abbreviations config should be written");

        let normalizer = TextNormalizer::load(&normalizer_path);

        let upper = normalizer.plan_page(&["US. policy".to_string()]);
        let lower = normalizer.plan_page(&["us. policy".to_string()]);
        assert!(
            upper
                .audio_sentences
                .iter()
                .any(|s| s.contains("United States")),
            "expected exact-case abbreviation expansion to apply"
        );
        assert!(
            lower
                .audio_sentences
                .iter()
                .any(|s| s.contains("us. policy")),
            "expected lowercase token to remain unchanged for case-sensitive entry"
        );

        let _ = fs::remove_file(&normalizer_path);
        let _ = fs::remove_file(&abbreviations_path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn external_regex_abbreviation_expands_page_number_pattern() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lanternleaf-normalizer-test-{nonce}"));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let normalizer_path = dir.join("normalizer.toml");
        let abbreviations_path = dir.join("abbreviations.toml");
        fs::write(&normalizer_path, "[normalization]\nmode = \"sentence\"\n")
            .expect("normalizer config should be written");
        fs::write(
            &abbreviations_path,
            "[[abbreviations.regex]]\npattern = 'p\\.\\s*(\\d+)\\.'\nreplace = 'page $1'\ncase_sensitive = false\n",
        )
        .expect("abbreviations config should be written");

        let normalizer = TextNormalizer::load(&normalizer_path);

        let plan = normalizer.plan_page(&["See p. 169. now.".to_string()]);
        assert!(
            plan.audio_sentences.iter().any(|s| s.contains("page 169")),
            "expected regex abbreviation expansion to map p. <number>. to page <number>"
        );

        let _ = fs::remove_file(&normalizer_path);
        let _ = fs::remove_file(&abbreviations_path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn regex_page_rule_takes_precedence_over_literal_p_rule() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lanternleaf-normalizer-test-{nonce}"));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let normalizer_path = dir.join("normalizer.toml");
        let abbreviations_path = dir.join("abbreviations.toml");
        fs::write(&normalizer_path, "[normalization]\nmode = \"sentence\"\n")
            .expect("normalizer config should be written");
        fs::write(
            &abbreviations_path,
            "[abbreviations.nocase]\n\"p.\" = \"page\"\n\n[[abbreviations.regex]]\npattern = 'p\\.\\s*(\\d+)\\.'\nreplace = 'page $1'\ncase_sensitive = false\n",
        )
        .expect("abbreviations config should be written");

        let normalizer = TextNormalizer::load(&normalizer_path);

        let plan = normalizer.plan_page(&["See p. 169. and p. 8.".to_string()]);
        let joined = plan.audio_sentences.join(" ");
        assert!(
            joined.contains("page 169"),
            "expected regex to map p. 169. into page 169"
        );
        assert!(
            joined.contains("page 8"),
            "expected regex to map p. 8. into page 8"
        );
        assert!(
            !joined.contains("page page"),
            "unexpected double replacement indicates literal p. won before regex"
        );

        let _ = fs::remove_file(&normalizer_path);
        let _ = fs::remove_file(&abbreviations_path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mixed_case_literal_abbreviation_entries_do_not_break_page_mapping() {
        let mut config = AbbreviationConfig::default();
        config.nocase.insert("p.".to_string(), "page".to_string());
        config.nocase.insert("P.".to_string(), "P".to_string());
        config.regex.push(AbbreviationRegexRule {
            pattern: "p\\.\\s*(\\d+)\\.?".to_string(),
            replace: "page $1".to_string(),
            case_sensitive: false,
        });

        let joined = apply_abbreviation_map("See p. 42 and p. then P. Hart.", &config);
        assert!(
            joined.contains("page 42"),
            "expected p. <number> to expand into page <number>"
        );
        assert!(
            joined.contains("and page then"),
            "expected literal p. to expand into page"
        );
        assert!(
            !joined.contains("pageage"),
            "unexpected malformed replacement indicates duplicate abbreviation collision"
        );
    }

    #[test]
    fn numeric_range_regex_expands_into_to_phrase() {
        let mut config = AbbreviationConfig::default();
        config.regex.push(AbbreviationRegexRule {
            pattern: r"\b(\d+)-(\d+)\b".to_string(),
            replace: "$1 to $2".to_string(),
            case_sensitive: false,
        });

        let output = apply_abbreviation_map("Read chapters 12-34 tonight.", &config);
        assert!(
            output.contains("12 to 34"),
            "expected numeric range to expand into '<number> to <number>'"
        );
    }

    #[test]
    fn sentence_mode_cache_reused_across_page_indices() {
        let normalizer = TextNormalizer::default();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let epub_path = std::env::temp_dir().join(format!("lanternleaf-normalizer-{nonce}.epub"));
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
        let page_cache_files = files_after_second
            .iter()
            .filter(|name| name.starts_with("p"))
            .count();
        assert!(
            page_cache_files >= 2,
            "sentence mode should persist page-level normalization plans for fast reloads"
        );

        let _ = fs::remove_dir_all(&cache_root);
    }

    #[test]
    fn splits_oversized_sentence_into_audio_chunks_with_stable_mapping() {
        let normalizer = TextNormalizer::default();
        let page = vec![String::from(
            "In the word lists of Cheshire, Derbyshire, Lancashire and Yorkshire we find the \
            following terms, all of which took root in the Delaware Valley: abide as in cannot \
            abide it, all out for entirely, apple-pie order to mean very good order, bamboozle \
            for deceive, black and white for writing, blather for empty talk, boggle for take \
            fright, brat for child, budge for move, burying for funeral, by golly as an \
            expletive, by gum for another expletive.",
        )];

        let plan = normalizer.plan_page(&page);
        assert!(
            plan.audio_sentences.len() > 1,
            "long comma-heavy sentence should be split into multiple audio chunks"
        );
        assert_eq!(plan.display_to_audio, vec![Some(0)]);
        assert!(
            plan.audio_to_display.iter().all(|idx| *idx == 0),
            "all generated chunks should map back to the original display sentence"
        );
        assert!(
            plan.audio_sentences
                .iter()
                .all(|chunk| chunk.chars().count() <= 180 && chunk.split_whitespace().count() <= 32),
            "chunking should enforce configured max chunk size"
        );
    }

    #[test]
    fn normalizes_unicode_quotes_and_dashes_for_tts() {
        let normalizer = TextNormalizer::default();
        let page = vec![String::from("“Quote”—and ‘apostrophe’ … done.")];
        let plan = normalizer.plan_page(&page);
        assert_eq!(plan.audio_sentences.len(), 1);
        assert_eq!(plan.audio_sentences[0], "Quote - and 'apostrophe'... done.");
    }

    #[test]
    fn drops_ascii_and_fancy_square_bracket_blocks() {
        let normalizer = TextNormalizer::default();
        let page = vec![String::from(
            "Alpha [12] beta ”【50†L260-L270】 gamma ［note］ delta.",
        )];
        let plan = normalizer.plan_page(&page);
        assert_eq!(plan.audio_sentences.len(), 1);
        assert_eq!(plan.audio_sentences[0], "Alpha beta gamma delta.");
    }
}
