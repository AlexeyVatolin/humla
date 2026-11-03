use anyhow::{ensure, Context, Result};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use transcribe_cpp::{Backend, Model, ModelOptions, RunOptions, SessionOptions, TimestampKind};

fn main() -> Result<()> {
    transcribe_cpp::disable_logging();
    let path = std::env::args_os().nth(1).context("missing model path")?;
    let model = Model::load_with(
        path,
        &ModelOptions {
            backend: Backend::Metal,
            ..Default::default()
        },
    )?;
    ensure!(
        model.device()?.kind == "metal",
        "GigaAM requires Metal GPU inference"
    );
    let mut session = model.session_with(&SessionOptions {
        n_threads: 2,
        ..Default::default()
    })?;
    let options = RunOptions {
        language: Some("ru".into()),
        timestamps: TimestampKind::Token,
        ..Default::default()
    };
    let mut output = io::stdout().lock();
    writeln!(output, "\"metal\"")?;
    output.flush()?;
    for line in io::stdin().lock().lines() {
        let path: PathBuf = serde_json::from_str(&line?)?;
        let mut wav = hound::WavReader::open(path)?;
        let spec = wav.spec();
        ensure!(
            spec.channels == 1
                && spec.sample_rate == 16_000
                && spec.bits_per_sample == 16
                && spec.sample_format == hound::SampleFormat::Int,
            "expected 16 kHz mono PCM16 WAV"
        );
        let samples = wav
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / 32768.0))
            .collect::<Result<Vec<_>, _>>()?;
        let result = session.run(&samples, &options)?;
        serde_json::to_writer(&mut output, &result)?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}
