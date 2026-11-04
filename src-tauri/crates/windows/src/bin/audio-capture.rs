#[cfg(target_os = "windows")]
fn main() {
    if let Err(error) = humla_windows::capture::main() {
        eprintln!("humla-error: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "windows"))]
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag| {
        args.windows(2)
            .find(|pair| pair[0] == flag)
            .map(|pair| std::path::Path::new(&pair[1]))
    };
    humla_windows::import::replay(
        value("--import").ok_or_else(|| anyhow::anyhow!("--import is required"))?,
        value("--out").ok_or_else(|| anyhow::anyhow!("--out is required"))?,
        || false,
    )
}
