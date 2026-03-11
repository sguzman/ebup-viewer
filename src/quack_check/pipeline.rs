use crate::cancellation::CancellationToken;
use crate::quack_check::{
    chunk_plan::ChunkPlan,
    config::Config,
    engine::{ConvertIn, Engine},
    policy, postprocess, probe,
    report::{ChunkReport, JobReport},
    util::ensure_dir,
};
use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, info, warn};

pub struct Pipeline<E: Engine> {
    cfg: Config,
    engine: E,
    cancel: Option<CancellationToken>,
}

pub struct JobOutput {
    pub markdown: String,
    pub text: String,
    pub report: JobReport,
}

impl<E: Engine> Pipeline<E> {
    pub fn new(cfg: &Config, engine: E) -> Self {
        Self::new_with_cancel(cfg, engine, None)
    }

    pub fn new_with_cancel(cfg: &Config, engine: E, cancel: Option<CancellationToken>) -> Self {
        Self {
            cfg: cfg.clone(),
            engine,
            cancel,
        }
    }

    pub fn run_job(&self, input: &Path, job_dir: &Path) -> Result<JobOutput> {
        let started = Instant::now();
        self.ensure_not_cancelled("job_start")?;

        let probe_res = probe::probe_pdf(&self.cfg, &self.engine, input)?;
        self.ensure_not_cancelled("after_probe")?;
        let decision = policy::decide(&self.cfg, &probe_res);
        let mut plan = ChunkPlan::from_probe(&self.cfg, &probe_res)?;

        info!(
            "probe page_count={} file_bytes={} avg_chars={} garbage_ratio={} whitespace_ratio={} text_page_ratio={} sparse_text_page_ratio={} repeated_header_ratio={} repeated_footer_ratio={} image_page_ratio={} mixed_text_image_page_ratio={} full_page_raster_page_ratio={} hidden_text_layer_page_ratio={} duplicate_text_page_ratio={}",
            probe_res.input.page_count,
            probe_res.input.file_bytes,
            probe_res.sample.avg_chars_per_page,
            probe_res.sample.garbage_ratio,
            probe_res.sample.whitespace_ratio,
            probe_res.sample.text_page_ratio,
            probe_res.sample.sparse_text_page_ratio,
            probe_res.sample.repeated_header_ratio,
            probe_res.sample.repeated_footer_ratio,
            probe_res.sample.image_page_ratio,
            probe_res.sample.mixed_text_image_page_ratio,
            probe_res.sample.full_page_raster_page_ratio,
            probe_res.sample.hidden_text_layer_page_ratio,
            probe_res.sample.duplicate_text_page_ratio
        );
        info!(
            "policy tier={:?} engine={} do_ocr={}",
            decision.tier, decision.chosen_engine, decision.do_ocr
        );
        debug!(?plan, "chunk plan");

        if decision.chosen_engine == "native_text" && self.cfg.native_text.backend != "python_pypdf"
        {
            return Err(anyhow!(
                "unsupported native_text.backend: {}",
                self.cfg.native_text.backend
            ));
        }

        let require_chunking = probe_res.input.page_count
            > self.cfg.limits.require_chunking_over_pages
            || probe_res.input.file_bytes > self.cfg.limits.require_chunking_over_bytes;

        if !require_chunking && plan.chunks.len() > 1 {
            plan = ChunkPlan::single(plan.page_count, &self.cfg.chunking.strategy);
        }

        if self.cfg.global.max_parallel_chunks > 1 {
            warn!(
                "max_parallel_chunks > 1 is configured, but pipeline runs sequentially in this build"
            );
        }

        let chunks_dir = job_dir.join("chunks");
        ensure_dir(&chunks_dir)?;
        self.ensure_not_cancelled("before_prepare_chunks")?;

        let chunk_inputs = match self.prepare_chunks(input, &plan, &chunks_dir) {
            Ok(inputs) => inputs,
            Err(err) => {
                if self.cfg.chunking.strategy == "physical_split" {
                    warn!("physical split failed; falling back to page_range: {err}");
                    let mut fallback = plan.clone();
                    fallback.strategy = "page_range".to_string();
                    self.ensure_not_cancelled("before_prepare_chunks_fallback")?;
                    self.prepare_chunks(input, &fallback, &chunks_dir)?
                } else {
                    return Err(err);
                }
            }
        };

        let mut chunk_reports = Vec::new();
        let mut markdown_parts = Vec::new();

        for (i, ch) in chunk_inputs.iter().enumerate() {
            self.ensure_not_cancelled("before_chunk_convert")?;
            if self.cfg.limits.job_timeout_seconds > 0
                && started.elapsed().as_secs() > self.cfg.limits.job_timeout_seconds
            {
                return Err(anyhow!(
                    "job timeout exceeded: {}s",
                    self.cfg.limits.job_timeout_seconds
                ));
            }

            info!(
                "chunk {} pages {}-{} input={}",
                i,
                ch.start_page,
                ch.end_page,
                ch.input_pdf.display()
            );

            let req = ConvertIn {
                input_pdf: ch.input_pdf.display().to_string(),
                out_dir: chunks_dir.display().to_string(),
                chunk_index: i as u32,
                start_page: ch.start_page,
                end_page: ch.end_page,
                do_ocr: decision.do_ocr,
                pdf_backend: self.cfg.docling.backend.pdf_backend.clone(),
                use_page_range: ch.use_page_range,
            };

            let mut used_fallback = false;
            let mut out = match decision.chosen_engine.as_str() {
                "docling" => self.engine.convert_docling(&req),
                "native_text" => self.engine.convert_native_text(&req),
                other => Err(anyhow!("unknown engine: {other}")),
            };

            if matches!(decision.chosen_engine.as_str(), "native_text") {
                let fallback_reason = match &out {
                    Ok(o) if o.ok => None,
                    Ok(o) => Some(format!(
                        "native_text returned ok=false warnings={:?}",
                        o.warnings
                    )),
                    Err(e) => Some(format!("native_text error: {e}")),
                };

                if let Some(reason) = fallback_reason {
                    warn!(
                        "native_text failed for chunk {}; falling back to docling: {}",
                        i, reason
                    );
                    out = self.engine.convert_docling(&req);
                    used_fallback = true;
                }
            }

            let mut out = out.with_context(|| {
                format!(
                    "convert failed for chunk {} pages {}-{}",
                    i, ch.start_page, ch.end_page
                )
            })?;

            if !out.ok {
                return Err(anyhow!("chunk {} failed; warnings={:?}", i, out.warnings));
            }

            if used_fallback {
                out.warnings
                    .push("native_text failed; fell back to docling".to_string());
            }

            if self.cfg.output.write_chunk_json {
                let chunk_json_path = chunks_dir.join(format!("chunk_{:05}.json", i));
                std::fs::write(&chunk_json_path, serde_json::to_string_pretty(&out)?)?;
            }

            chunk_reports.push(ChunkReport {
                chunk_index: i as u32,
                start_page: ch.start_page,
                end_page: ch.end_page,
                ok: out.ok,
                warnings: out.warnings.clone(),
                meta: out.meta.clone(),
            });

            markdown_parts.push(out.markdown);
            self.ensure_not_cancelled("after_chunk_convert")?;
        }

        self.ensure_not_cancelled("before_merge")?;
        let merged_md = postprocess::merge_markdown(&self.cfg, markdown_parts)?;
        let merged_txt = postprocess::markdown_to_text(&self.cfg, &merged_md)?;

        if !self.cfg.global.keep_intermediates {
            self.cleanup_intermediates(&chunk_inputs)?;
        }

        let report = JobReport {
            input: probe_res.input,
            sample: probe_res.sample,
            decision,
            chunk_reports,
        };

        Ok(JobOutput {
            markdown: merged_md,
            text: merged_txt,
            report,
        })
    }

