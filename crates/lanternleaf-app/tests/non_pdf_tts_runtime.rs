use lanternleaf_app::tts_runtime::{TtsCommand, TtsRuntime, TtsRuntimeEventKind, TtsRuntimeMode};
use lanternleaf_core::{config::AppConfig, normalizer::TextNormalizer, session};
use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/source-ingestion")
        .join(name)
}

fn wait_for_progress(runtime: &TtsRuntime) -> Vec<lanternleaf_app::tts_runtime::TtsRuntimeEvent> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut events = Vec::new();
    while Instant::now() < deadline {
        events.extend(runtime.collect_events());
        if events
            .iter()
            .any(|event| event.kind == TtsRuntimeEventKind::Progress)
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    events
}

#[test]
fn simulated_tts_uses_real_txt_and_markdown_sessions() {
    let normalizer = TextNormalizer::load_default();
    let mut config = AppConfig::default();
    config.lines_per_page = 8;
    for name in ["representative.txt", "representative.md"] {
        let reader = session::load_session_for_source(fixture(name), &config, &normalizer).unwrap();
        let runtime = TtsRuntime::new_with_mode(normalizer.clone(), TtsRuntimeMode::Simulated);
        runtime.set_session(Some(reader));
        let before = runtime.snapshot().unwrap();
        assert!(!before.canonical_sentences.is_empty());
        runtime.apply_command(TtsCommand::PlayFromPageStart);
        let events = wait_for_progress(&runtime);
        assert!(
            events
                .iter()
                .any(|event| event.kind == TtsRuntimeEventKind::Progress)
        );
        assert!(
            events
                .iter()
                .filter_map(|event| event.tts.as_ref().and_then(|tts| tts.current_sentence_idx))
                .all(|idx| idx < before.canonical_sentences.len())
        );

        runtime.apply_command(TtsCommand::PlayFromHighlight);
        runtime.apply_command(TtsCommand::Stop);
        thread::sleep(Duration::from_millis(20));
        let cancelled = runtime.collect_events();
        assert!(
            cancelled
                .iter()
                .any(|event| event.kind == TtsRuntimeEventKind::Cancelled)
        );
    }
}
