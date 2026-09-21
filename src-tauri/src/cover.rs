use crate::binaries::{ffmpeg, ffprobe};
use crate::types::CoverArtResult;
use base64::Engine;
use std::fs;
use std::path::{Path, PathBuf};

static LAST_EXTRACTED_COVER: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Remove the cover ffmpeg extracted into the system temp dir, if any.
pub fn cleanup_extracted_cover() {
    if let Ok(mut prev) = LAST_EXTRACTED_COVER.lock() {
        if let Some(old_path) = prev.take() {
            let _ = fs::remove_file(&old_path);
        }
    }
}

/// A cover is only usable if ffmpeg sees a JPEG or PNG picture in it. A
/// 0-byte file, a WebP renamed to .jpg or a stray text file would otherwise
/// fail the bind at the very last step with an opaque ffmpeg error.
pub fn validate_cover_image(path: &str) -> Result<(), String> {
    let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
    let bad = || format!("Cover image {} isn't a readable JPEG or PNG — pick a different image.", name);
    // ffprobe trusts the extension (garbage named .jpg probes as "mjpeg"), so
    // the codec check alone is not enough: actually decode one frame.
    let decode = ffmpeg()
        .args(["-v", "error", "-i", path, "-frames:v", "1", "-f", "null", "-"])
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;
    if !decode.status.success() || !decode.stderr.is_empty() {
        return Err(bad());
    }
    let output = ffprobe()
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=codec_name",
            "-of", "csv=p=0",
            path,
        ])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;
    let codec = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() && matches!(codec.as_str(), "mjpeg" | "png") {
        Ok(())
    } else {
        Err(bad())
    }
}

/// Folder covers, case-insensitively (Linux filesystems are case-sensitive,
/// so "Cover.jpg" was found on macOS and Windows but not there).
fn find_folder_cover(parent: &Path) -> Option<PathBuf> {
    const NAMES: [&str; 6] = ["cover.jpg", "cover.jpeg", "cover.png", "folder.jpg", "folder.jpeg", "folder.png"];
    let entries: Vec<PathBuf> = fs::read_dir(parent).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    for wanted in NAMES {
        if let Some(p) = entries.iter().find(|p| {
            p.file_name().and_then(|n| n.to_str()).map(|n| n.eq_ignore_ascii_case(wanted)).unwrap_or(false)
        }) {
            if p.is_file() && p.to_str().map(validate_cover_image).map(|r| r.is_ok()).unwrap_or(false) {
                return Some(p.clone());
            }
        }
    }
    None
}

#[tauri::command]
pub fn get_cover_art(paths: Vec<String>) -> Option<CoverArtResult> {
    cleanup_extracted_cover();

    if paths.is_empty() {
        return None;
    }

    if let Some(cover_path) = Path::new(&paths[0]).parent().and_then(find_folder_cover) {
        if let Ok(data) = fs::read(&cover_path) {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
            let ext = cover_path.extension().and_then(|e| e.to_str()).unwrap_or("jpg").to_ascii_lowercase();
            let mime = if ext == "png" { "image/png" } else { "image/jpeg" };
            return Some(CoverArtResult {
                data_uri: format!("data:{};base64,{}", mime, b64),
                file_path: cover_path.to_str().unwrap_or("").to_string(),
            });
        }
    }

    let tmp = match tempfile::Builder::new()
        .prefix("bind_it_cover_")
        .suffix(".jpg")
        .tempfile()
    {
        Ok(f) => match f.keep() {
            Ok((_, path)) => path,
            Err(_) => return None,
        },
        Err(_) => return None,
    };

    let tmp_str = match tmp.to_str() {
        Some(s) => s,
        None => { let _ = fs::remove_file(&tmp); return None; }
    };
    let result = ffmpeg()
        .args([
            "-y", "-i", &paths[0],
            "-an", "-vcodec", "copy",
            tmp_str,
        ])
        .output();

    if let Ok(output) = result {
        if output.status.success() && tmp.exists() && tmp.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            if let Ok(data) = fs::read(&tmp) {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                if let Ok(mut prev) = LAST_EXTRACTED_COVER.lock() {
                    *prev = Some(tmp.clone());
                }
                return Some(CoverArtResult {
                    data_uri: format!("data:image/jpeg;base64,{}", b64),
                    file_path: tmp.to_str().unwrap_or("").to_string(),
                });
            }
        }
    }
    let _ = fs::remove_file(&tmp);

    None
}

#[tauri::command]
pub fn set_custom_cover_art(path: String) -> Result<CoverArtResult, String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err("File does not exist".to_string());
    }

    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if !["jpg", "jpeg", "png"].contains(&ext.as_str()) {
        return Err("Unsupported image format. Please select a JPG or PNG file.".to_string());
    }
    validate_cover_image(&path)?;

    let data = fs::read(p).map_err(|e| format!("Failed to read image: {}", e))?;
    let mime = if ext == "png" { "image/png" } else { "image/jpeg" };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

    Ok(CoverArtResult {
        data_uri: format!("data:{};base64,{}", mime, b64),
        file_path: path,
    })
}
