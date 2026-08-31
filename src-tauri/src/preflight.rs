use crate::binaries::{ffmpeg, ffprobe};
use crate::types::PreflightResult;
use std::fs;
use std::path::Path;

#[tauri::command]
pub fn preflight_check(
    files: Vec<String>,
    output_dir: String,
    output_filename: Option<String>,
    output_extension: Option<String>,
) -> PreflightResult {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    for path in &files {
        let p = Path::new(path);
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or(path);
        if !p.exists() {
            errors.push(format!("Source file was moved or deleted: {}. Re-add your files and try again.", name));
        } else if fs::metadata(p).is_err() {
            errors.push(format!("Could not read {}. The file may be corrupted.", name));
        }
    }

    let out = Path::new(&output_dir);
    if !out.exists() {
        match fs::create_dir_all(out) {
            Ok(_) => {}
            Err(e) => {
                let msg = format!("{}", e);
                if msg.contains("Permission denied") || msg.contains("EACCES") {
                    errors.push("Permission denied — choose a different output folder or check folder permissions.".to_string());
                } else {
                    errors.push(format!("Cannot create output directory: {}", msg));
                }
            }
        }
    }
    if out.exists() {
        let test_file = out.join(".bind_it_write_test");
        match fs::File::create(&test_file) {
            Ok(_) => {
                let _ = fs::remove_file(&test_file);
            }
            Err(e) => {
                let msg = format!("{}", e);
                if msg.contains("Permission denied") || msg.contains("EACCES") {
                    errors.push("Permission denied — choose a different output folder or check folder permissions.".to_string());
                } else if msg.contains("No space left") || msg.contains("ENOSPC") {
                    errors.push("Not enough disk space. Free up space or choose a different drive.".to_string());
                } else {
                    errors.push(format!("Output folder is not writable: {}", msg));
                }
            }
        }
    }

    if ffmpeg().arg("-version").output().is_err() || ffprobe().arg("-version").output().is_err() {
        errors.push(crate::binaries::ffmpeg_install_hint().to_string());
    }

    if let Some(ref name) = output_filename {
        let ext = output_extension.as_deref().unwrap_or("m4b");
        let candidate = Path::new(&output_dir).join(format!("{}.{}", name, ext));
        if candidate.exists() {
            warnings.push(format!("{}.{} already exists — a numbered suffix will be added.", name, ext));
        }
    }

    PreflightResult {
        ok: errors.is_empty(),
        warnings,
        errors,
    }
}
