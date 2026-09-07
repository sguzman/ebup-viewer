#![cfg(windows)]

use lanternleaf_core::tts::{
    enumerate_windows_voices, resolve_windows_voice_id, synthesize_windows_sentence_to_wav,
};
use lanternleaf_core::{config::AppConfig, normalizer::TextNormalizer, session};
use rodio::{Decoder, Source};
use std::fs;
use std::fs::File;
use std::io::BufReader;

#[test]
fn windows_voice_catalog_and_sentence_wav_probe() {
    let voices = enumerate_windows_voices().expect("enumerating Windows voices");
    println!("windows_tts_voice_count={}", voices.len());
    let Some(_voice) = voices.first() else {
        println!("windows_tts_synthesis=skipped_no_usable_voice");
        return;
    };
    let default_voice_id = resolve_windows_voice_id(None).expect("resolve Windows default voice");
    assert!(!default_voice_id.is_empty());
    println!("windows_tts_default_voice_id={default_voice_id}");
    assert!(
        resolve_windows_voice_id(Some("lanternleaf-missing-voice"))
            .expect_err("missing Windows voice should be actionable")
            .to_string()
            .contains("not found")
    );
    let path = std::env::temp_dir().join(format!(
        "lanternleaf-windows-tts-{}.wav",
        std::process::id()
    ));
    let result = synthesize_windows_sentence_to_wav(
        "LanternLeaf Windows speech synthesis probe.",
        &path,
        Some(&default_voice_id),
    );
    result.expect("synthesizing a sentence with Windows TTS");
    let metadata = fs::metadata(&path).expect("Windows TTS probe should create a WAV");
    assert!(
        metadata.len() > 44,
        "Windows TTS probe produced an empty WAV"
    );
    let decoder = Decoder::new(BufReader::new(
        File::open(&path).expect("open synthesized WAV"),
    ))
    .expect("shared Rodio decoder should open synthesized Windows WAV");
    assert!(decoder.channels() > 0);
    assert!(decoder.sample_rate() > 0);
    assert!(
        decoder
            .total_duration()
            .is_some_and(|duration| !duration.is_zero())
    );
    println!("windows_tts_synthesis=synthesized voice_id={default_voice_id}");
    let _ = fs::remove_file(path);
}

#[test]
fn windows_source_session_sentence_synthesis_probe() {
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/source-ingestion/representative.txt");
    let normalizer = TextNormalizer::load_default();
    let mut reader = session::load_session_for_source(fixture, &AppConfig::default(), &normalizer)
        .expect("representative TXT should load before Windows synthesis");
    let snapshot = reader.snapshot(session::PanelState::default(), &normalizer);
    let sentence = snapshot
        .canonical_sentences
        .first()
        .expect("representative TXT should expose a canonical sentence");
    let voice_id = resolve_windows_voice_id(None).expect("resolve Windows default voice");
    let path = std::env::temp_dir().join(format!(
        "lanternleaf-windows-source-session-{}.wav",
        std::process::id()
    ));
    synthesize_windows_sentence_to_wav(sentence, &path, Some(&voice_id))
        .expect("synthesize the canonical source sentence through Windows TTS");
    let decoder = Decoder::new(BufReader::new(
        File::open(&path).expect("open source/session Windows WAV"),
    ))
    .expect("decode source/session Windows WAV through shared Rodio decoder");
    assert!(decoder.channels() > 0);
    assert!(decoder.sample_rate() > 0);
    assert!(
        decoder
            .total_duration()
            .is_some_and(|duration| !duration.is_zero())
    );
    println!("windows_tts_source_session_synthesis=synthesized");
    let _ = fs::remove_file(path);
}
