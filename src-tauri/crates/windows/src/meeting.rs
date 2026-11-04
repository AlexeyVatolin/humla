use anyhow::Result;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::Path;
use std::sync::mpsc::{sync_channel, SyncSender};
use windows::core::{implement, Interface, Ref, BOOL, GUID, PCWSTR, PWSTR};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

#[implement(IAudioSessionNotification)]
struct Created(SyncSender<()>);
impl IAudioSessionNotification_Impl for Created_Impl {
    fn OnSessionCreated(&self, _: Ref<IAudioSessionControl>) -> windows::core::Result<()> {
        let _ = self.0.try_send(());
        Ok(())
    }
}

#[implement(IAudioSessionEvents)]
struct Changed(SyncSender<()>);
#[allow(non_snake_case)]
impl IAudioSessionEvents_Impl for Changed_Impl {
    fn OnDisplayNameChanged(&self, _: &PCWSTR, _: *const GUID) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnIconPathChanged(&self, _: &PCWSTR, _: *const GUID) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnSimpleVolumeChanged(&self, _: f32, _: BOOL, _: *const GUID) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnChannelVolumeChanged(
        &self,
        _: u32,
        _: *const f32,
        _: u32,
        _: *const GUID,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnGroupingParamChanged(&self, _: *const GUID, _: *const GUID) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnStateChanged(&self, _: AudioSessionState) -> windows::core::Result<()> {
        let _ = self.0.try_send(());
        Ok(())
    }
    fn OnSessionDisconnected(&self, _: AudioSessionDisconnectReason) -> windows::core::Result<()> {
        let _ = self.0.try_send(());
        Ok(())
    }
}

struct Watch {
    manager: IAudioSessionManager2,
    created: IAudioSessionNotification,
    sessions: Vec<(IAudioSessionControl, IAudioSessionEvents)>,
}
impl Drop for Watch {
    fn drop(&mut self) {
        unsafe {
            let _ = self.manager.UnregisterSessionNotification(&self.created);
            for (session, callback) in &self.sessions {
                let _ = session.UnregisterAudioSessionNotification(callback);
            }
        }
    }
}

fn is_zoom(session: &IAudioSessionControl) -> bool {
    let Ok(control) = session.cast::<IAudioSessionControl2>() else {
        return false;
    };
    unsafe {
        let Ok(pid) = control.GetProcessId() else {
            return false;
        };
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let _owned = OwnedHandle::from_raw_handle(handle.0);
        let mut path = [0u16; 4096];
        let mut size = path.len() as u32;
        if QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut size,
        )
        .is_err()
        {
            return false;
        }
        Path::new(&String::from_utf16_lossy(&path[..size as usize]))
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("Zoom.exe"))
    }
}

fn scan(enumerator: &IMMDeviceEnumerator, sender: &SyncSender<()>) -> Result<(Vec<Watch>, bool)> {
    let mut watched = Vec::new();
    let mut active = false;
    unsafe {
        let devices = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
        for index in 0..devices.GetCount()? {
            let device = devices.Item(index)?;
            let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                continue;
            };
            let created: IAudioSessionNotification = Created(sender.clone()).into();
            manager.RegisterSessionNotification(&created)?;
            let mut watch = Watch {
                manager,
                created,
                sessions: Vec::new(),
            };
            let sessions = watch.manager.GetSessionEnumerator()?;
            for index in 0..sessions.GetCount()? {
                let session = sessions.GetSession(index)?;
                if !is_zoom(&session) {
                    continue;
                }
                let changed: IAudioSessionEvents = Changed(sender.clone()).into();
                session.RegisterAudioSessionNotification(&changed)?;
                active |= session.GetState()? == AudioSessionStateActive;
                watch.sessions.push((session, changed));
            }
            watched.push(watch);
        }
    }
    Ok((watched, active))
}

pub fn watch(mut changed: impl FnMut(bool) + Send + 'static) {
    std::thread::spawn(move || {
        let result = (|| -> Result<()> {
            wasapi::initialize_mta().ok()?;
            let enumerator: IMMDeviceEnumerator =
                unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
            let (sender, receiver) = sync_channel(1);
            let devices = wasapi::DeviceEnumerator::new()?;
            let mut callbacks = wasapi::DeviceEventCallbacks::new();
            let notify = sender.clone();
            callbacks.set_device_added_callback(move |_| {
                let _ = notify.try_send(());
            });
            let notify = sender.clone();
            callbacks.set_device_removed_callback(move |_| {
                let _ = notify.try_send(());
            });
            let notify = sender.clone();
            callbacks.set_device_state_callback(move |_, _| {
                let _ = notify.try_send(());
            });
            let notify = sender.clone();
            callbacks.set_default_device_callback(move |_, _, _| {
                let _ = notify.try_send(());
            });
            let _registration = devices.register_notification_callback(callbacks)?;
            let (mut subscriptions, mut active) = scan(&enumerator, &sender)?;
            changed(active);
            while receiver.recv().is_ok() {
                match scan(&enumerator, &sender) {
                    Ok((next, value)) => {
                        subscriptions = next;
                        if active != value {
                            active = value;
                            changed(active);
                        }
                    }
                    Err(error) => eprintln!("Zoom audio monitoring: {error:#}"),
                }
            }
            drop(subscriptions);
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("Zoom audio monitoring unavailable: {error:#}");
        }
    });
}
