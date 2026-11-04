#[cfg(target_os = "windows")]
pub mod capture;
#[cfg(target_os = "windows")]
pub mod diarize;
#[cfg(target_os = "windows")]
pub mod export;
pub mod import;
#[cfg(target_os = "windows")]
pub mod inference;
#[cfg(target_os = "windows")]
pub mod meeting;
#[cfg(target_os = "windows")]
pub mod models;
#[cfg(target_os = "windows")]
pub mod permissions;
pub mod wave;

pub fn emit(value: serde_json::Value) {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    if serde_json::to_writer(&mut stdout, &value).is_ok() {
        let _ = writeln!(stdout);
        let _ = stdout.flush();
    }
}
