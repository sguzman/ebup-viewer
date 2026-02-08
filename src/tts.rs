//! Text-to-speech support using `piper-rs` with caching in `.cache`.
//! Audio is generated per sentence and stored as WAV for reuse.

use anyhow::{Context, Result};
use rodio::buffer::SamplesBuffer;
use rodio::source::Zero;
use rodio::{Decoder, OutputStream, Sink, Source};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::thread;
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct TtsEngine {
    model_path: PathBuf,
    espeak_root: PathBuf,
    worker_pool: Arc<Mutex<Option<WorkerPoolState>>>,
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
            espeak_root: espeak_path,
            worker_pool: Arc::new(Mutex::new(None)),
        })
    }

    /// Play a list of audio files sequentially; returns a sink to control playback.
    pub fn play_files(
        &self,
        files: &[PathBuf],
        pause_after: std::time::Duration,
        speed: f32,
    ) -> Result<TtsPlayback> {
        let (_stream, handle) = OutputStream::try_default().context("Opening audio output")?;
        let sink = Sink::try_new(&handle).context("Creating sink")?;
        let speed = if speed <= f32::EPSILON { 1.0 } else { speed };

        info!(
            count = files.len(),
            pause_ms = pause_after.as_millis(),
            speed,
            "Starting TTS playback"
        );
        let mut sentence_durations = Vec::with_capacity(files.len());
        for file in files {
            let reader = BufReader::new(File::open(file)?);
            let source = Decoder::new(reader)?;
            if (speed - 1.0).abs() <= f32::EPSILON {
                let dur = source
                    .total_duration()
                    .unwrap_or_else(|| sentence_duration(file));
                sentence_durations.push(dur);
                sink.append(source);
            } else {
                let channels = source.channels() as u16;
                let sample_rate = source.sample_rate();
                let samples: Vec<f32> = source.convert_samples().collect();
                let stretched = time_stretch(&samples, sample_rate, channels, speed)
                    .context("Time-stretching audio")?;
                let dur = std::time::Duration::from_secs_f64(
                    stretched.len() as f64 / (sample_rate as f64 * channels as f64),
                );
                sentence_durations.push(dur);
                let buffer = SamplesBuffer::new(channels, sample_rate, stretched);
                sink.append(buffer);
            }
            if pause_after > std::time::Duration::ZERO {
                let silence = Zero::<f32>::new(1, 48_000).take_duration(pause_after);
                sink.append(silence);
            }
        }

        sink.play();
        Ok(TtsPlayback {
            _stream,
            sink,
            sentence_durations,
        })
    }

    /// Prepare a batch of sentences concurrently using a thread pool.
    pub fn prepare_batch(
        &self,
        cache_root: PathBuf,
        sentences: Vec<String>,
        start_idx: usize,
        threads: usize,
    ) -> Result<Vec<(PathBuf, std::time::Duration)>> {
        info!(
            sentence_count = sentences.len(),
            start_idx, threads, "Preparing TTS batch"
        );

        let threads = threads.max(1);
        let pool = self.ensure_worker_pool(threads)?;
        let total = sentences.len().saturating_sub(start_idx);
        let mut collected: Vec<Option<(PathBuf, std::time::Duration)>> = vec![None; total];
        let mut pending: Vec<(usize, PathBuf, mpsc::Receiver<Result<()>>)> = Vec::new();

        for (offset, sentence) in sentences.into_iter().skip(start_idx).enumerate() {
            let path = cache_path(&cache_root, &self.model_path, &sentence);
            if path.exists() {
                let dur = sentence_duration(&path);
                collected[offset] = Some((path, dur));
                continue;
            }

            if let Some(parent) = path.parent() {
                if let Err(err) = fs::create_dir_all(parent) {
                    warn!("Failed to create TTS cache dir: {err}");
                    return Err(err.into());
                }
            }

            let (result_tx, result_rx) = mpsc::channel();
            pool.dispatch(sentence, path.clone(), result_tx)?;
            pending.push((offset, path, result_rx));
        }

        for (offset, path, result_rx) in pending {
            match result_rx.recv() {
                Ok(Ok(())) => {
                    let dur = sentence_duration(&path);
                    collected[offset] = Some((path, dur));
                }
                Ok(Err(err)) => {
                    warn!("Failed to synthesize sentence: {err}");
                    return Err(err);
                }
                Err(err) => {
                    return Err(anyhow::anyhow!("TTS worker channel closed: {err}"));
                }
            }
        }

        let collected: Vec<(PathBuf, std::time::Duration)> = collected
            .into_iter()
            .flatten()
            .collect();
        debug!(count = collected.len(), "Prepared TTS batch");
        Ok(collected)
    }

    fn ensure_worker_pool(&self, threads: usize) -> Result<Arc<WorkerPool>> {
        let mut guard = self.worker_pool.lock().unwrap();
        let rebuild = match guard.as_ref() {
            Some(state) => state.threads != threads,
            None => true,
        };
        if rebuild {
            let pool = WorkerPool::new(threads, &self.model_path, &self.espeak_root)?;
            *guard = Some(WorkerPoolState {
                threads,
                pool: Arc::new(pool),
            });
        }
        Ok(guard.as_ref().unwrap().pool.clone())
    }
}

