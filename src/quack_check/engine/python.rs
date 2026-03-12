use super::{Engine, types::*};
use crate::cancellation::CancellationToken;
use crate::quack_check::config::Config;
use anyhow::{Context, Result, anyhow};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

pub struct PythonEngine {
    cfg: Config,
    scripts_dir: PathBuf,
    python_exe: PathBuf,
    cancel: Option<CancellationToken>,
}

impl PythonEngine {
    pub fn new(cfg: &Config) -> Result<Self> {
        Self::new_with_cancel(cfg, None)
    }

    pub fn new_with_cancel(cfg: &Config, cancel: Option<CancellationToken>) -> Result<Self> {
        let scripts_dir = PathBuf::from(&cfg.paths.scripts_dir);
        if cfg.security.pin_scripts_dir {
            let canon = scripts_dir
                .canonicalize()
                .with_context(|| format!("canonicalize scripts_dir: {}", scripts_dir.display()))?;
            let root = workspace_root();
            if !canon.starts_with(&root) {
                return Err(anyhow!(
                    "scripts_dir is outside workspace root while pin_scripts_dir=true: {} (root: {})",
                    canon.display(),
                    root.display()
                ));
            }
        }
        for script in [
            "docling_runner.py",
            "pdf_probe.py",
            "pdf_split.py",
            "pdf_text.py",
        ] {
            let path = scripts_dir.join(script);
            if !path.exists() {
                return Err(anyhow!("missing script: {}", path.display()));
            }
        }
        let python_exe = resolve_python_exe(&cfg.docling.python_exe)?;
        Ok(Self {
            cfg: cfg.clone(),
            scripts_dir,
            python_exe,
            cancel,
        })
    }

    fn script(&self, name: &str) -> PathBuf {
        self.scripts_dir.join(name)
    }

    pub fn extract_pdf_page_texts(&self, input: &Path) -> Result<Vec<String>> {
        let script = self.script("pdf_text.py");
        let req = serde_json::json!({
            "cmd": "extract_page_texts",
            "input_pdf": input,
            "cfg": &self.cfg,
        });
        let out: PdfPageTextOut = self.run_json(&script, &req, Some(180), &[])?;
        if !out.ok {
            let message = out
                .error
                .unwrap_or_else(|| "extract_page_texts returned ok=false".to_string());
            return Err(anyhow!(message));
        }
        Ok(out.page_texts)
    }

    fn run_json<I: serde::Serialize, O: for<'de> serde::Deserialize<'de>>(
        &self,
        script: &Path,
        input: &I,
        timeout_seconds: Option<u64>,
        extra_env: &[(&str, &str)],
    ) -> Result<O> {
        debug!(
            "python run {} timeout={:?}",
            script.display(),
            timeout_seconds
        );
        let mut cmd = Command::new(&self.python_exe);
        cmd.arg(script);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        for (k, v) in &self.cfg.docling.env {
            cmd.env(k, v);
        }
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        if let Some(artifacts_dir) = resolve_artifacts_dir(&self.cfg) {
            cmd.env("DOCLING_ARTIFACTS_PATH", artifacts_dir);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawning python: {}", script.display()))?;

        {
            let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
            let bytes = serde_json::to_vec(input)?;
            use std::io::Write;
            stdin.write_all(&bytes)?;
            stdin.flush().ok();
        }

        let output = if let Some(secs) = timeout_seconds {
            wait_with_timeout(&mut child, Duration::from_secs(secs), self.cancel.as_ref())?
        } else {
            child
                .wait_with_output()
                .with_context(|| "waiting for python")?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "python script failed: {}\n{}",
                script.display(),
                stderr
            ));
        }

        if self.cfg.debug.keep_python_stderr && !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!("python stderr {}: {}", script.display(), stderr.trim());
        }

        let out: O = serde_json::from_slice(&output.stdout)
            .with_context(|| format!("parsing python JSON output: {}", script.display()))?;
        Ok(out)
    }
}

fn resolve_python_exe(raw: &str) -> Result<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("auto") {
        if let Ok(env_val) = std::env::var("DOCLING_PYTHON") {
            let p = expand_tilde(&env_val);
            if p.exists() {
                return Ok(p);
            }
        }
        let default_path = expand_tilde("~/Code/AI/docling/.venv/bin/python");
        if default_path.exists() {
            return Ok(default_path);
        }
        return Ok(PathBuf::from("python3"));
    }
    let p = expand_tilde(raw);
    Ok(p)
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest_dir.ancestors() {
        let candidate = ancestor.to_path_buf();
        if candidate.join("conf").exists() {
            return candidate;
        }
    }
    manifest_dir
}
fn resolve_artifacts_dir(cfg: &Config) -> Option<PathBuf> {
    if !cfg.paths.docling_artifacts_dir.is_empty() {
        return Some(PathBuf::from(&cfg.paths.docling_artifacts_dir));
    }
    None
}
impl Engine for PythonEngine {
    fn doctor(&self) -> Result<DocDiag> {
        let script = self.script("docling_runner.py");
        self.run_json::<serde_json::Value, DocDiag>(
            &script,
            &serde_json::json!({"cmd":"doctor"}),
            Some(self.cfg.docling.doctor_timeout_seconds),
            &[],
        )
    }

