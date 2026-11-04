use crate::models::{self, ModelFile};
use anyhow::{ensure, Context, Result};
use serde_json::json;
use sherpa_onnx::{OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig, Wave};
use std::path::PathBuf;

pub fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args
        .first()
        .context("An audio file or a model command is required")?;
    let directory = dirs::data_local_dir()
        .context("Windows AppData is unavailable")?
        .join("no.humla.app/models/diarize-directml");
    let files: Vec<ModelFile> = serde_json::from_str(include_str!("models/diarize.json"))?;
    match command.as_str() {
        "status" => {
            let downloaded = models::downloaded(&directory, &files);
            crate::emit(
                json!({"downloaded":downloaded,"needsWarmUp":false,"warmUpError":null,
                "sizeBytes":if downloaded { Some(files.iter().map(|file| file.size).sum::<u64>()) } else {None},
                "path":if downloaded { Some(directory) } else {None}}),
            );
        }
        "download" => {
            models::download(&directory, &files, |received, total| {
                crate::emit(
                    json!({"event":"progress","phase":"downloading","fraction":received as f64 / total as f64}),
                )
            })?;
            crate::emit(json!({"event":"done"}));
        }
        "delete" => {
            if directory.exists() {
                std::fs::remove_dir_all(directory)?;
            }
        }
        path => {
            ensure!(
                models::downloaded(&directory, &files),
                "Download the speaker model in Settings → Transcription first"
            );
            let mut config = OfflineSpeakerDiarizationConfig::default();
            config.segmentation.pyannote.model = Some(
                directory
                    .join("segmentation.onnx")
                    .to_string_lossy()
                    .into_owned(),
            );
            config.segmentation.provider = Some("directml".into());
            config.segmentation.num_threads = 2;
            config.embedding.model = Some(
                directory
                    .join("embedding.onnx")
                    .to_string_lossy()
                    .into_owned(),
            );
            config.embedding.provider = Some("directml".into());
            config.embedding.num_threads = 2;
            config.clustering.num_clusters = crate::capture::argument(&args, "--num-speakers")
                .map(str::parse)
                .transpose()?
                .unwrap_or(-1);
            let engine = OfflineSpeakerDiarization::create(&config).context(
                "Could not start speaker identification with DirectML. Check your graphics driver.",
            )?;
            let audio = Wave::read(&PathBuf::from(path).to_string_lossy())
                .context("Could not read the recording")?;
            ensure!(
                audio.sample_rate() == engine.sample_rate(),
                "Speaker identification requires 16 kHz audio"
            );
            let result = engine
                .process(audio.samples())
                .context("Speaker identification returned no result")?;
            let segments: Vec<_> = result
                .sort_by_start_time()
                .iter()
                .map(|segment| {
                    json!({
                        "start_ms":(segment.start.max(0.0)*1000.0) as u64,
                        "end_ms":(segment.end.max(0.0)*1000.0) as u64,
                        "speaker_id":format!("speaker_{}",segment.speaker)
                    })
                })
                .collect();
            crate::emit(json!(segments));
        }
    }
    Ok(())
}
