use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const DEFAULT_NORMALIZER_PATH: &str = "conf/normalizer.toml";
const SENTENCE_MARKER: &str = "\n<<__EBUP_SENTENCE_BOUNDARY__>>\n";

static RE_INLINE_CODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"`([^`]+)`").unwrap());
static RE_MARKDOWN_LINK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap());
static RE_NUMERIC_BRACKET_CITE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[\s*\d+(?:\s*,\s*\d+)*\s*\]").unwrap());
static RE_PARENTHETICAL_NUMERIC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\(\s*\d+(?:\s*,\s*\d+)*\s*\)").unwrap());
static RE_SQUARE_BRACKET_BLOCK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[[^\]]*\]").unwrap());
static RE_CURLY_BRACKET_BLOCK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\{[^}]*\}").unwrap());
static RE_HORIZONTAL_WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t\u{00A0}]+").unwrap());
static RE_SPACE_BEFORE_PUNCT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+([,.;:!?])").unwrap());

#[derive(Debug, Clone)]
pub struct TextNormalizer {
    config: NormalizerConfig,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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
    drop_square_bracket_text: bool,
    drop_curly_brace_text: bool,
    min_sentence_chars: usize,
    require_alphanumeric: bool,
    replacements: BTreeMap<String, String>,
    drop_tokens: Vec<String>,
}

impl Default for NormalizerConfig {
    fn default() -> Self {
        let mut replacements = BTreeMap::new();
        replacements.insert("#".to_string(), " ".to_string());

        Self {
            enabled: true,
            mode: NormalizationMode::Page,
            collapse_whitespace: true,
            remove_space_before_punctuation: true,
            strip_inline_code: true,
            strip_markdown_links: true,
            drop_numeric_bracket_citations: true,
            drop_parenthetical_numeric_citations: true,
            drop_square_bracket_text: true,
            drop_curly_brace_text: true,
            min_sentence_chars: 2,
            require_alphanumeric: true,
            replacements,
            drop_tokens: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
enum NormalizationMode {
    #[default]
    Page,
    Sentence,
}

#[derive(Debug, Clone)]
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

        if self.config.drop_square_bracket_text {
            text = RE_SQUARE_BRACKET_BLOCK.replace_all(&text, " ").to_string();
        }

        if self.config.drop_curly_brace_text {
            text = RE_CURLY_BRACKET_BLOCK.replace_all(&text, " ").to_string();
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
}

impl Default for TextNormalizer {
    fn default() -> Self {
        Self {
            config: NormalizerConfig::default(),
        }
    }
}
