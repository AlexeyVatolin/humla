//! macOS permission commands: the microphone and screen recording through the
//! audio-capture sidecar, notifications in this process
//! (`crate::notifications`), plus a deep-link into System Settings.

#[cfg(not(target_os = "windows"))]
use std::process::Stdio;
use tauri::AppHandle;

#[cfg(target_os = "macos")]
use crate::notifications;

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct PermissionsStatus {
    pub microphone: String,
    pub screen: String,
    /// Not in the sidecar's answer.
    #[serde(default)]
    pub notifications: String,
}

#[cfg(not(target_os = "windows"))]
async fn run_sidecar_cmd(app: &AppHandle, mode: &str) -> Result<String, String> {
    let path = super::sidecar_path(app)?;
    let child = tokio::process::Command::new(&path)
        .arg(mode)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;
    let fut = child.wait_with_output();
    match tokio::time::timeout(std::time::Duration::from_secs(3), fut).await {
        Ok(Ok(out)) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
        Ok(Err(e)) => Err(format!("read: {e}")),
        Err(_) => Err("sidecar timed out".into()),
    }
}

pub(crate) async fn capture_permissions(app: &AppHandle) -> Result<PermissionsStatus, String> {
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        Ok(PermissionsStatus {
            microphone: humla_windows::permissions::microphone().into(),
            screen: "not_required".into(),
            notifications: "not_required".into(),
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let stdout = run_sidecar_cmd(app, "status").await?;
        let line = stdout.lines().last().unwrap_or("");
        serde_json::from_str(line).map_err(|e| format!("parse: {e}: {line}"))
    }
}

/// A failed sidecar reads as `unknown` for its two, so the notifications
/// answer still arrives.
#[tauri::command]
pub async fn permissions_status(app: AppHandle) -> Result<PermissionsStatus, String> {
    let status = capture_permissions(&app).await.unwrap_or_else(|_| PermissionsStatus {
        microphone: "unknown".into(),
        screen: "unknown".into(),
        ..Default::default()
    });
    #[cfg(target_os = "macos")]
    {
        let mut status = status;
        status.notifications = notifications::status().await.label().to_string();
        Ok(status)
    }
    #[cfg(not(target_os = "macos"))]
    Ok(status)
}

#[tauri::command]
pub async fn permissions_request(app: AppHandle, kind: String) -> Result<PermissionsStatus, String> {
    #[cfg(target_os = "windows")]
    {
        if kind == "microphone" {
            humla_windows::permissions::open_settings(&kind).map_err(|e| e.to_string())?;
        } else if kind != "screen" {
            return Err("unknown kind".into());
        }
        return permissions_status(app).await;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mode = match kind.as_str() {
            "microphone" => "request-microphone",
            "screen" => "request-screen",
            #[cfg(target_os = "macos")]
            "notifications" => {
                notifications::request().await;
                return permissions_status(app).await;
            }
            _ => return Err("unknown kind".into()),
        };
        let _ = run_sidecar_cmd(&app, mode).await; // result ignored; we re-query
        permissions_status(app).await
    }
}

#[tauri::command]
pub async fn permissions_open_settings(kind: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    return humla_windows::permissions::open_settings(&kind).map_err(|e| e.to_string());
    #[cfg(not(target_os = "windows"))]
    {
        let url = match kind.as_str() {
            "microphone" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
            "screen" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            "notifications" => "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=no.humla.app",
            _ => return Err("unknown kind".into()),
        };
        tokio::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("open: {e}"))?
            .wait()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
