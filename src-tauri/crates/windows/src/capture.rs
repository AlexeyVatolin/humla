use anyhow::{Context, Result};
use serde_json::json;
use std::io::Read;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use wasapi::{
    AudioClient, DeviceEnumerator, DeviceEventCallbacks, Direction, Role, SampleType, StreamMode,
    WaveFormat,
};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{IAudioCaptureClient, AUDCLNT_BUFFERFLAGS_SILENT};
use windows::Win32::System::Threading::{
    CreateEventW, OpenEventW, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
    EVENT_MODIFY_STATE,
};

use crate::wave::{StreamWriter, RATE};

#[derive(Clone, Copy)]
pub enum Signal {
    Stop,
    Pause,
    Resume,
}

fn name(pid: u32, signal: Signal) -> Vec<u16> {
    let kind = match signal {
        Signal::Stop => "stop",
        Signal::Pause => "pause",
        Signal::Resume => "resume",
    };
    format!(r"Local\HumlaCapture-{pid}-{kind}")
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

pub fn signal(pid: u32, signal: Signal) -> Result<()> {
    let name = name(pid, signal);
    let raw = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr()))? };
    let handle = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    unsafe {
        SetEvent(HANDLE(handle.as_raw_handle()))?;
    }
    Ok(())
}

struct Controls {
    events: Vec<OwnedHandle>,
    stopped: AtomicBool,
    paused: AtomicBool,
}

impl Controls {
    fn start() -> Result<Arc<Self>> {
        let controls = Arc::new(Self::new()?);
        let listener = controls.clone();
        std::thread::spawn(move || listener.listen());
        let parent = controls.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 1];
            while std::io::stdin().read(&mut buffer).is_ok_and(|n| n > 0) {}
            parent.stop();
        });
        Ok(controls)
    }

    fn new() -> Result<Self> {
        let mut events = Vec::new();
        for signal in [Signal::Stop, Signal::Pause, Signal::Resume] {
            let name = name(std::process::id(), signal);
            let handle = unsafe {
                CreateEventW(
                    None,
                    matches!(signal, Signal::Stop),
                    false,
                    PCWSTR(name.as_ptr()),
                )?
            };
            events.push(unsafe { OwnedHandle::from_raw_handle(handle.0) });
        }
        Ok(Self {
            events,
            stopped: AtomicBool::new(false),
            paused: AtomicBool::new(false),
        })
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let _ = unsafe { SetEvent(HANDLE(self.events[0].as_raw_handle())) };
    }

    fn listen(&self) {
        let handles: Vec<HANDLE> = self
            .events
            .iter()
            .map(|h| HANDLE(h.as_raw_handle()))
            .collect();
        loop {
            match unsafe { WaitForMultipleObjects(&handles, false, u32::MAX) }.0 - WAIT_OBJECT_0.0 {
                0 => {
                    self.stopped.store(true, Ordering::Release);
                    return;
                }
                1 => {
                    if !self.paused.swap(true, Ordering::AcqRel) {
                        crate::emit(json!({"event":"paused"}));
                    }
                }
                2 => {
                    if self.paused.swap(false, Ordering::AcqRel) {
                        crate::emit(json!({"event":"resumed"}));
                    }
                }
                _ => {
                    self.stop();
                    return;
                }
            }
        }
    }
}

#[derive(Default)]
struct Stats {
    mic_frames: AtomicU64,
    sys_frames: AtomicU64,
    chunks: AtomicU64,
    mic_peak: AtomicU32,
    sys_peak: AtomicU32,
    active: AtomicUsize,
    input_device: Mutex<Option<String>>,
}

fn capture_stream(
    source: &'static str,
    directory: &Path,
    excluded_pid: u32,
    controls: &Controls,
    stats: &Stats,
) -> Result<()> {
    wasapi::initialize_mta().ok()?;
    let mut writer = StreamWriter::new(source, directory);
    let result = capture_audio(source, excluded_pid, controls, stats, &mut writer);
    let old_chunks = writer.chunks;
    let finished = writer.finish();
    stats
        .chunks
        .fetch_add(writer.chunks - old_chunks, Ordering::Relaxed);
    unsafe { windows::Win32::System::Com::CoUninitialize() };
    result?;
    finished
}

