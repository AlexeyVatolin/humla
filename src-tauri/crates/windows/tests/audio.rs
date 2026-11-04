use humla_windows::{import, wave::StreamWriter};

#[test]
fn speech_is_bounded_and_full_recording_keeps_pcm() {
    let directory = tempfile::tempdir().unwrap();
    let samples: Vec<i16> = (0..16_000 * 34)
        .map(|index| ((index % 1024) as i16 - 512) * 32)
        .collect();
    let mut writer = StreamWriter::new("mic", directory.path());
    writer
        .write(
            &samples
                .iter()
                .map(|sample| *sample as f32 / 32768.0)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    writer.finish().unwrap();
    let mut full = hound::WavReader::open(directory.path().join("mic-full.wav")).unwrap();
    assert_eq!(
        full.samples::<i16>()
            .map(Result::unwrap)
            .collect::<Vec<_>>(),
        samples
    );
    let mut chunks: Vec<_> = std::fs::read_dir(directory.path())
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| entry.file_name().to_string_lossy().contains("chunk"))
        .map(|entry| entry.path())
        .collect();
    chunks.sort();
    let mut joined = Vec::new();
    for path in chunks {
        let mut wav = hound::WavReader::open(path).unwrap();
        assert!(wav.duration() <= 16_000 * 15);
        joined.extend(wav.samples::<i16>().map(Result::unwrap));
    }
    assert_eq!(joined, samples);
}

#[test]
fn stereo_import_resamples_without_a_shift_or_silent_tail() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("stereo.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = hound::WavWriter::create(&input, spec).unwrap();
    for frame in 0..48_000 * 3 {
        let sample =
            ((frame as f64 * std::f64::consts::TAU * 440.0 / 48_000.0).sin() * 16_000.0) as i16;
        wav.write_sample(sample).unwrap();
        wav.write_sample(sample).unwrap();
    }
    wav.finalize().unwrap();
    let output = directory.path().join("output");
    import::replay(&input, &output, || false).unwrap();
    let mut wav = hound::WavReader::open(output.join("mic-full.wav")).unwrap();
    assert_eq!(wav.spec().sample_rate, 16_000);
    assert_eq!(wav.spec().channels, 1);
    let samples: Vec<i16> = wav.samples::<i16>().map(Result::unwrap).collect();
    assert_eq!(samples.len(), 16_000 * 3);
    let error = samples
        .iter()
        .enumerate()
        .skip(160)
        .take(samples.len() - 320)
        .map(|(frame, sample)| {
            let expected =
                (frame as f64 * std::f64::consts::TAU * 440.0 / 16_000.0).sin() * 16_000.0;
            (*sample as f64 - expected).powi(2)
        })
        .sum::<f64>()
        / (samples.len() - 320) as f64;
    assert!(
        error.sqrt() < 3.0,
        "Resampling changed the waveform: RMS error {}",
        error.sqrt()
    );
}

#[cfg(windows)]
#[test]
fn windows_aac_export_can_be_imported_again() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = hound::WavWriter::create(&input, spec).unwrap();
    for frame in 0..16_000 * 3 {
        let sample =
            ((frame as f64 * std::f64::consts::TAU * 440.0 / 16_000.0).sin() * 16_000.0) as i16;
        wav.write_sample(sample).unwrap();
    }
    wav.finalize().unwrap();
    let encoded = directory.path().join("output.m4a");
    humla_windows::export::m4a(&input, &encoded).unwrap();
    assert!(encoded.metadata().unwrap().len() > 1000);
    let output = directory.path().join("decoded");
    import::replay(&encoded, &output, || false).unwrap();
    let mut wav = hound::WavReader::open(output.join("mic-full.wav")).unwrap();
    assert_eq!(wav.spec().sample_rate, 16_000);
    assert!((47_000..52_000).contains(&wav.duration()));
    assert!(wav
        .samples::<i16>()
        .map(Result::unwrap)
        .any(|sample| sample.abs() > 8000));
}