    fn ensure_not_cancelled(&self, stage: &'static str) -> Result<()> {
        if let Some(token) = &self.cancel {
            token.check_cancelled(stage)?;
        }
        Ok(())
    }

    fn prepare_chunks(
        &self,
        input: &Path,
        plan: &ChunkPlan,
        chunks_dir: &Path,
    ) -> Result<Vec<ChunkInput>> {
        // Use the plan's strategy so callers can switch strategies for fallback.
        let strategy = plan.strategy.as_str();
        if strategy == "physical_split" && plan.chunks.len() > 1 {
            let split_outputs = self.engine.split_pdf(input, chunks_dir, &plan.chunks)?;
            let mut out = Vec::new();
            for c in split_outputs {
                let path = PathBuf::from(c.path);
                if self.cfg.chunking.cap_chunk_bytes && self.cfg.chunking.max_chunk_bytes > 0 {
                    if let Ok(meta) = std::fs::metadata(&path) {
                        if meta.len() > self.cfg.chunking.max_chunk_bytes {
                            warn!(
                                "chunk {} exceeds max_chunk_bytes ({} > {})",
                                c.chunk_index,
                                meta.len(),
                                self.cfg.chunking.max_chunk_bytes
                            );
                        }
                    }
                }
                out.push(ChunkInput {
                    input_pdf: path,
                    start_page: c.start_page,
                    end_page: c.end_page,
                    use_page_range: false,
                    temp_file: true,
                });
            }
            return Ok(out);
        }

        let use_page_range = strategy == "page_range" && plan.chunks.len() > 1;
        Ok(plan
            .chunks
            .iter()
            .map(|r| ChunkInput {
                input_pdf: input.to_path_buf(),
                start_page: r.start_page,
                end_page: r.end_page,
                use_page_range,
                temp_file: false,
            })
            .collect())
    }

