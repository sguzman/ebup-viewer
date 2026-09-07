use anyhow::{Context, Result, anyhow};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use windows::Media::SpeechSynthesis::SpeechSynthesizer;
use windows::Storage::Streams::DataReader;
use windows::core::HSTRING;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct WindowsVoiceDescriptor {
    pub id: String,
    pub display_name: String,
    pub language: String,
    pub gender: String,
}

pub fn enumerate_voices() -> Result<Vec<WindowsVoiceDescriptor>> {
    SpeechSynthesizer::new().context("initializing Windows speech synthesizer")?;
    let voices = SpeechSynthesizer::AllVoices().context("enumerating Windows voices")?;
    let mut result = Vec::with_capacity(voices.Size()? as usize);
    for index in 0..voices.Size()? {
        let voice = voices.GetAt(index)?;
        result.push(WindowsVoiceDescriptor {
            id: voice.Id()?.to_string_lossy(),
            display_name: voice.DisplayName()?.to_string_lossy(),
            language: voice.Language()?.to_string_lossy(),
            gender: format!("{:?}", voice.Gender()),
        });
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

pub fn synthesize_sentence_to_wav(text: &str, path: &Path, voice_id: Option<&str>) -> Result<()> {
    let synthesizer = SpeechSynthesizer::new().context("creating Windows speech synthesizer")?;
    if let Some(id) = voice_id {
        let voices = SpeechSynthesizer::AllVoices()?;
        let mut selected = None;
        for index in 0..voices.Size()? {
            let candidate = voices.GetAt(index)?;
            if candidate.Id()?.to_string_lossy() == id {
                selected = Some(candidate);
                break;
            }
        }
        let voice = selected.ok_or_else(|| anyhow!("Windows voice ID was not found: {id}"))?;
        synthesizer.SetVoice(&voice)?;
    }
    let stream = synthesizer
        .SynthesizeTextToStreamAsync(&HSTRING::from(text))?
        .join()
        .context("waiting for Windows speech synthesis")?;
    let content_type = stream.ContentType()?.to_string_lossy();
    if !content_type.eq_ignore_ascii_case("audio/wav") {
        return Err(anyhow!(
            "Windows speech synthesis returned unsupported content type: {content_type}"
        ));
    }
    let size = stream.Size()?;
    if size == 0 || size > u32::MAX as u64 {
        return Err(anyhow!(
            "Windows speech synthesis returned invalid stream size: {size}"
        ));
    }
    let input = stream.GetInputStreamAt(0)?;
    let reader = DataReader::CreateDataReader(&input)?;
    reader
        .LoadAsync(size as u32)?
        .join()
        .context("reading Windows speech stream")?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;
    let temp_path = path.with_extension("wav.partial");
    let mut file =
        File::create(&temp_path).with_context(|| format!("creating {}", temp_path.display()))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temp_path, path).with_context(|| format!("publishing {}", path.display()))?;
    Ok(())
}
