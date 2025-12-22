//! Text-to-speech support using `piper-rs` with caching in `.cache`.
//! Audio is generated per sentence and stored as WAV for reuse.

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use piper_rs::synth::{AudioOutputConfig, PiperSpeechSynthesizer};
use piper_rs::from_config_path;
use rodio::source::Zero;
use rodio::{Decoder, OutputStream, Sink, Source};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::{debug, info, warn};

static PIPER_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Clone)]
pub struct TtsEngine {
    model_path: PathBuf,
}

impl TtsEngine {
    pub fn new(model_path: PathBuf, espeak_path: PathBuf) -> Result<Self> {
        let espeak_path = sanitize_espeak_root(espeak_path);
        if env::var_os("PIPER_ESPEAKNG_DATA_DIRECTORY").is_none() {
            // Safe because we set a deterministic value early in process startup.
            unsafe {
                env::set_var("PIPER_ESPEAKNG_DATA_DIRECTORY", &espeak_path);
            }
        }
        info!(
            model = %model_path.display(),
            espeak_root = %espeak_path.display(),
            "Initializing TTS engine"
        );
        Ok(Self {
            model_path,
        })
    }

    /// Play a list of audio files sequentially; returns a sink to control playback.
    pub fn play_files(
        &self,
        files: &[PathBuf],
        pause_after: std::time::Duration,
    ) -> Result<TtsPlayback> {
        let (_stream, handle) = OutputStream::try_default().context("Opening audio output")?;
        let sink = Sink::try_new(&handle).context("Creating sink")?;

        info!(
            count = files.len(),
            pause_ms = pause_after.as_millis(),
            "Starting TTS playback"
        );
        for file in files {
            let reader = BufReader::new(File::open(file)?);
            let source = Decoder::new(reader)?;
            sink.append(source);
            if pause_after > std::time::Duration::ZERO {
                let silence = Zero::<f32>::new(1, 48_000).take_duration(pause_after);
                sink.append(silence);
            }
        }

        sink.play();
        Ok(TtsPlayback { _stream, sink })
    }

    /// Prepare a batch of sentences concurrently using a thread pool.
    pub fn prepare_batch(
        &self,
        cache_root: PathBuf,
        sentences: Vec<String>,
        start_idx: usize,
        speed: f32,
        threads: usize,
    ) -> Result<Vec<(PathBuf, std::time::Duration)>> {
        let _lock = PIPER_GUARD.lock().unwrap();
        let config_path = resolve_piper_config(&self.model_path);
        if !config_path.exists() {
            anyhow::bail!(
                "Piper config not found at {} (expected from {})",
                config_path.display(),
                self.model_path.display()
            );
        }
        let model = from_config_path(&config_path).context("Loading Piper model")?;
        let piper = PiperSpeechSynthesizer::new(model).context("Preparing Piper synthesizer")?;

        info!(
            sentence_count = sentences.len(),
            start_idx, speed, threads, "Preparing TTS batch"
        );

        let mut collected = Vec::new();
        for sentence in sentences.into_iter().skip(start_idx) {
            let path = cache_path(&cache_root, &self.model_path, &sentence, speed);

            if !path.exists() {
                debug!(path = %path.display(), "Synthesizing new sentence");
                if let Some(parent) = path.parent() {
                    if let Err(err) = fs::create_dir_all(parent) {
                        warn!("Failed to create TTS cache dir: {err}");
                        return Err(err.into());
                    }
                }

                if let Err(err) = synth_with_piper(&piper, &path, &sentence, speed) {
                    warn!("Failed to synthesize sentence: {err}");
                    return Err(err);
                }
            }

            let dur = sentence_duration(&path);
            collected.push((path, dur));
        }
        debug!(count = collected.len(), "Prepared TTS batch");
        Ok(collected)
    }
}

pub struct TtsPlayback {
    _stream: OutputStream,
    sink: Sink,
}

impl TtsPlayback {
    pub fn pause(&self) {
        debug!("Pausing playback");
        self.sink.pause();
    }

    pub fn play(&self) {
        debug!("Resuming playback");
        self.sink.play();
    }

    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }

    pub fn stop(self) {
        self.sink.stop();
        // stream dropped automatically
    }
}

fn cache_path(base: &Path, model_path: &Path, sentence: &str, speed: f32) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(model_path.as_os_str().to_string_lossy().as_bytes());
    hasher.update(sentence.as_bytes());
    hasher.update(speed.to_le_bytes());
    let hash = format!("{:x}", hasher.finalize());
    base.join(format!("tts-{hash}.wav"))
}

/// Piper expects the parent directory that contains `espeak-ng-data/phonindex`.
/// Users often point directly at `.../espeak-ng-data`; trim that to avoid
/// duplicated segments like `/espeak-ng-data/espeak-ng-data/phonindex`.
fn sanitize_espeak_root(path: PathBuf) -> PathBuf {
    if path
        .file_name()
        .map(|n| n == "espeak-ng-data")
        .unwrap_or(false)
    {
        if let Some(parent) = path.parent() {
            debug!(
                original = %path.display(),
                sanitized = %parent.display(),
                "Trimming espeak-ng-data suffix"
            );
            return parent.to_path_buf();
        }
    }
    path
}

fn sentence_duration(path: &Path) -> std::time::Duration {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return std::time::Duration::from_secs(1),
    };
    let reader = BufReader::new(file);
    Decoder::new(reader)
        .ok()
        .and_then(|d| d.total_duration())
        .unwrap_or(std::time::Duration::from_secs(1))
}

fn synth_with_piper(
    piper: &PiperSpeechSynthesizer,
    path: &Path,
    sentence: &str,
    speed: f32,
) -> Result<()> {
    debug!(
        path = %path.display(),
        speed,
        chars = sentence.len(),
        "Synthesizing sentence with Piper"
    );
    let output_config = if (speed - 1.0).abs() <= f32::EPSILON {
        None
    } else {
        Some(AudioOutputConfig {
            rate: Some(speed_to_rate_percent(speed)),
            volume: None,
            pitch: None,
            appended_silence_ms: None,
        })
    };
    piper
        .synthesize_to_file(path, sentence.to_string(), output_config)
        .context("Synthesizing audio")?;
    Ok(())
}

fn resolve_piper_config(model_path: &Path) -> PathBuf {
    if model_path
        .extension()
        .map(|ext| ext == "onnx")
        .unwrap_or(false)
    {
        return model_path.with_extension("onnx.json");
    }
    model_path.to_path_buf()
}

fn speed_to_rate_percent(speed: f32) -> u8 {
    let clamped = speed.clamp(0.5, 5.5);
    let percent = ((clamped - 0.5) / 5.0) * 100.0;
    percent.round().clamp(0.0, 100.0) as u8
}
