use anyhow::{bail, Context, Result};
use rubato::{FftFixedInOut, Resampler};
use std::io::ErrorKind;
use std::path::Path;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::wave::{StreamWriter, RATE};

struct Converter {
    resampler: FftFixedInOut<f32>,
    pending: Vec<f32>,
    skip: usize,
    input: u64,
    output: u64,
    rate: u32,
}

impl Converter {
    fn new(rate: u32) -> Result<Self> {
        let resampler = FftFixedInOut::new(rate as usize, RATE as usize, 1024, 1)?;
        let skip = resampler.output_delay();
        Ok(Self {
            resampler,
            pending: Vec::new(),
            skip,
            input: 0,
            output: 0,
            rate,
        })
    }

    fn output(
        &mut self,
        samples: &[f32],
        writer: &mut StreamWriter,
        limit: Option<u64>,
    ) -> Result<()> {
        let skipped = self.skip.min(samples.len());
        self.skip -= skipped;
        let samples = &samples[skipped..];
        let count = limit.map_or(samples.len(), |n| {
            samples.len().min(n.saturating_sub(self.output) as usize)
        });
        writer.write(&samples[..count])?;
        self.output += count as u64;
        Ok(())
    }

    fn write(&mut self, samples: &[f32], writer: &mut StreamWriter) -> Result<()> {
        self.input += samples.len() as u64;
        self.pending.extend_from_slice(samples);
        let frames = self.resampler.input_frames_next();
        let mut consumed = 0;
        while self.pending.len() - consumed >= frames {
            let output = self
                .resampler
                .process(&[&self.pending[consumed..consumed + frames]], None)?;
            self.output(&output[0], writer, None)?;
            consumed += frames;
        }
        self.pending.drain(..consumed);
        Ok(())
    }

    fn finish(&mut self, writer: &mut StreamWriter) -> Result<()> {
        let expected = (self.input * RATE as u64 + self.rate as u64 / 2) / self.rate as u64;
        let output = self
            .resampler
            .process_partial(Some(&[&self.pending]), None)?;
        self.output(&output[0], writer, Some(expected))?;
        while self.output < expected {
            let output = self.resampler.process_partial::<Vec<f32>>(None, None)?;
            self.output(&output[0], writer, Some(expected))?;
        }
        Ok(())
    }
}

pub fn replay(source: &Path, directory: &Path, stopped: impl Fn() -> bool) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    let input = MediaSourceStream::new(Box::new(std::fs::File::open(source)?), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = source.extension().and_then(|s| s.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe().probe(
        &hint,
        input,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;
    let track = format
        .default_track(TrackType::Audio)
        .context("No audio track was found")?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .context("No audio codec was found")?;
    let rate = params
        .sample_rate
        .context("The audio sample rate is missing")?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())?;
    let mut converter = if rate == RATE {
        None
    } else {
        Some(Converter::new(rate)?)
    };
    let mut writer = StreamWriter::new("mic", directory);
    let mut samples = Vec::<f32>::new();
    let mut mono = Vec::<f32>::new();
    while !stopped() {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(Error::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error.into()),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet)?;
        if decoded.spec().rate() != rate {
            bail!("The sample rate changed inside the audio file");
        }
        let channels = decoded.spec().channels().count();
        samples.resize(decoded.samples_interleaved(), 0.0);
        decoded.copy_to_slice_interleaved(&mut samples);
        mono.clear();
        mono.extend(
            samples
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32),
        );
        if let Some(converter) = &mut converter {
            converter.write(&mono, &mut writer)?;
        } else {
            writer.write(&mono)?;
        }
    }
    if let Some(converter) = &mut converter {
        converter.finish(&mut writer)?;
    }
    writer.finish()
}