    fn cleanup_intermediates(&self, chunks: &[ChunkInput]) -> Result<()> {
        if self.cfg.chunking.keep_split_pdfs {
            return Ok(());
        }
        for ch in chunks {
            if ch.temp_file {
                let _ = std::fs::remove_file(&ch.input_pdf);
            }
        }
        Ok(())
    }
}

struct ChunkInput {
    input_pdf: PathBuf,
    start_page: u32,
    end_page: u32,
    use_page_range: bool,
    temp_file: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quack_check::engine::{ConvertOut, DocDiag, ProbeOut, SplitChunk};
    use anyhow::Result;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Clone, Copy)]
    enum NativeMode {
        Success,
        OkFalse,
    }

    #[derive(Clone)]
    struct MockEngine {
        probe: ProbeOut,
        fail_split: bool,
        native_mode: NativeMode,
        split_calls: Arc<Mutex<usize>>,
        native_requests: Arc<Mutex<Vec<ConvertIn>>>,
        docling_requests: Arc<Mutex<Vec<ConvertIn>>>,
    }

    impl MockEngine {
        fn new(probe: ProbeOut) -> Self {
            Self {
                probe,
                fail_split: false,
                native_mode: NativeMode::Success,
                split_calls: Arc::new(Mutex::new(0)),
                native_requests: Arc::new(Mutex::new(Vec::new())),
                docling_requests: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn split_call_count(&self) -> usize {
            *self
                .split_calls
                .lock()
                .expect("split_calls lock should be available")
        }

        fn native_requests(&self) -> Vec<ConvertIn> {
            self.native_requests
                .lock()
                .expect("native_requests lock should be available")
                .clone()
        }

        fn docling_requests(&self) -> Vec<ConvertIn> {
            self.docling_requests
                .lock()
                .expect("docling_requests lock should be available")
                .clone()
        }
    }

    impl Engine for MockEngine {
        fn doctor(&self) -> Result<DocDiag> {
            Ok(DocDiag {
                python_exe: "python".to_string(),
                python_version: "3.12".to_string(),
                docling_version: Some("test".to_string()),
                ok: true,
                error: None,
            })
        }

        fn probe_pdf(&self, _input: &Path, _sample_pages: u32) -> Result<ProbeOut> {
            Ok(self.probe.clone())
        }

        fn split_pdf(
            &self,
            _input: &Path,
            _out_dir: &Path,
            ranges: &[crate::quack_check::chunk_plan::PageRange],
        ) -> Result<Vec<SplitChunk>> {
            if let Ok(mut calls) = self.split_calls.lock() {
                *calls += 1;
            }
            if self.fail_split {
                anyhow::bail!("simulated split failure");
            }
            Ok(ranges
                .iter()
                .enumerate()
                .map(|(i, range)| SplitChunk {
                    chunk_index: i as u32,
                    start_page: range.start_page,
                    end_page: range.end_page,
                    path: format!("/tmp/mock-split-{i}.pdf"),
                })
                .collect())
        }

        fn convert_docling(&self, req: &ConvertIn) -> Result<ConvertOut> {
            if let Ok(mut requests) = self.docling_requests.lock() {
                requests.push(req.clone());
            }
            Ok(ConvertOut {
                ok: true,
                markdown: format!(
                    "docling chunk {} [{}-{}]",
                    req.chunk_index, req.start_page, req.end_page
                ),
                warnings: vec![],
                meta: json!({ "engine": "docling" }),
            })
        }

        fn convert_native_text(&self, req: &ConvertIn) -> Result<ConvertOut> {
            if let Ok(mut requests) = self.native_requests.lock() {
                requests.push(req.clone());
            }
            match self.native_mode {
                NativeMode::Success => Ok(ConvertOut {
                    ok: true,
                    markdown: format!(
                        "native chunk {} [{}-{}]",
                        req.chunk_index, req.start_page, req.end_page
                    ),
                    warnings: vec![],
                    meta: json!({ "engine": "native_text" }),
                }),
                NativeMode::OkFalse => Ok(ConvertOut {
                    ok: false,
                    markdown: String::new(),
                    warnings: vec!["simulated native_text failure".to_string()],
                    meta: json!({ "engine": "native_text" }),
                }),
            }
        }
    }

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("lanternleaf_quack_check_{prefix}_{now}"))
    }

    fn create_dummy_pdf_file() -> PathBuf {
        let path = unique_temp_path("input").with_extension("pdf");
        std::fs::write(&path, b"%PDF-1.4\n% dummy test payload\n")
            .expect("dummy pdf should be written");
        path
    }

    fn high_text_probe(page_count: u32) -> ProbeOut {
        ProbeOut {
            page_count,
            sampled_pages: page_count.min(12),
            avg_chars_per_page: 2500,
            garbage_ratio: 0.0,
            whitespace_ratio: 0.15,
            text_page_ratio: 1.0,
            empty_text_page_ratio: 0.0,
            sparse_text_page_ratio: 0.0,
            noisy_text_page_ratio: 0.0,
            repeated_header_ratio: 0.0,
            repeated_footer_ratio: 0.0,
            image_page_ratio: 0.0,
            mixed_text_image_page_ratio: 0.0,
            full_page_raster_page_ratio: 0.0,
            hidden_text_layer_page_ratio: 0.0,
            duplicate_text_page_ratio: 0.0,
            pages: Vec::new(),
            error: None,
        }
    }

    #[test]
    fn run_job_falls_back_to_docling_when_native_text_returns_ok_false() {
        let mut cfg = Config::default();
        cfg.classification.forced_tier = "HIGH_TEXT".to_string();
        cfg.output.write_chunk_json = false;

        let mut engine = MockEngine::new(high_text_probe(1));
        engine.native_mode = NativeMode::OkFalse;
        let pipeline = Pipeline::new(&cfg, engine.clone());

        let input = create_dummy_pdf_file();
        let job_dir = unique_temp_path("job_docling_fallback");
        ensure_dir(&job_dir).expect("job dir should exist");

        let result = pipeline
            .run_job(&input, &job_dir)
            .expect("pipeline should recover with docling fallback");

        assert!(
            result
                .report
                .chunk_reports
                .iter()
                .flat_map(|chunk| chunk.warnings.iter())
                .any(|warning| warning.contains("fell back to docling")),
            "report should include fallback warning"
        );
        assert_eq!(engine.native_requests().len(), 1);
        assert_eq!(engine.docling_requests().len(), 1);
        assert!(result.markdown.contains("docling chunk"));

        let _ = std::fs::remove_file(input);
        let _ = std::fs::remove_dir_all(job_dir);
    }

    #[test]
    fn run_job_falls_back_to_page_range_when_physical_split_fails() {
        let mut cfg = Config::default();
        cfg.classification.forced_tier = "HIGH_TEXT".to_string();
        cfg.chunking.strategy = "physical_split".to_string();
        cfg.chunking.target_pages_per_chunk = 1;
        cfg.chunking.max_pages_per_chunk = 1;
        cfg.chunking.min_pages_per_chunk = 1;
        cfg.limits.require_chunking_over_pages = 0;
        cfg.output.write_chunk_json = false;

        let mut engine = MockEngine::new(high_text_probe(3));
        engine.fail_split = true;
        let pipeline = Pipeline::new(&cfg, engine.clone());

        let input = create_dummy_pdf_file();
        let job_dir = unique_temp_path("job_page_range_fallback");
        ensure_dir(&job_dir).expect("job dir should exist");

        let result = pipeline
            .run_job(&input, &job_dir)
            .expect("pipeline should recover with page_range fallback");

        assert_eq!(engine.split_call_count(), 1);
        assert_eq!(result.report.chunk_reports.len(), 3);
        let native_requests = engine.native_requests();
        assert_eq!(native_requests.len(), 3);
        assert!(
            native_requests.iter().all(|request| request.use_page_range),
            "fallback conversions should use page-range mode"
        );

        let _ = std::fs::remove_file(input);
        let _ = std::fs::remove_dir_all(job_dir);
    }
}
