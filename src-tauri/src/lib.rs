mod binaries;
mod concat;
mod cover;
mod merge;
mod plan;
mod preflight;
mod probe;
mod scan;
mod transcode;
mod types;
mod util;

#[cfg(test)]
mod tests;

use binaries::check_ffmpeg;
use cover::{get_cover_art, set_custom_cover_art};
use merge::{cancel_merge, merge_audiobook};
use plan::get_merge_plan;
use preflight::preflight_check;
use probe::probe_files;
use scan::resolve_audio_paths;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            preflight_check,
            probe_files,
            get_cover_art,
            set_custom_cover_art,
            get_merge_plan,
            merge_audiobook,
            cancel_merge,
            resolve_audio_paths,
            check_ffmpeg,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
