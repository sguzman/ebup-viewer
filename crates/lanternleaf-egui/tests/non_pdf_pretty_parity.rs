use lanternleaf_core::{config::PrettyUiConfig, session::ReaderImageRef};
use lanternleaf_egui::pretty::{PrettyBlockKind, html_to_blocks, markdown_to_blocks};
use std::{fs, path::PathBuf};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/source-ingestion")
        .join(name);
    fs::read_to_string(path).expect("representative fixture should be readable")
}

fn images() -> Vec<ReaderImageRef> {
    vec![ReaderImageRef {
        raw_path: "fixture.png".to_string(),
        local_path: "fixture.png".to_string(),
    }]
}

fn assert_stable_bounded_anchors(blocks: &[lanternleaf_egui::pretty::PrettyBlock]) {
    let anchors: Vec<usize> = blocks.iter().map(|block| block.anchor_idx).collect();
    assert!(!anchors.is_empty());
    assert!(anchors.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(anchors, (0..blocks.len()).collect::<Vec<_>>());
}

#[test]
fn representative_markdown_pretty_projection_exposes_supported_structures() {
    let blocks = markdown_to_blocks(
        &fixture("representative.md"),
        &images(),
        PrettyUiConfig::default(),
    );
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::Heading { .. }))
    );
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::Paragraph))
    );
    assert!(blocks.iter().any(|block| {
        block
            .spans
            .iter()
            .any(|span| span.style.italics || span.style.bold)
    }));
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::ListItem { .. }))
    );
    assert!(
        blocks
            .iter()
            .any(|block| { block.spans.iter().any(|span| span.style.is_link) })
    );
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::Image))
    );
    assert_stable_bounded_anchors(&blocks);
}

#[test]
fn representative_html_pretty_projection_exposes_supported_structures() {
    let blocks = html_to_blocks(
        &fixture("representative.html"),
        &images(),
        PrettyUiConfig::default(),
    );
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::Heading { .. }))
    );
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::Paragraph))
    );
    assert!(blocks.iter().any(|block| {
        block
            .spans
            .iter()
            .any(|span| span.style.italics || span.style.bold)
    }));
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::ListItem { .. }))
    );
    assert!(
        blocks
            .iter()
            .any(|block| { block.spans.iter().any(|span| span.style.is_link) })
    );
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::Image))
    );
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::Table))
    );
    assert_stable_bounded_anchors(&blocks);
}

#[test]
fn deterministic_epub_chapter_html_uses_the_same_native_projection() {
    let chapters = r##"
        <h1>Chapter One</h1>
        <p>EPUB alpha appears here with <em>emphasis</em> and an <a href="#chapter-two">internal link</a>.</p>
        <h1 id="chapter-two">Chapter Two</h1>
        <p>EPUB beta appears here.</p>
        <ul><li>The chapter list is readable.</li></ul>
        <table><tr><th>Kind</th><th>Value</th></tr><tr><td>Fixture</td><td>Deterministic</td></tr></table>
        <img src="fixture.png" alt="A tiny fixture image" />
    "##;
    let blocks = html_to_blocks(chapters, &images(), PrettyUiConfig::default());
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::Heading { .. }))
    );
    assert!(blocks.iter().any(|block| {
        block
            .spans
            .iter()
            .any(|span| span.style.italics || span.style.is_link)
    }));
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::ListItem { .. }))
    );
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::Table))
    );
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block.kind, PrettyBlockKind::Image))
    );
    assert_stable_bounded_anchors(&blocks);
}
