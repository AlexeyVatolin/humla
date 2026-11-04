use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::LazyLock;

#[derive(Clone, Debug, Deserialize)]
pub enum ModelKind {
    Multilingual,
    LanguageSpecific { language: String },
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    pub filename: String,
    pub size_bytes_hint: u64,
    pub description: String,
    pub kind: ModelKind,
    pub files: Vec<ModelFile>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModelFile {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

pub const DEFAULT_MODEL_ID: &str = "large-v3-turbo-q5";
pub const GIGAAM_MODEL_ID: &str = "gigaam-v3-e2e-rnnt";

pub fn models() -> &'static [ModelInfo] {
    static MODELS: LazyLock<Vec<ModelInfo>> = LazyLock::new(|| {
        serde_json::from_str(include_str!("catalog.json")).expect("Windows model catalog")
    });
    &MODELS
}

pub fn find_model(id: &str) -> Option<&'static ModelInfo> {
    models().iter().find(|model| model.id == id)
}

pub fn default_model() -> &'static ModelInfo {
    find_model(DEFAULT_MODEL_ID).expect("Default Windows model")
}

pub fn downloaded(directory: &Path, files: &[ModelFile]) -> bool {
    files.iter().all(|file| {
        directory
            .join(&file.name)
            .metadata()
            .is_ok_and(|meta| meta.len() == file.size)
    })
}

pub fn download(
    directory: &Path,
    files: &[ModelFile],
    mut progress: impl FnMut(u64, u64),
) -> Result<()> {
    let total: u64 = files.iter().map(|file| file.size).sum();
    if downloaded(directory, files) {
        progress(total, total);
        return Ok(());
    }
    let parent = directory
        .parent()
        .context("Model directory has no parent")?;
    std::fs::create_dir_all(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix("model-")
        .tempdir_in(parent)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(1800))
        .build()?;
    let mut received = 0;
    let mut last = std::time::Instant::now();
    let mut buffer = [0u8; 64 * 1024];
    for file in files {
        let mut response = client.get(&file.url).send()?.error_for_status()?;
        let mut output = std::fs::File::create(temporary.path().join(&file.name))?;
        let mut hash = Sha256::new();
        let mut size = 0;
        loop {
            let count = response.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            output.write_all(&buffer[..count])?;
            hash.update(&buffer[..count]);
            size += count as u64;
            received += count as u64;
            if last.elapsed().as_millis() >= 200 {
                progress(received.min(total.saturating_sub(1)), total);
                last = std::time::Instant::now();
            }
        }
        ensure!(
            size == file.size,
            "Incomplete model download: {}",
            file.name
        );
        if let Some(expected) = &file.sha256 {
            ensure!(
                format!("{:x}", hash.finalize()) == *expected,
                "Model checksum mismatch: {}",
                file.name
            );
        }
        output.sync_all()?;
    }
    if downloaded(directory, files) {
        progress(total, total);
        return Ok(());
    }
    if directory.exists() {
        std::fs::remove_dir_all(directory)?;
    }
    std::fs::rename(temporary.path(), directory)?;
    progress(total, total);
    Ok(())
}
