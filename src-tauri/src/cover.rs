use crate::binaries::ffmpeg;
use crate::types::CoverArtResult;
use base64::Engine;
use std::fs;
use std::path::{Path, PathBuf};

static LAST_EXTRACTED_COVER: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[tauri::command]
pub fn get_cover_art(paths: Vec<String>) -> Option<CoverArtResult> {
    if let Ok(mut prev) = LAST_EXTRACTED_COVER.lock() {
        if let Some(old_path) = prev.take() {
            let _ = fs::remove_file(&old_path);
        }
    }

    if paths.is_empty() {
        return None;
    }

    if let Some(parent) = Path::new(&paths[0]).parent() {
        for name in &["cover.jpg", "cover.jpeg", "cover.png", "folder.jpg"] {
            let cover_path = parent.join(name);
            if cover_path.exists() {
                if let Ok(data) = fs::read(&cover_path) {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                    let ext = cover_path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
                    let mime = if ext == "png" { "image/png" } else { "image/jpeg" };
                    return Some(CoverArtResult {
                        data_uri: format!("data:{};base64,{}", mime, b64),
                        file_path: cover_path.to_str().unwrap_or("").to_string(),
                    });
                }
            }
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

    let data = fs::read(p).map_err(|e| format!("Failed to read image: {}", e))?;
    let mime = if ext == "png" { "image/png" } else { "image/jpeg" };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

    Ok(CoverArtResult {
        data_uri: format!("data:{};base64,{}", mime, b64),
        file_path: path,
    })
}
