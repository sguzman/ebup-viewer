use lanternleaf_app::tts_runtime::{TtsCommand, TtsRuntime, TtsRuntimeEventKind, TtsRuntimeMode};
use lanternleaf_core::{
    config::{self, AppConfig},
    normalizer::TextNormalizer,
    session,
};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/source-ingestion")
        .join(name)
}

fn pandoc_available() -> bool {
    Command::new("pandoc").arg("--version").output().is_ok()
}

fn build_epub_fixture() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("lanternleaf-0006-runtime-{stamp}.epub"));
    let entries = [
        ("mimetype", "application/epub+zip"),
        (
            "META-INF/container.xml",
            "<container><rootfiles><rootfile full-path=\"OEBPS/content.opf\"/></rootfiles></container>",
        ),
        (
            "OEBPS/content.opf",
            "<package><manifest><item id=\"c1\" href=\"chapter1.xhtml\" media-type=\"application/xhtml+xml\"/><item id=\"c2\" href=\"chapter2.xhtml\" media-type=\"application/xhtml+xml\"/></manifest><spine><itemref idref=\"c1\"/><itemref idref=\"c2\"/></spine></package>",
        ),
        (
            "OEBPS/chapter1.xhtml",
            "<html><body><h1>Chapter One</h1><p>EPUB alpha appears here. The first chapter is readable.</p></body></html>",
        ),
        (
            "OEBPS/chapter2.xhtml",
            "<html><body><h1>Chapter Two</h1><p>EPUB beta appears here. The repeated search term appears again.</p></body></html>",
        ),
    ];
    let entry_count = entries.len();
    let mut bytes = Vec::new();
    let mut central = Vec::new();
    for (name, content) in entries {
        let name = name.as_bytes();
        let data = content.as_bytes();
        let offset = bytes.len() as u32;
        let crc = crc32(data);
        bytes.extend_from_slice(&0x04034b50u32.to_le_bytes());
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 8]);
        bytes.extend_from_slice(&crc.to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(data);
        central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&[0u8; 8]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0u8; 8]);
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let central_offset = bytes.len() as u32;
    bytes.extend_from_slice(&central);
    bytes.extend_from_slice(&0x06054b50u32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 4]);
    bytes.extend_from_slice(&(entry_count as u16).to_le_bytes());
    bytes.extend_from_slice(&(entry_count as u16).to_le_bytes());
    bytes.extend_from_slice(&(central.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&central_offset.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    fs::write(&path, bytes).unwrap();
    path
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
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

fn wait_for_cancellation(
    runtime: &TtsRuntime,
) -> Vec<lanternleaf_app::tts_runtime::TtsRuntimeEvent> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut events = Vec::new();
    while Instant::now() < deadline {
        events.extend(runtime.collect_events());
        if events
            .iter()
            .any(|event| event.kind == TtsRuntimeEventKind::Cancelled)
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    events
}

#[test]
fn simulated_tts_uses_real_non_pdf_sessions() {
    let normalizer = TextNormalizer::load_default();
    let mut config = AppConfig::default();
    config.lines_per_page = 8;
    let epub = build_epub_fixture();
    let mut cases = vec![
        (fixture("representative.txt"), false),
        (fixture("representative.md"), false),
    ];
    if pandoc_available() {
        cases.push((fixture("representative.html"), true));
        cases.push((epub.clone(), true));
    } else {
        eprintln!("skipping HTML/EPUB simulated runtime assertions because Pandoc is unavailable");
    }
    for (path, _) in cases {
        let reader = session::load_session_for_source(path, &config, &normalizer).unwrap();
        let runtime = TtsRuntime::new_with_mode(normalizer.clone(), TtsRuntimeMode::Simulated);
        runtime.set_session(Some(reader));
        let before = runtime.snapshot().unwrap();
        assert!(!before.canonical_sentences.is_empty());
        let page_start = runtime
            .apply_command(TtsCommand::PlayFromPageStart)
            .unwrap();
        assert!(page_start.tts.current_sentence_idx.is_some());
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

        let highlight = runtime
            .apply_command(TtsCommand::PlayFromHighlight)
            .unwrap();
        assert!(highlight.tts.current_sentence_idx.is_some());
        let next = runtime.apply_command(TtsCommand::SeekNext).unwrap();
        assert!(
            next.tts
                .current_sentence_idx
                .is_none_or(|idx| idx < before.canonical_sentences.len())
        );
        let previous = runtime.apply_command(TtsCommand::SeekPrev).unwrap();
        assert!(
            previous
                .tts
                .current_sentence_idx
                .is_none_or(|idx| idx < before.canonical_sentences.len())
        );
        let repeated = runtime.apply_command(TtsCommand::RepeatSentence).unwrap();
        assert!(
            repeated
                .tts
                .current_sentence_idx
                .is_none_or(|idx| idx < before.canonical_sentences.len())
        );

        let before_backend_switch = runtime.snapshot().unwrap();
        runtime.apply_command(TtsCommand::ApplySettings {
            patch: session::ReaderSettingsPatch {
                tts_backend: Some(config::TtsBackend::Windows),
                ..Default::default()
            },
        });
        let after_backend_switch = runtime.snapshot().unwrap();
        assert_eq!(
            after_backend_switch.tts.current_sentence_idx,
            before_backend_switch.tts.current_sentence_idx
        );
        runtime.apply_command(TtsCommand::PlayFromHighlight);
        runtime.apply_command(TtsCommand::Stop);
        let cancelled = wait_for_cancellation(&runtime);
        assert!(
            cancelled
                .iter()
                .any(|event| event.kind == TtsRuntimeEventKind::Cancelled)
        );

        runtime.apply_command(TtsCommand::PlayFromPageStart);
        let replacement = wait_for_progress(&runtime);
        assert!(replacement.iter().any(|event| {
            event
                .tts
                .as_ref()
                .and_then(|tts| tts.current_sentence_idx)
                .is_some_and(|idx| idx < before.canonical_sentences.len())
        }));
    }
    let _ = fs::remove_file(epub);
}
