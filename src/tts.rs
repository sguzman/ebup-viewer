//! Text-to-speech support using `piper1-rs` with caching in `.cache`.
//! Audio is generated per sentence and stored as WAV for reuse.

use anyhow::{Context, Result};
use hound::WavSpec;
use piper1_rs::{Piper, PiperSynthesisOptions};
use rodio::{Decoder, OutputStream, Sink, Source};
use sha2::{Digest, Sha256};
use threadpool::ThreadPool;
use std::sync::mpsc;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct TtsEngine {
    model_path: PathBuf,
    espeak_path: PathBuf,
}

impl TtsEngine {
    pub fn new(model_path: PathBuf, espeak_path: PathBuf) -> Result<Self> {
        let espeak_path = sanitize_espeak_root(espeak_path);
        Ok(Self { model_path, espeak_path })
    }

    /// Ensure audio for a sentence exists, returning the cached path.
    pub fn ensure_audio(&self, cache_root: &Path, sentence: &str, speed: f32) -> Result<PathBuf> {
        let path = cache_path(cache_root, &self.model_path, sentence, speed);
        if path.exists() {
            return Ok(path);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut piper = Piper::new(
            self.model_path.to_string_lossy().to_string(),
            None::<String>,
            self.espeak_path.to_string_lossy().to_string(),
        )
        .context("Loading Piper model")?;
        let mut options: PiperSynthesisOptions = piper.get_default_synthesis_options();

        // Piper length_scale is roughly inverse speed.
        let length_scale = (1.0 / speed).clamp(0.25, 4.0);
        options.set_length_scale(length_scale);

        let mut handle = piper
            .start_synthesis(sentence.to_string(), &options)
            .context("Synthesizing audio")?;

        let mut samples: Vec<f32> = Vec::new();
        let mut sample_rate = 22050u32;
        while let Some(chunk) = handle.get_next_chunk()? {
            sample_rate = chunk.sample_rate();
            samples.extend_from_slice(chunk.samples());
        }

        write_wav(&path, sample_rate, &samples)?;

        Ok(path)
    }

    /// Play a list of audio files sequentially; returns a sink to control playback.
    pub fn play_files(&self, files: &[PathBuf]) -> Result<TtsPlayback> {
        let (_stream, handle) = OutputStream::try_default().context("Opening audio output")?;
        let sink = Sink::try_new(&handle).context("Creating sink")?;

        for file in files {
            let reader = BufReader::new(File::open(file)?);
            let source = Decoder::new(reader)?;
            sink.append(source);
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
        let pool = ThreadPool::new(threads.max(1));
        let (tx, rx) = mpsc::channel();
        let engine = self.clone();
        pool.execute(move || {
            let mut collected = Vec::new();
            for sentence in sentences.into_iter().skip(start_idx) {
                match engine
                    .ensure_audio(&cache_root, &sentence, speed)
                    .and_then(|path| {
                        let dur = sentence_duration(&path);
                        Ok((path, dur))
                    }) {
                    Ok(pair) => collected.push(pair),
                    Err(err) => {
                        let _ = tx.send(Err(err));
                        return;
                    }
                }
            }
            let _ = tx.send(Ok(collected));
        });

        // Only one job is submitted; receive its result.
        match rx.recv() {
            Ok(res) => res,
            Err(_) => Ok(Vec::new()),
        }
    }
}

pub struct TtsPlayback {
    _stream: OutputStream,
    sink: Sink,
}

impl TtsPlayback {
    pub fn pause(&self) {
        self.sink.pause();
    }

    pub fn play(&self) {
        self.sink.play();
    }

    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }

    pub fn is_finished(&self) -> bool {
        self.sink.empty()
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
            return parent.to_path_buf();
        }
    }
    path
}

fn write_wav(path: &Path, sample_rate: u32, samples: &[f32]) -> Result<()> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        let clamped = (s * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        writer.write_sample(clamped)?;
    }
    writer.finalize()?;
    Ok(())
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
