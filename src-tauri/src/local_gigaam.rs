use anyhow::{anyhow, ensure, Context, Result};
use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::local_whisper::{Engine, LoadedModel, SharedContext};
use crate::stt::Word;

pub const MODEL_ID: &str = "gigaam-v3-e2e-rnnt";
pub const MODEL_FILENAME: &str = "gigaam-v3-e2e-rnnt-F16.gguf";

pub fn is_model(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == MODEL_FILENAME)
}

pub struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Worker {
    fn load(model: &Path) -> Result<Self> {
        let executable = std::env::current_exe()?.with_file_name("humla-gigaam");
        let executable = if executable.exists() {
            executable
        } else {
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                "binaries/humla-gigaam-{}-apple-darwin",
                if cfg!(target_arch = "aarch64") {
                    "aarch64"
                } else {
                    "x86_64"
                }
            ))
        };
        let mut child = Command::new(executable)
            .arg(model)
            .env_remove("GGML_METAL_PATH_RESOURCES")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .context("start GigaAM")?;
        let input = child.stdin.take().context("GigaAM stdin")?;
        let output = BufReader::new(child.stdout.take().context("GigaAM stdout")?);
        let mut worker = Self {
            child,
            input,
            output,
        };
        let backend: String = serde_json::from_str(&worker.read_line()?)?;
        ensure!(backend == "metal", "GigaAM requires Metal GPU inference");
        Ok(worker)
    }

    fn read_line(&mut self) -> Result<String> {
        let mut line = String::new();
        ensure!(
            self.output.read_line(&mut line)? > 0,
            "GigaAM worker stopped"
        );
        Ok(line)
    }

    fn transcribe(&mut self, path: &Path) -> Result<Transcript> {
        serde_json::to_writer(&mut self.input, path)?;
        writeln!(self.input)?;
        self.input.flush()?;
        Ok(serde_json::from_str(&self.read_line()?)?)
    }
}

pub fn with_worker<T>(
    shared: &SharedContext,
    path: &Path,
    run: impl FnOnce(&mut Worker) -> Result<T>,
) -> Result<T> {
    let mut guard = shared.lock();
    if !guard.as_ref().is_some_and(|loaded| loaded.path == path) {
        *guard = None;
        *guard = Some(LoadedModel {
            path: path.to_path_buf(),
            use_gpu: true,
            engine: Engine::GigaAm(Worker::load(path)?),
        });
    }
    let Some(LoadedModel {
        engine: Engine::GigaAm(worker),
        ..
    }) = guard.as_mut()
    else {
        return Err(anyhow!("GigaAM model is not loaded"));
    };
    let result = run(worker);
    if result.is_err() {
        *guard = None;
    }
    result
}

#[derive(Deserialize)]
struct Transcript {
    text: String,
    tokens: Vec<Token>,
}

#[derive(Deserialize)]
struct Token {
    text: String,
    t0_ms: i64,
    t1_ms: i64,
}

impl Transcript {
    fn words(&self) -> Vec<Word> {
        let mut words: Vec<Word> = Vec::new();
        let mut boundary = true;
        for token in &self.tokens {
            let text = token.text.trim_start_matches('▁');
            if text.len() != token.text.len() {
                boundary = true;
            }
            if text.trim().is_empty() {
                boundary = true;
            } else if boundary {
                words.push(Word {
                    text: text.to_string(),
                    start_ms: token.t0_ms.max(0) as u64,
                    end_ms: token.t1_ms.max(0) as u64,
                });
                boundary = false;
            } else if let Some(word) = words.last_mut() {
                word.text.push_str(text);
                word.end_ms = word.end_ms.max(token.t1_ms.max(0) as u64);
            }
        }
        words
    }
}