pub struct TtsPlayback {
    _stream: OutputStream,
    sink: Sink,
    sentence_durations: Vec<std::time::Duration>,
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

    pub fn sentence_durations(&self) -> &[std::time::Duration] {
        &self.sentence_durations
    }

    pub fn queued_sources(&self) -> usize {
        self.sink.len()
    }
}

fn cache_path(base: &Path, model_path: &Path, sentence: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(model_path.as_os_str().to_string_lossy().as_bytes());
    hasher.update(sentence.as_bytes());
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

fn time_stretch(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    speed: f32,
) -> Result<Vec<f32>> {
    if (speed - 1.0).abs() <= f32::EPSILON {
        return Ok(samples.to_vec());
    }

    let mut out_buf: Vec<f32> = Vec::new();
    unsafe {
        let stream = sonic_rs_sys::sonicCreateStream(sample_rate as i32, channels as i32);
        sonic_rs_sys::sonicSetSpeed(stream, speed);
        sonic_rs_sys::sonicWriteFloatToStream(stream, samples.as_ptr(), samples.len() as i32);
        sonic_rs_sys::sonicFlushStream(stream);
        let num_samples = sonic_rs_sys::sonicSamplesAvailable(stream);
        if num_samples <= 0 {
            sonic_rs_sys::sonicDestroyStream(stream);
            anyhow::bail!("Sonic error: no samples available after time-stretch");
        }
        out_buf.reserve_exact(num_samples as usize);
        sonic_rs_sys::sonicReadFloatFromStream(
            stream,
            out_buf.spare_capacity_mut().as_mut_ptr().cast(),
            num_samples,
        );
        sonic_rs_sys::sonicDestroyStream(stream);
        out_buf.set_len(num_samples as usize);
    }
    Ok(out_buf)
}

#[derive(Serialize)]
struct WorkerRequest {
    text: String,
    path: String,
}

#[derive(Serialize)]
struct WorkerShutdown {
    shutdown: bool,
}

#[derive(Deserialize)]
struct WorkerResponse {
    ok: bool,
    error: Option<String>,
}

struct WorkerPoolState {
    threads: usize,
    pool: Arc<WorkerPool>,
}

struct WorkerPool {
    workers: Vec<WorkerHandle>,
    next: AtomicUsize,
}

struct WorkerHandle {
    tx: mpsc::Sender<Job>,
}

enum Job {
    Synthesize {
        sentence: String,
        path: PathBuf,
        result_tx: mpsc::Sender<Result<()>>,
    },
    Shutdown,
}

impl WorkerPool {
    fn new(threads: usize, model_path: &Path, espeak_root: &Path) -> Result<Self> {
        let mut workers = Vec::with_capacity(threads);
        for _ in 0..threads {
            let (tx, rx) = mpsc::channel::<Job>();
            let model_path = model_path.to_path_buf();
            let espeak_root = espeak_root.to_path_buf();
            thread::spawn(move || worker_loop(rx, model_path, espeak_root));
            workers.push(WorkerHandle { tx });
        }
        Ok(Self {
            workers,
            next: AtomicUsize::new(0),
        })
    }

    fn dispatch(
        &self,
        sentence: String,
        path: PathBuf,
        result_tx: mpsc::Sender<Result<()>>,
    ) -> Result<()> {
        let idx = self.next.fetch_add(1, Ordering::Relaxed) % self.workers.len();
        self.workers[idx]
            .tx
            .send(Job::Synthesize {
                sentence,
                path,
                result_tx,
            })
            .map_err(|err| anyhow::anyhow!("TTS worker channel closed: {err}"))
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        for worker in &self.workers {
            let _ = worker.tx.send(Job::Shutdown);
        }
    }
}

fn worker_loop(rx: mpsc::Receiver<Job>, model_path: PathBuf, espeak_root: PathBuf) {
    let child = spawn_worker(&model_path, &espeak_root);
    let mut child = match child {
        Ok(child) => child,
        Err(err) => {
            let err_msg = err.to_string();
            warn!("Failed to spawn TTS worker: {err_msg}");
            for job in rx {
                if let Job::Synthesize { result_tx, .. } = job {
                    let _ = result_tx.send(Err(anyhow::anyhow!(err_msg.clone())));
                }
            }
            return;
        }
    };

    let mut stdin = BufWriter::new(child.stdin.take().unwrap());
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();

    for job in rx {
        match job {
            Job::Synthesize {
                sentence,
                path,
                result_tx,
            } => {
                let request = WorkerRequest {
                    text: sentence,
                    path: path.to_string_lossy().to_string(),
                };
                if let Err(err) = send_request(&mut stdin, &request) {
                    let _ = result_tx.send(Err(err));
                    break;
                }
                match read_response(&mut stdout, &mut line) {
                    Ok(()) => {
                        let _ = result_tx.send(Ok(()));
                    }
                    Err(err) => {
                        let _ = result_tx.send(Err(err));
                        break;
                    }
                }
            }
            Job::Shutdown => {
                let _ = send_request(&mut stdin, &WorkerShutdown { shutdown: true });
                break;
            }
        }
    }

    let _ = child.wait();
}

fn spawn_worker(model_path: &Path, espeak_root: &Path) -> Result<std::process::Child> {
    let exe = env::current_exe().context("Finding current executable")?;
    Command::new(exe)
        .arg("--tts-worker")
        .arg("--model")
        .arg(model_path)
        .arg("--espeak")
        .arg(espeak_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("Starting TTS worker process")
}

fn send_request<T: Serialize>(stdin: &mut BufWriter<std::process::ChildStdin>, request: &T) -> Result<()> {
    let payload = serde_json::to_string(request).context("Encoding worker request")?;
    stdin.write_all(payload.as_bytes())?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    Ok(())
}

fn read_response(
    stdout: &mut BufReader<std::process::ChildStdout>,
    line: &mut String,
) -> Result<()> {
    line.clear();
    let read = stdout.read_line(line)?;
    if read == 0 {
        anyhow::bail!("Worker process closed its stdout");
    }
    let response: WorkerResponse =
        serde_json::from_str(line.trim()).context("Decoding worker response")?;
    if response.ok {
        Ok(())
    } else {
        let msg = response
            .error
            .unwrap_or_else(|| "Unknown worker error".to_string());
        Err(anyhow::anyhow!(msg))
    }
}
