use anyhow::{ensure, Result};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Media::MediaFoundation::*;

struct Runtime;
impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            windows::Win32::System::Com::CoUninitialize();
        }
    }
}

pub fn m4a(source: &Path, destination: &Path) -> Result<()> {
    wasapi::initialize_mta().ok()?;
    unsafe {
        MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
    }
    let _runtime = Runtime;
    let mut audio = hound::WavReader::open(source)?;
    let spec = audio.spec();
    ensure!(
        spec.channels == 1 && spec.sample_rate == 16_000 && spec.bits_per_sample == 16,
        "Expected mono PCM16 at 16 kHz"
    );
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        let writer = MFCreateSinkWriterFromURL(PCWSTR(destination.as_ptr()), None, None)?;
        let output = MFCreateMediaType()?;
        output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        output.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
        output.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 1)?;
        output.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 44_100)?;
        output.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        output.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 12_000)?;
        let stream = writer.AddStream(&output)?;
        let input = MFCreateMediaType()?;
        input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        input.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        input.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 1)?;
        input.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 16_000)?;
        input.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        input.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 2)?;
        input.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 32_000)?;
        writer.SetInputMediaType(stream, &input, None)?;
        writer.BeginWriting()?;
        let mut samples = audio.samples::<i16>();
        let mut position = 0i64;
        loop {
            let mut bytes = Vec::with_capacity(32_000);
            for sample in samples.by_ref().take(16_000) {
                bytes.extend_from_slice(&sample?.to_le_bytes());
            }
            if bytes.is_empty() {
                break;
            }
            let buffer = MFCreateMemoryBuffer(bytes.len() as u32)?;
            let mut data = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(bytes.len() as u32)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(position * 10_000_000 / 16_000)?;
            let count = bytes.len() as i64 / 2;
            sample.SetSampleDuration(count * 10_000_000 / 16_000)?;
            writer.WriteSample(stream, &sample)?;
            position += count;
        }
        writer.Finalize()?;
    }
    Ok(())
}
