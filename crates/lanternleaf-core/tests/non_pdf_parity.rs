use lanternleaf_core::{config::AppConfig, normalizer::TextNormalizer, session, text_utils};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/source-ingestion")
        .join(name)
}

fn temp_path(extension: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("lanternleaf-0006-{stamp}.{extension}"))
}

fn copy_to_temp_source(path: &Path) -> PathBuf {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("txt");
    let target = temp_path(extension);
    fs::copy(path, &target).expect("copy representative source to temporary test path");
    target
}

fn pandoc_available() -> bool {
    Command::new("pandoc").arg("--version").output().is_ok()
}

fn build_epub_fixture() -> PathBuf {
    let path = temp_path("epub");
    let entries = [
        ("mimetype", "application/epub+zip"),
        (
            "META-INF/container.xml",
            "<?xml version=\"1.0\"?><container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>",
        ),
        (
            "OEBPS/content.opf",
            "<?xml version=\"1.0\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"2.0\" unique-identifier=\"uid\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>LanternLeaf Fixture</dc:title><dc:language>en</dc:language><dc:identifier id=\"uid\">urn:lanternleaf:0006</dc:identifier></metadata><manifest><item id=\"c1\" href=\"chapter1.xhtml\" media-type=\"application/xhtml+xml\"/><item id=\"c2\" href=\"chapter2.xhtml\" media-type=\"application/xhtml+xml\"/></manifest><spine><itemref idref=\"c1\"/><itemref idref=\"c2\"/></spine></package>",
        ),
        (
            "OEBPS/chapter1.xhtml",
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1>Chapter One</h1><p>EPUB alpha appears here. The first chapter has an internal link to chapter two.</p><p>Unicode café remains readable.</p></body></html>",
        ),
        (
            "OEBPS/chapter2.xhtml",
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1>Chapter Two</h1><p>EPUB beta appears here. EPUB alpha appears again for search navigation.</p><p>The repeated search term appears again.</p><ul><li>Native list item.</li></ul></body></html>",
        ),
    ];
    let mut bytes = Vec::new();
    let mut central = Vec::new();
    let entry_count = entries.len();
    for (name, content) in entries.iter().copied() {
        let name = name.as_bytes();
        let data = content.as_bytes();
        let offset = bytes.len() as u32;
        let crc = crc32(data);
        bytes.extend_from_slice(&0x04034b50u32.to_le_bytes());
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
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
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let central_offset = bytes.len() as u32;
    bytes.extend_from_slice(&central);
    bytes.extend_from_slice(&0x06054b50u32.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
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

fn assert_session_contract(path: &Path, expected_kind: session::PrettyKind, config: &AppConfig) {
    let normalizer = TextNormalizer::load_default();
    let mut reader =
        session::load_session_for_source(path.to_path_buf(), config, &normalizer).unwrap();
    let initial = reader.snapshot(session::PanelState::default(), &normalizer);
    assert_eq!(initial.pretty_kind, expected_kind);
    assert!(!initial.canonical_sentences.is_empty());
    assert_eq!(
        initial.canonical_sentences.len(),
        text_utils::split_sentences(&initial.canonical_sentences.join(" ")).len()
    );
    assert_eq!(initial.sentence_anchor_map.len(), initial.sentences.len());
    assert_eq!(
        initial.page_sentence_counts.iter().sum::<usize>(),
        initial.canonical_sentences.len()
    );
    let canonical = initial.canonical_sentences.clone();
    let canonical_text = canonical.join(" ");
    match expected_kind {
        session::PrettyKind::Markdown => {
            for raw_syntax in [
                "# LanternLeaf",
                "**Emphasis**",
                "- The list",
                "](https",
                "![",
            ] {
                assert!(
                    !canonical_text.contains(raw_syntax),
                    "Markdown canonical text leaked source syntax: {raw_syntax:?}"
                );
            }
        }
        session::PrettyKind::Html => {
            for raw_noise in [
                "<html",
                "<p>",
                "</p>",
                "console.log",
                "this must not become speech",
                "body {",
                "href=",
                "src=",
                "xmlns=",
                "media-type=",
            ] {
                assert!(
                    !canonical_text.contains(raw_noise),
                    "HTML/EPUB canonical text leaked source noise: {raw_noise:?}"
                );
            }
        }
        session::PrettyKind::None | session::PrettyKind::Pdf => {}
    }
    reader.apply_command(
        session::SessionCommand::SearchSetQuery {
            query: "repeated".into(),
        },
        session::PanelState::default(),
        &normalizer,
    );
    let searched = reader.snapshot(session::PanelState::default(), &normalizer);
    assert!(!searched.search_matches.is_empty());
    assert_eq!(searched.selected_search_match, Some(0));
    let first_match = searched.search_matches[0];
    reader.apply_command(
        session::SessionCommand::SearchNext,
        session::PanelState::default(),
        &normalizer,
    );
    let next = reader.snapshot(session::PanelState::default(), &normalizer);
    assert!(next.search_matches.len() >= 2);
    assert_eq!(next.selected_search_match, Some(1));
    assert_eq!(next.highlighted_sentence_idx, Some(next.search_matches[1]));
    assert_ne!(next.search_matches[1], first_match);
    reader.apply_command(
        session::SessionCommand::SearchPrev,
        session::PanelState::default(),
        &normalizer,
    );
    let previous = reader.snapshot(session::PanelState::default(), &normalizer);
    assert_eq!(previous.selected_search_match, Some(0));
    assert_eq!(
        previous.highlighted_sentence_idx,
        Some(previous.search_matches[0])
    );
    reader.apply_command(
        session::SessionCommand::SentenceClick { sentence_idx: 0 },
        session::PanelState::default(),
        &normalizer,
    );
    reader.apply_command(
        session::SessionCommand::TtsPlayFromHighlight,
        session::PanelState::default(),
        &normalizer,
    );
    let playing = reader.snapshot(session::PanelState::default(), &normalizer);
    assert_eq!(playing.canonical_sentences, canonical);
    assert!(playing.tts.current_sentence_idx.is_some());
    reader.apply_command(
        session::SessionCommand::ToggleTextOnly,
        session::PanelState::default(),
        &normalizer,
    );
    let toggled = reader.snapshot(session::PanelState::default(), &normalizer);
    assert_eq!(toggled.canonical_sentences, canonical);
    assert_eq!(
        toggled.tts.current_sentence_idx,
        playing.tts.current_sentence_idx
    );
    let bookmark = reader.to_bookmark();
    session::persist_session_housekeeping(&reader);
    let reopened =
        session::load_session_for_source(path.to_path_buf(), config, &normalizer).unwrap();
    assert_eq!(reopened.to_bookmark().sentence_idx, bookmark.sentence_idx);
    assert!(lanternleaf_core::cache::delete_recent_source_and_cache(path).is_ok());
    assert!(lanternleaf_core::cache::delete_recent_source_and_cache(path).is_ok());
}

#[test]
fn representative_non_pdf_sources_preserve_session_and_canonical_contracts() {
    let mut config = AppConfig::default();
    config.lines_per_page = 8;
    let txt = fixture("representative.txt");
    let md = fixture("representative.md");
    let html = fixture("representative.html");
    let epub = build_epub_fixture();
    let cases = [
        (copy_to_temp_source(&txt), session::PrettyKind::None),
        (copy_to_temp_source(&md), session::PrettyKind::Markdown),
    ];
    for (path, kind) in cases {
        assert_session_contract(&path, kind, &config);
    }
    if pandoc_available() {
        let html = copy_to_temp_source(&html);
        assert_session_contract(&html, session::PrettyKind::Html, &config);
        assert_session_contract(&epub, session::PrettyKind::Html, &config);
    } else {
        eprintln!("skipping HTML/EPUB session assertions because Pandoc is unavailable");
    }
    let _ = fs::remove_file(epub);
}

#[test]
fn representative_epub_builder_is_deterministic_and_loadable() {
    let first = build_epub_fixture();
    let second = build_epub_fixture();
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    epub::doc::EpubDoc::new(&first).expect("deterministic EPUB should be structurally loadable");
    if pandoc_available() {
        let loaded = lanternleaf_core::epub_loader::load_book_content(&first).unwrap();
        assert!(loaded.tts_text.contains("EPUB alpha"));
        assert!(
            loaded
                .reading_html
                .as_deref()
                .unwrap_or_default()
                .contains("data-ll-epub-chapter=\"1\"")
        );
    } else {
        eprintln!("skipping EPUB load assertion because Pandoc is unavailable");
    }
    let _ = fs::remove_file(first);
    let _ = fs::remove_file(second);
}
