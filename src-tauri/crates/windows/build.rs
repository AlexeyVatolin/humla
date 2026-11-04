use std::{env, fs, path::PathBuf};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }
    println!("cargo:rerun-if-env-changed=SHERPA_ONNX_LIB_DIR");
    let libraries = PathBuf::from(env::var_os("SHERPA_ONNX_LIB_DIR").expect("Build the Windows runtime first"));
    println!("cargo:rerun-if-changed={}", libraries.display());
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let profile = output.ancestors().nth(3).expect("Cargo profile directory");
    for directory in [profile.to_path_buf(), profile.join("deps")] {
        fs::create_dir_all(&directory).unwrap();
        for file in fs::read_dir(&libraries).unwrap() {
            let file = file.unwrap().path();
            if file.extension().is_some_and(|extension| extension == "dll") {
                fs::copy(&file, directory.join(file.file_name().unwrap())).unwrap();
            }
        }
    }
}
