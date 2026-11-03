use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::{db, menubar, AppState};

static EVENTS: OnceLock<mpsc::Sender<()>> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(true);

const START_DELAY: Duration = Duration::from_secs(3);
const END_DELAY: Duration = Duration::from_secs(20);

#[derive(Default)]
struct CallState {
    started: bool,
    deadline: Option<Instant>,
    input: bool,
}

impl CallState {
    fn update(&mut self, input: bool, now: Instant) -> Option<bool> {
        if input != self.input {
            self.input = input;
            self.deadline = if input == self.started {
                None
            } else {
                Some(now + if input { START_DELAY } else { END_DELAY })
            };
        }
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.deadline = None;
            self.started = input;
            return Some(input);
        }
        None
    }
}

pub fn dismiss(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("meeting-prompt") {
        let _ = window.destroy();
    }
}

pub fn set_enabled(app: &AppHandle, enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        dismiss(app);
    }
    if let Some(events) = EVENTS.get() {
        let _ = events.try_send(());
    }
}

fn show_prompt(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("meeting-prompt").is_some() {
        return Ok(());
    }
    let mut builder = WebviewWindowBuilder::new(
        app, "meeting-prompt", WebviewUrl::App("meeting.html".into()),
    )
    .title("Record Zoom meeting?")
    .inner_size(360.0, 154.0)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .visible(false)
    .accept_first_mouse(true)
    .focused(false);
    let monitor = app.cursor_position().ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let area = monitor.work_area();
        let scale = monitor.scale_factor();
        builder = builder.position(
            (area.position.x as f64 + area.size.width as f64) / scale - 380.0,
            area.position.y as f64 / scale + 20.0,
        );
    }
    let window = builder.build()?;
    #[cfg(target_os = "macos")]
    {
        let prompt = window.clone();
        window.run_on_main_thread(move || {
            if !ENABLED.load(Ordering::Relaxed) || !native::input_active() || !menubar::can_start(prompt.app_handle()) {
                let _ = prompt.destroy();
                return;
            }
            if let Ok(raw) = prompt.ns_window() {
                unsafe {
                    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
                    let native = &*raw.cast::<NSWindow>();
                    native.setCollectionBehavior(native.collectionBehavior()
                        | NSWindowCollectionBehavior::FullScreenAuxiliary);
                    native.orderFrontRegardless();
                }
            }
        })?;
    }
    #[cfg(not(target_os = "macos"))]
    window.show()?;
    Ok(())
}

#[tauri::command]
pub fn meeting_prompt_dismiss(app: AppHandle) {
    dismiss(&app);
}

#[tauri::command]
pub async fn meeting_prompt_start(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let active = native::input_active();
    #[cfg(not(target_os = "macos"))]
    let active = false;
    if !ENABLED.load(Ordering::Relaxed) || !active {
        dismiss(&app);
        return Err("Zoom is no longer using the microphone.".into());
    }
    if !menubar::can_start(&app) {
        dismiss(&app);
        return Err("A recording is already in progress.".into());
    }
    menubar::headless_start(&app).await?;
    dismiss(&app);
    Ok(())
}