    fn probe_pdf(&self, input: &Path, sample_pages: u32) -> Result<ProbeOut> {
        let script = self.script("pdf_probe.py");
        let req = serde_json::json!({
            "input_pdf": input,
            "sample_pages": sample_pages,
        });
        let out: ProbeOut = self.run_json(&script, &req, Some(120), &[])?;
        if let Some(err) = out.error.as_deref() {
            return Err(anyhow!("pdf_probe error: {err}"));
        }
        Ok(out)
    }

    fn split_pdf(
        &self,
        input: &Path,
        out_dir: &Path,
        ranges: &[crate::quack_check::chunk_plan::PageRange],
    ) -> Result<Vec<SplitChunk>> {
        let script = self.script("pdf_split.py");
        let req = serde_json::json!({
            "input_pdf": input,
            "out_dir": out_dir,
            "chunks": ranges,
        });
        let out: SplitOut = self.run_json(&script, &req, Some(300), &[])?;
        if !out.ok {
            let msg = out.error.unwrap_or_else(|| "pdf_split failed".to_string());
            return Err(anyhow!(msg));
        }
        Ok(out.outputs)
    }

    fn convert_docling(&self, req: &ConvertIn) -> Result<ConvertOut> {
        let script = self.script("docling_runner.py");
        let timeout = if self.cfg.docling.chunk_timeout_seconds > 0 {
            Some(self.cfg.docling.chunk_timeout_seconds)
        } else {
            None
        };
        let out: ConvertOut = self.run_json(
            &script,
            &serde_json::json!({"cmd":"convert","req":req, "cfg": &self.cfg}),
            timeout,
            &[],
        )?;
        if !out.ok {
            warn!(
                "docling convert returned ok=false for chunk {}",
                req.chunk_index
            );
        }
        Ok(out)
    }

    fn convert_native_text(&self, req: &ConvertIn) -> Result<ConvertOut> {
        let script = self.script("pdf_text.py");
        let timeout = if self.cfg.docling.chunk_timeout_seconds > 0 {
            Some(self.cfg.docling.chunk_timeout_seconds)
        } else {
            None
        };
        let out: ConvertOut = self.run_json(
            &script,
            &serde_json::json!({"cmd":"convert","req":req, "cfg": &self.cfg}),
            timeout,
            &[],
        )?;
        if !out.ok {
            warn!(
                "native text convert returned ok=false for chunk {}",
                req.chunk_index
            );
        }
        Ok(out)
    }
}

fn wait_with_timeout(
    child: &mut Child,
    timeout: Duration,
    cancel: Option<&CancellationToken>,
) -> Result<Output> {
    // Drain pipes while waiting so verbose python logging can't deadlock the child
    // on a full stdout/stderr buffer.
    let stdout_reader = child.stdout.take();
    let stderr_reader = child.stderr.take();

    let stdout_thread = std::thread::spawn(move || -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        if let Some(mut out) = stdout_reader {
            out.read_to_end(&mut buf).with_context(|| "read stdout")?;
        }
        Ok(buf)
    });

    let stderr_thread = std::thread::spawn(move || -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        if let Some(mut err) = stderr_reader {
            err.read_to_end(&mut buf).with_context(|| "read stderr")?;
        }
        Ok(buf)
    });

    let start = Instant::now();
    loop {
        if let Some(token) = cancel
            && token.is_cancelled()
        {
            warn!("python process cancelled by token");
            let _ = child.kill();
            let status = child.wait().with_context(|| "wait after cancel kill")?;
            let stdout = stdout_thread
                .join()
                .map_err(|_| anyhow!("stdout reader thread panicked"))??;
            let stderr = stderr_thread
                .join()
                .map_err(|_| anyhow!("stderr reader thread panicked"))??;
            let output = Output {
                status,
                stdout,
                stderr,
            };
            return Err(anyhow!(
                "python process cancelled; stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        if let Some(status) = child.try_wait().with_context(|| "try_wait")? {
            let stdout = stdout_thread
                .join()
                .map_err(|_| anyhow!("stdout reader thread panicked"))??;
            let stderr = stderr_thread
                .join()
                .map_err(|_| anyhow!("stderr reader thread panicked"))??;
            return Ok(Output {
                status,
                stdout,
                stderr,
            });
        }

        if start.elapsed() > timeout {
            warn!("python process timed out after {:?}", timeout);
            let _ = child.kill();
            let status = child.wait().with_context(|| "wait after kill")?;
            let stdout = stdout_thread
                .join()
                .map_err(|_| anyhow!("stdout reader thread panicked"))??;
            let stderr = stderr_thread
                .join()
                .map_err(|_| anyhow!("stderr reader thread panicked"))??;
            let output = Output {
                status,
                stdout,
                stderr,
            };
            return Err(anyhow!(
                "python process exceeded timeout ({:?}); stderr: {}",
                timeout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}
