use anyhow::Result;
use hound::{SampleFormat, WavSpec, WavWriter};
use serde_json::json;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

pub const RATE: u32 = 16_000;

pub struct StreamWriter {
    source: &'static str,
    directory: PathBuf,
    full: Option<WavWriter<BufWriter<File>>>,
    samples: Vec<i16>,
    pub frames: u64,
    pub chunks: u64,
    index: u64,
    peak: f32,
    silence: usize,
}

fn spec() -> WavSpec {
    WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    }
}

impl StreamWriter {
    pub fn new(source: &'static str, directory: &Path) -> Self {
        Self {
            source,
            directory: directory.into(),
            full: None,
            samples: Vec::with_capacity(RATE as usize * 15),
            frames: 0,
            chunks: 0,
            index: 0,
            peak: 0.0,
            silence: 0,
        }
    }

    pub fn write(&mut self, samples: &[f32]) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        if self.full.is_none() {
            self.full = Some(WavWriter::create(
                self.directory.join(format!("{}-full.wav", self.source)),
                spec(),
            )?);
        }
        let mut remaining = samples;
        while !remaining.is_empty() {
            let count = remaining
                .len()
                .min(320)
                .min(RATE as usize * 15 - self.samples.len());
            let samples = &remaining[..count];
            remaining = &remaining[count..];
            let peak = samples.iter().fold(0.0f32, |p, sample| p.max(sample.abs()));
            self.peak = self.peak.max(peak);
            self.silence = if peak < 0.008 {
                self.silence + samples.len()
            } else {
                0
            };
            let full = self.full.as_mut().unwrap();
            for sample in samples {
                let pcm = (sample * 32768.0)
                    .round()
                    .clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                full.write_sample(pcm)?;
                self.samples.push(pcm);
            }
            self.frames += samples.len() as u64;
            if self.samples.len() >= RATE as usize * 15
                || (self.samples.len() >= RATE as usize && self.silence >= RATE as usize / 2)
            {
                self.flush_chunk()?;
            }
        }
        Ok(())
    }

    pub fn flush_chunk(&mut self) -> Result<()> {
        if self.samples.is_empty() {
            return Ok(());
        }
        self.index += 1;
        if self.peak >= 0.005 {
            let path = self
                .directory
                .join(format!("{}-chunk-{:04}.wav", self.source, self.index));
            let mut wav = WavWriter::create(&path, spec())?;
            for sample in &self.samples {
                wav.write_sample(*sample)?;
            }
            wav.finalize()?;
            crate::emit(json!({"event":"chunk", "source":self.source, "path":path,
                "start_ms":(self.frames - self.samples.len() as u64) * 1000 / RATE as u64}));
            self.chunks += 1;
        }
        self.samples.clear();
        self.peak = 0.0;
        self.silence = 0;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        self.flush_chunk()?;
        if let Some(full) = self.full.take() {
            full.finalize()?;
            crate::emit(json!({"event":"full_recording", "source":self.source,
                "path":self.directory.join(format!("{}-full.wav", self.source)),
                "duration_ms":self.frames * 1000 / RATE as u64}));
        }
        Ok(())
    }
}