pub fn install(app: &AppHandle) {
    let enabled = db::get_setting(&app.state::<AppState>().db.lock(), "zoom_meeting_prompt")
        .ok().flatten().as_deref() != Some("false");
    ENABLED.store(enabled, Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    native::install(app);
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use objc2_core_audio::*;
    use objc2_core_foundation::{CFRetained, CFString};
    use std::collections::BTreeSet;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut, NonNull};

    static RESCAN: AtomicBool = AtomicBool::new(true);
    const SYSTEM: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;

    fn address(selector: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        }
    }

    unsafe extern "C-unwind" fn changed(
        object: AudioObjectID, _: u32, _: NonNull<AudioObjectPropertyAddress>, _: *mut c_void,
    ) -> i32 {
        if object == SYSTEM {
            RESCAN.store(true, Ordering::Relaxed);
        }
        if let Some(events) = EVENTS.get() {
            let _ = events.try_send(());
        }
        0
    }

    fn read<T>(object: AudioObjectID, selector: u32, value: &mut T) -> bool {
        let mut size = std::mem::size_of::<T>() as u32;
        unsafe {
            AudioObjectGetPropertyData(
                object, NonNull::from(&mut address(selector)), 0, null(),
                NonNull::from(&mut size), NonNull::from(value).cast(),
            ) == 0
        }
    }

    fn is_zoom(object: AudioObjectID) -> bool {
        let mut value: *mut CFString = null_mut();
        if !read(object, kAudioProcessPropertyBundleID, &mut value) {
            return false;
        }
        let Some(value) = NonNull::new(value) else { return false };
        let id = unsafe { CFRetained::from_raw(value) }.to_string();
        id == "us.zoom.xos" || id.starts_with("us.zoom.xos.") || id == "us.zoom.CptHost"
    }

    fn running(object: AudioObjectID) -> bool {
        let mut active = 0u32;
        read(object, kAudioProcessPropertyIsRunningInput, &mut active) && active != 0
    }

    fn processes() -> Result<BTreeSet<AudioObjectID>, i32> {
        let mut addr = address(kAudioHardwarePropertyProcessObjectList);
        let mut size = 0;
        let status = unsafe {
            AudioObjectGetPropertyDataSize(SYSTEM, NonNull::from(&mut addr),
                0, null(), NonNull::from(&mut size))
        };
        if status != 0 { return Err(status); }
        let mut ids = vec![0u32; size as usize / std::mem::size_of::<u32>()];
        if ids.is_empty() { return Ok(BTreeSet::new()); }
        let status = unsafe {
            AudioObjectGetPropertyData(SYSTEM, NonNull::from(&mut addr),
                0, null(), NonNull::from(&mut size), NonNull::new(ids.as_mut_ptr()).unwrap().cast())
        };
        if status != 0 { return Err(status); }
        ids.truncate(size as usize / std::mem::size_of::<u32>());
        Ok(ids.into_iter().filter(|id| is_zoom(*id)).collect())
    }

    fn listen(object: AudioObjectID, selector: u32, add: bool) -> i32 {
        unsafe {
            let mut address = address(selector);
            let addr = NonNull::from(&mut address);
            if add {
                AudioObjectAddPropertyListener(object, addr, Some(changed), null_mut())
            } else {
                AudioObjectRemovePropertyListener(object, addr, Some(changed), null_mut())
            }
        }
    }

    pub(super) fn input_active() -> bool {
        processes().is_ok_and(|ids| ids.into_iter().any(running))
    }

    pub(super) fn install(app: &AppHandle) {
        let (tx, mut rx) = mpsc::channel(1);
        if EVENTS.set(tx).is_err() { return; }
        let status = listen(SYSTEM, kAudioHardwarePropertyProcessObjectList, true);
        if status != 0 {
            eprintln!("Zoom meeting detection unavailable: CoreAudio {status}");
            return;
        }
        let app = app.clone();
        let _ = EVENTS.get().unwrap().try_send(());
        tauri::async_runtime::spawn(async move {
            let mut watched = BTreeSet::new();
            let mut call = CallState::default();
            loop {
                tokio::select! {
                    event = rx.recv() => { if event.is_none() { break; } }
                    _ = async {
                        match call.deadline {
                            Some(deadline) => tokio::time::sleep_until(deadline).await,
                            None => std::future::pending::<()>().await,
                        }
                    } => {}
                }
                let enabled = ENABLED.load(Ordering::Relaxed);
                if !enabled || RESCAN.swap(false, Ordering::Relaxed) || watched.is_empty() {
                    let next = if enabled { processes() } else { Ok(BTreeSet::new()) };
                    let Ok(next) = next else {
                        call = CallState::default();
                        RESCAN.store(true, Ordering::Relaxed);
                        dismiss(&app);
                        continue;
                    };
                    for id in watched.difference(&next) {
                        listen(*id, kAudioProcessPropertyIsRunningInput, false);
                    }
                    for id in next.difference(&watched) {
                        let status = listen(*id, kAudioProcessPropertyIsRunningInput, true);
                        if status != 0 {
                            eprintln!("Zoom input monitoring failed: CoreAudio {status}");
                        }
                    }
                    watched = next;
                }
                let input = enabled && watched.iter().any(|id| running(*id));
                if !input { dismiss(&app); }
                if !enabled {
                    call = CallState::default();
                    continue;
                }
                match call.update(input, Instant::now()) {
                    Some(true) if menubar::can_start(&app) => {
                        let onboarded = db::get_setting(&app.state::<AppState>().db.lock(), "onboarding_completed")
                            .ok().flatten().as_deref() == Some("true");
                        if onboarded {
                            if let Err(e) = show_prompt(&app) { eprintln!("Zoom meeting prompt: {e}"); }
                        }
                    }
                    Some(false) => dismiss(&app),
                    _ => {}
                }
            }
        });
    }

    #[cfg(test)]
    #[tokio::test]
    #[ignore]
    async fn coreaudio_delivers_process_changes_without_polling() {
        let (tx, mut rx) = mpsc::channel(1);
        EVENTS.set(tx).unwrap();
        assert_eq!(listen(SYSTEM, kAudioHardwarePropertyProcessObjectList, true), 0);
        assert!(processes().is_ok());
        let mut child = tokio::process::Command::new("/usr/bin/afplay")
            .args(["-v", "0", "-t", "0.1", "/System/Library/Sounds/Tink.aiff"])
            .kill_on_drop(true).spawn().unwrap();
        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;
        assert!(child.wait().await.unwrap().success());
        assert_eq!(listen(SYSTEM, kAudioHardwarePropertyProcessObjectList, false), 0);
        assert!(event.unwrap().is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brief_input_does_not_prompt_and_a_sustained_call_prompts_once() {
        let now = Instant::now();
        let mut call = CallState::default();
        assert_eq!(call.update(true, now), None);
        assert_eq!(call.update(false, now + Duration::from_secs(1)), None);
        assert_eq!(call.deadline, None);
        assert_eq!(call.update(true, now + Duration::from_secs(2)), None);
        assert_eq!(call.update(true, now + Duration::from_secs(5)), Some(true));
        assert_eq!(call.update(true, now + Duration::from_secs(60)), None);
        assert_eq!(call.deadline, None);
    }

    #[test]
    fn a_brief_mute_does_not_repeat_the_prompt_but_a_new_call_does() {
        let now = Instant::now();
        let mut call = CallState::default();
        call.update(true, now);
        assert_eq!(call.update(true, now + START_DELAY), Some(true));
        call.update(false, now + Duration::from_secs(4));
        assert_eq!(call.update(true, now + Duration::from_secs(6)), None);
        assert_eq!(call.deadline, None);
        call.update(false, now + Duration::from_secs(7));
        assert_eq!(call.update(false, now + Duration::from_secs(27)), Some(false));
        call.update(true, now + Duration::from_secs(28));
        assert_eq!(call.update(true, now + Duration::from_secs(31)), Some(true));
    }
}
