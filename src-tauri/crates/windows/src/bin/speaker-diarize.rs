#[cfg(windows)]
fn main() {
    if let Err(error) = humla_windows::diarize::main() {
        eprintln!("humla-error: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {}
