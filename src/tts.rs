//! Text-to-speech support using `piper1-rs` with caching in `.cache`.
//! Audio is generated per sentence and stored as WAV for reuse.

use anyhow::{Context, Result};
use hound::WavSpec;
use piper1_rs::{Piper, PiperSynthesisOptions};
use rodio::{Decoder, OutputStream, Sink};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct TtsEngine {
    model_path: PathBuf,
    espeak_path: PathBuf,
    inner: Arc<Mutex<Piper>>,
}

impl TtsEngine {
    pub fn new(model_path: PathBuf, espeak_path: PathBuf) -> Result<Self> {
        let piper = Piper::new(
            model_path.to_string_lossy().to_string(),
            None::<String>,
            espeak_path.to_string_lossy().to_string(),
        )
            .context("Loading Piper model")?;
        Ok(Self {
            model_path,
            espeak_path,
            inner: Arc::new(Mutex::new(piper)),
        })
    }

    /// Ensure audio for a sentence exists, returning the cached path.
    pub fn ensure_audio(
        &self,
        cache_root: &Path,
        sentence: &str,
        speed: f32,
    ) -> Result<PathBuf> {
        let path = cache_path(cache_root, &self.model_path, sentence, speed);
        if path.exists() {
            return Ok(path);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut piper = self.inner.lock().expect("tts mutex poisoned");
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
}

fn cache_path(base: &Path, model_path: &Path, sentence: &str, speed: f32) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(model_path.as_os_str().to_string_lossy().as_bytes());
    hasher.update(sentence.as_bytes());
    hasher.update(speed.to_le_bytes());
    let hash = format!("{:x}", hasher.finalize());
    base.join(format!("tts-{hash}.wav"))
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
        let clamped = (s * i16::MAX as f32)
            .clamp(i16::MIN as f32, i16::MAX as f32)
            as i16;
        writer.write_sample(clamped)?;
    }
    writer.finalize()?;
    Ok(())
}
