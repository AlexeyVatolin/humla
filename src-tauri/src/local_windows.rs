use anyhow::Result;
use humla_windows::inference::Recognizer;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type SharedContext = Arc<Mutex<Option<LoadedModel>>>;

pub struct LoadedModel {
    path: PathBuf,
    use_gpu: bool,
    language: String,
    recognizer: Recognizer,
}

#[derive(Clone, Copy)]
pub enum Preset {
    Fast,
    Balanced,
    Quality,
}

impl Preset {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "fast" => Self::Fast,
            "balanced" => Self::Balanced,
            _ => Self::Quality,
        }
    }
}

pub fn new_shared() -> SharedContext {
    Arc::new(Mutex::new(None))
}
pub fn unload(shared: &SharedContext) {
    *shared.lock() = None;
}

fn loaded<'a>(
    slot: &'a mut Option<LoadedModel>,
    path: &Path,
    gpu: bool,
    language: &str,
) -> Result<&'a Recognizer> {
    if !slot.as_ref().is_some_and(|model| {
        model.path == path && model.use_gpu == gpu && model.language == language
    }) {
        *slot = None;
        *slot = Some(LoadedModel {
            path: path.into(),
            use_gpu: gpu,
            language: language.into(),
            recognizer: Recognizer::load(path, gpu, language)?,
        });
    }
    Ok(&slot.as_ref().unwrap().recognizer)
}

pub async fn prewarm(shared: SharedContext, model_path: PathBuf, use_gpu: bool) -> Result<()> {
    if !crate::local_gigaam::is_model(&model_path) {
        return Ok(());
    }
    tokio::task::spawn_blocking(move || {
        loaded(&mut shared.lock(), &model_path, use_gpu, "ru").map(|_| ())
    })
    .await?
}

pub async fn transcribe_file_with_words(
    shared: SharedContext,
    model_path: PathBuf,
    use_gpu: bool,
    language: &str,
    _initial_prompt: Option<&str>,
    _preset: Preset,
    audio_path: &Path,
) -> Result<(String, Vec<crate::stt::Word>, Option<String>)> {
    let samples = crate::wav::read_f32_mono_16k(audio_path).await?;
    let gigaam = crate::local_gigaam::is_model(&model_path);
    let language = language.to_string();
    tokio::task::spawn_blocking(move || {
        let mut slot = shared.lock();
        let key = if gigaam { "ru" } else { &language };
        let result = loaded(&mut slot, &model_path, use_gpu, key)?
            .transcribe(&samples, gigaam, &language)?;
        let words = result
            .words
            .into_iter()
            .map(|word| crate::stt::Word {
                text: word.text,
                start_ms: word.start_ms,
                end_ms: word.end_ms,
            })
            .collect();
        Ok((result.text, words, result.detected_language))
    })
    .await?
}
