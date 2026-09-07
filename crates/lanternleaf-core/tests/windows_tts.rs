#![cfg(windows)]

use lanternleaf_core::tts::{
    enumerate_windows_voices, resolve_windows_voice_id, synthesize_windows_sentence_to_wav,
};
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