fn capture_audio(
    source: &'static str,
    excluded_pid: u32,
    controls: &Controls,
    stats: &Stats,
    writer: &mut StreamWriter,
) -> Result<()> {
    let enumerator = DeviceEnumerator::new()?;
    let changed = Arc::new(AtomicBool::new(false));
    let mut callbacks = DeviceEventCallbacks::new();
    let change = changed.clone();
    callbacks.set_default_device_callback(move |direction, role, _| {
        if direction == Direction::Capture && role == Role::Console {
            change.store(true, Ordering::Release);
        }
    });
    let _registration = enumerator.register_notification_callback(callbacks)?;
    loop {
        changed.store(false, Ordering::Release);
        let mut client = if source == "sys" {
            AudioClient::new_application_loopback_client(excluded_pid, false)?
        } else {
            let device = enumerator.get_default_device(&Direction::Capture)?;
            *stats.input_device.lock().unwrap() = device.get_friendlyname().ok();
            device.get_iaudioclient()?
        };
        let format = WaveFormat::new(32, 32, &SampleType::Float, RATE as usize, 1, None);
        client.initialize_client(
            &format,
            &Direction::Capture,
            &StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: 200_000,
            },
        )?;
        let event = client.set_get_eventhandle()?;
        let capture: IAudioCaptureClient = unsafe { client.as_iaudioclient().GetService()? };
        client.start_stream()?;
        let mut buffer = Vec::with_capacity(RATE as usize);
        let mut paused = false;
        while !controls.stopped.load(Ordering::Acquire)
            && !(source == "mic" && changed.load(Ordering::Acquire))
        {
            let _ = event.wait_for_event(500);
            if source == "mic" && changed.load(Ordering::Acquire) {
                break;
            }
            loop {
                match unsafe { capture.GetNextPacketSize() } {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(error) if source == "mic" && error.code().0 as u32 == 0x88890004 => {
                        changed.store(true, Ordering::Release);
                        break;
                    }
                    Err(error) => return Err(error.into()),
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0;
                let mut flags = 0;
                unsafe {
                    capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                }
                buffer.resize(frames as usize, 0.0);
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    buffer.fill(0.0);
                } else {
                    buffer.copy_from_slice(unsafe {
                        std::slice::from_raw_parts(data.cast::<f32>(), frames as usize)
                    });
                }
                unsafe {
                    capture.ReleaseBuffer(frames)?;
                }
                let old_chunks = writer.chunks;
                let now_paused = controls.paused.load(Ordering::Acquire);
                if now_paused && !paused {
                    writer.flush_chunk()?;
                    stats
                        .chunks
                        .fetch_add(writer.chunks - old_chunks, Ordering::Relaxed);
                }
                paused = now_paused;
                if paused {
                    continue;
                }
                let old_chunks = writer.chunks;
                writer.write(&buffer)?;
                let peak = buffer.iter().fold(0.0f32, |p, sample| p.max(sample.abs()));
                let (frame_counter, peak_counter) = if source == "mic" {
                    (&stats.mic_frames, &stats.mic_peak)
                } else {
                    (&stats.sys_frames, &stats.sys_peak)
                };
                frame_counter.store(writer.frames, Ordering::Relaxed);
                peak_counter.fetch_max(peak.to_bits(), Ordering::Relaxed);
                stats
                    .chunks
                    .fetch_add(writer.chunks - old_chunks, Ordering::Relaxed);
            }
        }
        let stopped = client.stop_stream();
        if !changed.load(Ordering::Acquire) {
            stopped?;
        }
        if controls.stopped.load(Ordering::Acquire) {
            return Ok(());
        }
        let old_chunks = writer.chunks;
        writer.flush_chunk()?;
        stats
            .chunks
            .fetch_add(writer.chunks - old_chunks, Ordering::Relaxed);
        crate::emit(json!({"event":"diagnostic", "message":"The audio input device changed."}));
    }
}

pub fn run(directory: &Path, excluded_pid: u32, keep_awake: bool) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    let controls = Controls::start()?;
    let stats = Arc::new(Stats::default());
    stats.active.store(2, Ordering::Relaxed);
    if keep_awake {
        use windows::Win32::System::Power::{
            SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
        };
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        }
    }
    let mut streams = Vec::new();
    for source in ["mic", "sys"] {
        let directory = directory.to_path_buf();
        let controls = controls.clone();
        let stats = stats.clone();
        streams.push(std::thread::spawn(move || {
            if let Err(error) = capture_stream(source, &directory, excluded_pid, &controls, &stats)
            {
                crate::emit(json!({"event":"error", "message":format!("{source}: {error:#}")}));
            }
            if stats.active.fetch_sub(1, Ordering::AcqRel) == 1 {
                controls.stop();
            }
        }));
    }
    while unsafe { WaitForSingleObject(HANDLE(controls.events[0].as_raw_handle()), 1000) }
        != WAIT_OBJECT_0
    {
        crate::emit(json!({"event":"heartbeat",
            "mic_frames":stats.mic_frames.load(Ordering::Relaxed),
            "sys_frames":stats.sys_frames.load(Ordering::Relaxed),
            "chunks":stats.chunks.load(Ordering::Relaxed),
            "mic_peak":f32::from_bits(stats.mic_peak.swap(0, Ordering::Relaxed)),
            "sys_peak":f32::from_bits(stats.sys_peak.swap(0, Ordering::Relaxed)),
            "input_device":stats.input_device.lock().unwrap().clone()}));
    }
    controls.stop();
    for stream in streams {
        stream
            .join()
            .map_err(|_| anyhow::anyhow!("Audio capture thread stopped unexpectedly"))?;
    }
    if keep_awake {
        use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS};
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
    crate::emit(json!({"event":"stopped"}));
    Ok(())
}

pub fn argument<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
}

pub fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("status" | "request-microphone" | "request-screen")
    ) {
        if args[0] == "request-microphone" {
            crate::permissions::open_settings("microphone")?;
        }
        crate::emit(
            json!({"microphone":crate::permissions::microphone(), "screen":"not_required"}),
        );
        return Ok(());
    }
    let directory = Path::new(argument(&args, "--out").context("--out is required")?);
    if let Some(source) = argument(&args, "--import") {
        let controls = Controls::start()?;
        crate::import::replay(Path::new(source), directory, || {
            controls.stopped.load(Ordering::Acquire)
        })?;
        controls.stop();
        crate::emit(json!({"event":"stopped"}));
        return Ok(());
    }
    let excluded_pid = argument(&args, "--exclude-pid")
        .map(str::parse)
        .transpose()?
        .unwrap_or(std::process::id());
    run(
        directory,
        excluded_pid,
        args.iter().any(|arg| arg == "--keep-awake"),
    )
}
