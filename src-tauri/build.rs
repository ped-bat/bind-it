fn main() {
    // Debug builds (cargo test, the dev CLI) run from target/debug/deps or
    // target/debug/examples, where tauri-build does not copy the sidecars;
    // binaries.rs uses this to find them in ./binaries by their triple name
    // so tests and the CLI exercise the ffmpeg that ships, not PATH's.
    println!(
        "cargo:rustc-env=BIND_IT_TARGET_TRIPLE={}",
        std::env::var("TARGET").unwrap_or_default()
    );
    tauri_build::build()
}