pub async fn transcribe_file(
    shared: SharedContext,
    model: PathBuf,
    audio: &Path,
) -> Result<(String, Vec<Word>, Option<String>)> {
    let audio = audio.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let result = with_worker(&shared, &model, |worker| worker.transcribe(&audio))?;
        let words = result.words();
        Ok((result.text, words, Some("ru".into())))
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentencepiece_timestamps_preserve_words_and_punctuation() {
        let transcript: Transcript = serde_json::from_str(
            r#"{
                "text":"Он добавил, что следует.",
                "tokens":[
                    {"text":"▁Он","t0_ms":0,"t1_ms":40},
                    {"text":"▁до","t0_ms":80,"t1_ms":120},
                    {"text":"бавил","t0_ms":120,"t1_ms":160},
                    {"text":",","t0_ms":160,"t1_ms":200},
                    {"text":"▁что","t0_ms":240,"t1_ms":280},
                    {"text":"▁","t0_ms":280,"t1_ms":320},
                    {"text":"следует","t0_ms":320,"t1_ms":360},
                    {"text":".","t0_ms":360,"t1_ms":400}
                ]
            }"#,
        ).unwrap();
        let words = transcript.words();
        assert_eq!(words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
            ["Он", "добавил,", "что", "следует."]);
        assert_eq!((words[1].start_ms, words[1].end_ms), (80, 200));
        assert_eq!((words[3].start_ms, words[3].end_ms), (320, 400));
    }

    #[test]
    fn character_timestamps_form_russian_words() {
        let transcript: Transcript = serde_json::from_str(
            r#"{
            "text": "привет мир",
            "tokens": [
                {"text":"п","t0_ms":0,"t1_ms":40},
                {"text":"р","t0_ms":40,"t1_ms":80},
                {"text":"и","t0_ms":80,"t1_ms":120},
                {"text":"в","t0_ms":120,"t1_ms":160},
                {"text":"е","t0_ms":160,"t1_ms":200},
                {"text":"т","t0_ms":200,"t1_ms":240},
                {"text":" ","t0_ms":240,"t1_ms":280},
                {"text":"м","t0_ms":320,"t1_ms":360},
                {"text":"и","t0_ms":360,"t1_ms":400},
                {"text":"р","t0_ms":400,"t1_ms":440}
            ]
        }"#,
        )
        .unwrap();
        let words = transcript.words();
        assert_eq!(
            words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            transcript.text
        );
        assert_eq!((words[0].start_ms, words[0].end_ms), (0, 240));
        assert_eq!((words[1].start_ms, words[1].end_ms), (320, 440));
    }

    #[tokio::test]
    #[ignore]
    async fn gpu_model_reuses_worker_and_releases_it_on_unload() {
        let model = PathBuf::from(std::env::var_os("HUMLA_GIGAAM_MODEL").unwrap());
        let audio = PathBuf::from(std::env::var_os("HUMLA_GIGAAM_WAV").unwrap());
        let shared = crate::local_whisper::new_shared();
        crate::local_whisper::prewarm(shared.clone(), model.clone(), true, "ru".into())
            .await
            .unwrap();
        let pid = with_worker(&shared, &model, |w| Ok(w.child.id())).unwrap();
        for _ in 0..2 {
            let (text, words, lang) = crate::local_whisper::transcribe_file_with_words(
                shared.clone(),
                model.clone(),
                false,
                "ru",
                None,
                crate::local_whisper::Preset::Quality,
                &audio,
            )
            .await
            .unwrap();
            assert!(!text.is_empty());
            assert_eq!(
                words
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                text.trim()
            );
            assert!(words.iter().all(|w| w.end_ms > w.start_ms));
            assert_eq!(lang.as_deref(), Some("ru"));
            assert_eq!(
                with_worker(&shared, &model, |w| Ok(w.child.id())).unwrap(),
                pid
            );
        }
        crate::local_whisper::unload(&shared);
        assert!(shared.lock().is_none());
        assert_ne!(unsafe { libc::kill(pid as i32, 0) }, 0);
    }
}
