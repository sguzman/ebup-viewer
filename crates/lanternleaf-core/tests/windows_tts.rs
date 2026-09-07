#![cfg(windows)]

use lanternleaf_core::tts::{enumerate_windows_voices, synthesize_windows_sentence_to_wav};
use std::fs;

#[test]
fn windows_voice_catalog_and_sentence_wav_probe() {
    let voices = enumerate_windows_voices().expect("enumerating Windows voices");
    println!("windows_tts_voice_count={}", voices.len());
    let Some(voice) = voices.first() else {
        println!("windows_tts_no_usable_voice");
        return;
    };
    let path = std::env::temp_dir().join(format!(
        "lanternleaf-windows-tts-{}.wav",
        std::process::id()
    ));
    let result = synthesize_windows_sentence_to_wav(
        "LanternLeaf Windows speech synthesis probe.",
        &path,
        Some(&voice.id),
    );
    result.expect("synthesizing a sentence with Windows TTS");
    let metadata = fs::metadata(&path).expect("Windows TTS probe should create a WAV");
    assert!(
        metadata.len() > 44,
        "Windows TTS probe produced an empty WAV"
    );
    let _ = fs::remove_file(path);
}
