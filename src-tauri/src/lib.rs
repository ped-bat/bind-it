pub mod binaries;
pub mod concat;
pub mod cover;
pub mod merge;
pub mod plan;
pub mod preflight;
pub mod probe;
pub mod scan;
pub mod transcode;
pub mod types;
pub mod util;

#[cfg(test)]
mod tests;

use binaries::check_ffmpeg;
use cover::{get_cover_art, set_custom_cover_art};
use merge::{cancel_merge, merge_audio_files};
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
            merge_audio_files,
            cancel_merge,
            resolve_audio_paths,
            check_ffmpeg,
        ])
        // `_app` rather than `app`: the only consumer is the macOS menu
        // installer, so on Windows and Linux the binding is compiled out and
        // an un-prefixed name trips `unused_variables`.
        .setup(|_app| {
            #[cfg(target_os = "macos")]
            install_macos_menu(_app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(target_os = "macos")]
fn install_macos_menu(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{AboutMetadataBuilder, MenuBuilder, PredefinedMenuItem, SubmenuBuilder};

    // Custom About panel: app icon (auto-pulled from the bundle), version,
    // and author with website. The app menu's other entries (Hide / Quit /
    // Services …) are kept as macOS standards.
    let about_metadata = AboutMetadataBuilder::new()
        .name(Some("Bind it"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .authors(Some(vec!["Pedro Batista (pedbat.com)".into()]))
        .website(Some("https://pedbat.com"))
        .website_label(Some("pedbat.com"))
        .copyright(Some(format!("© {} Pedro Batista", chrono_year())))
        .build();

    let about_item = PredefinedMenuItem::about(app, Some("About Bind it"), Some(about_metadata))?;
    let separator1 = PredefinedMenuItem::separator(app)?;
    let services = PredefinedMenuItem::services(app, None)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let hide = PredefinedMenuItem::hide(app, None)?;
    let hide_others = PredefinedMenuItem::hide_others(app, None)?;
    let show_all = PredefinedMenuItem::show_all(app, None)?;
    let separator3 = PredefinedMenuItem::separator(app)?;
    let quit = PredefinedMenuItem::quit(app, None)?;

    let app_submenu = SubmenuBuilder::new(app, "Bind it")
        .item(&about_item)
        .item(&separator1)
        .item(&services)
        .item(&separator2)
        .item(&hide)
        .item(&hide_others)
        .item(&show_all)
        .item(&separator3)
        .item(&quit)
        .build()?;

    let edit_submenu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let window_submenu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .build()?;

    let menu = MenuBuilder::new(app)
        .items(&[&app_submenu, &edit_submenu, &window_submenu])
        .build()?;

    app.set_menu(menu)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn chrono_year() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    // Approximate year from epoch seconds without pulling chrono:
    // 1970 + floor(seconds / (365.25 * 86400))
    1970 + (secs / 31_557_600) as i32
}
