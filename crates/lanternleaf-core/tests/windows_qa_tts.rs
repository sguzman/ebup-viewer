#![cfg(windows)]

use lanternleaf_core::config::{self, TtsBackend};
use lanternleaf_core::tts::{
    enumerate_windows_voices, resolve_windows_voice_id, synthesize_windows_sentence_to_wav,
};
use rodio::{Decoder, Source};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::PathBuf;

#[test]
fn staged_qa_config_uses_windows_backend_and_decodes_synthesis() {
    let config_path = std::env::var_os("LANTERNLEAF_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.qa/windows/conf/config.toml")
        });
    assert!(
        config_path.is_file(),
        "repo-native QA config was not staged: {}",
        config_path.display()
    );

    // Mirror qa.ps1's repo-native override even when this focused regression is
    // launched directly by `cargo test --workspace`.
    unsafe {
        std::env::set_var("LANTERNLEAF_QA_TTS_BACKEND", "windows");
    }
    let config = config::load_config(&config_path);
    unsafe {
        std::env::remove_var("LANTERNLEAF_QA_TTS_BACKEND");
    }
    assert_eq!(config.tts_backend, TtsBackend::Windows);
    println!("windows_qa_effective_backend={:?}", config.tts_backend);

    let voices = enumerate_windows_voices().expect("enumerate installed Windows voices");
    assert!(
        !voices.is_empty(),
        "Windows CI must expose an installed voice"
    );
    let voice_id = resolve_windows_voice_id(config.windows_voice_id.as_deref())
        .expect("resolve the staged QA Windows voice");
    assert!(!voice_id.is_empty());
    println!("windows_qa_effective_voice_id={voice_id}");

    let path = std::env::temp_dir().join(format!(
        "lanternleaf-staged-qa-tts-{}.wav",
        std::process::id()
    ));
    synthesize_windows_sentence_to_wav(
        "LanternLeaf staged Windows QA synthesis probe.",
        &path,
        Some(&voice_id),
    )
    .expect("synthesize staged Windows QA sentence");
    let decoder = Decoder::new(BufReader::new(
        File::open(&path).expect("open staged QA WAV"),
    ))
    .expect("shared Rodio decoder should decode staged QA WAV");
    assert!(decoder.channels() > 0);
    assert!(decoder.sample_rate() > 0);
    assert!(
        decoder
            .total_duration()
            .is_some_and(|duration| !duration.is_zero())
    );
    println!("windows_qa_synthesis=decoded");
    let _ = fs::remove_file(path);
}
