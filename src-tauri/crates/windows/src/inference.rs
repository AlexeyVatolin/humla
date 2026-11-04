use anyhow::{ensure, Context, Result};
use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, OfflineRecognizerResult,
    OfflineTransducerModelConfig, OfflineWhisperModelConfig,
};
use std::path::Path;

use crate::models::{self, GIGAAM_MODEL_ID};

pub struct Recognizer {
    inner: OfflineRecognizer,
}

#[derive(Clone, Debug)]
pub struct Word {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

pub struct Transcript {
    pub text: String,
    pub words: Vec<Word>,
    pub detected_language: Option<String>,
}

impl Recognizer {
    pub fn load(directory: &Path, gpu: bool, language: &str) -> Result<Self> {
        let id = directory
            .file_name()
            .and_then(|name| name.to_str())
            .context("Invalid model path")?;
        let model = models::find_model(id).context("Unknown Windows transcription model")?;
        ensure!(
            models::downloaded(directory, &model.files),
            "Download {} in Settings → Transcription first",
            model.label
        );
        ensure!(
            id != GIGAAM_MODEL_ID || matches!(language, "auto" | "ru"),
            "GigaAM supports Russian only"
        );
        let file = |suffix: &str| -> Option<String> {
            model
                .files
                .iter()
                .find(|file| file.name.ends_with(suffix))
                .map(|file| directory.join(&file.name).to_string_lossy().into_owned())
        };
        let mut config = OfflineRecognizerConfig::default();
        config.model_config.num_threads = 2;
        config.model_config.provider = Some(if gpu { "directml" } else { "cpu" }.into());
        config.model_config.tokens = file("tokens.txt");
        config.decoding_method = Some("greedy_search".into());
        if id == GIGAAM_MODEL_ID {
            config.model_config.transducer = OfflineTransducerModelConfig {
                encoder: file("encoder.int8.onnx"),
                decoder: file("decoder.onnx"),
                joiner: file("joiner.onnx"),
            };
            config.model_config.model_type = Some("nemo_transducer".into());
        } else {
            config.model_config.whisper = OfflineWhisperModelConfig {
                encoder: file("encoder.int8.onnx"),
                decoder: file("decoder.int8.onnx"),
                language: if language == "auto" {
                    None
                } else {
                    Some(language.into())
                },
                task: Some("transcribe".into()),
                enable_segment_timestamps: true,
                ..Default::default()
            };
        }
        let inner = OfflineRecognizer::create(&config).context(if gpu {
            "Could not start DirectML inference. Update your graphics driver and check that your GPU supports DirectX 12."
        } else { "Could not load the transcription model" })?;
        Ok(Self { inner })
    }

    pub fn transcribe(&self, samples: &[f32], gigaam: bool, language: &str) -> Result<Transcript> {
        let stream = self.inner.create_stream();
        stream.accept_waveform(16_000, samples);
        self.inner.decode(&stream);
        let result = stream
            .get_result()
            .context("The transcription model returned no result")?;
        let words = words(&result, samples.len() as u64 / 16);
        Ok(Transcript {
            text: result.text.trim().into(),
            words,
            detected_language: (gigaam && language == "auto").then(|| "ru".into()),
        })
    }
}

fn words(result: &OfflineRecognizerResult, duration_ms: u64) -> Vec<Word> {
    let Some(timestamps) = &result.timestamps else {
        return Vec::new();
    };
    let mut words = Vec::<Word>::new();
    for (index, (token, &time)) in result.tokens.iter().zip(timestamps).enumerate() {
        if token.starts_with('<') {
            continue;
        }
        let start_ms = (time.max(0.0) * 1000.0) as u64;
        let length = result
            .durations
            .as_ref()
            .and_then(|values| values.get(index))
            .copied()
            .unwrap_or(0.04);
        let end_ms = (start_ms + (length.max(0.001) * 1000.0) as u64).min(duration_ms);
        let text = token.replace('▁', " ");
        if text.trim().is_empty() {
            continue;
        }
        if text.starts_with(' ') || words.is_empty() {
            words.push(Word {
                text: text.trim_start().into(),
                start_ms: start_ms.min(duration_ms),
                end_ms,
            });
        } else if let Some(word) = words.last_mut() {
            word.text.push_str(&text);
            word.end_ms = end_ms;
        }
    }
    words
}
