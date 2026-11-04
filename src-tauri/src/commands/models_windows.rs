use super::{err, local_model_dir, DownloadError, DownloadProgress, LocalWhisperModelStatus};
use crate::{local_whisper, AppState};
use humla_windows::models;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

pub(crate) fn local_model_path(
    app: &AppHandle,
    _language: &str,
    model_id: &str,
) -> Result<PathBuf, String> {
    let model = models::find_model(model_id).unwrap_or_else(models::default_model);
    Ok(local_model_dir(app)?.join(&model.filename))
}

#[tauri::command]
pub fn local_whisper_models(app: AppHandle) -> Result<Vec<LocalWhisperModelStatus>, String> {
    let directory = local_model_dir(&app)?;
    Ok(models::models()
        .iter()
        .map(|model| {
            let path = directory.join(&model.filename);
            let downloaded = models::downloaded(&path, &model.files);
            let (kind, specific_language) = match &model.kind {
                models::ModelKind::Multilingual => ("multilingual", None),
                models::ModelKind::LanguageSpecific { language } => {
                    ("language_specific", Some(language.clone()))
                }
            };
            LocalWhisperModelStatus {
                id: model.id.clone(),
                label: model.label.clone(),
                description: model.description.clone(),
                filename: model.filename.clone(),
                size_bytes_hint: model.size_bytes_hint,
                kind,
                specific_language,
                downloaded,
                size_bytes: downloaded.then_some(model.size_bytes_hint),
                path: downloaded.then(|| path.to_string_lossy().into_owned()),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn local_whisper_download(app: AppHandle, model_id: String) -> Result<(), String> {
    let model =
        models::find_model(&model_id).ok_or_else(|| format!("Unknown model: {model_id}"))?;
    let path = local_model_dir(&app)?.join(&model.filename);
    let events = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        models::download(&path, &model.files, |received, total| {
            let _ = events.emit(
                "local_whisper_progress",
                DownloadProgress {
                    model_id: model.id.clone(),
                    received,
                    total: Some(total),
                },
            );
        })
    })
    .await
    .map_err(err)?
    .map_err(err);
    if let Err(message) = &result {
        let _ = app.emit(
            "local_whisper_download_error",
            DownloadError {
                model_id,
                message: message.clone(),
            },
        );
    }
    result
}

#[tauri::command]
pub async fn local_whisper_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    let model =
        models::find_model(&model_id).ok_or_else(|| format!("Unknown model: {model_id}"))?;
    local_whisper::unload(&state.whisper);
    let path = local_model_dir(&app)?.join(&model.filename);
    if path.exists() {
        tokio::fs::remove_dir_all(path).await.map_err(err)?;
    }
    Ok(())
}
