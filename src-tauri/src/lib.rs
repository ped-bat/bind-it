use serde::{Deserialize, Serialize};
use std::process::Command;

/// Audio file metadata extracted via ffprobe
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AudioFileInfo {
    pub path: String,
    pub codec: String,
    pub duration: f64,
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate: Option<u64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

/// Probe a single audio file using ffprobe and return structured metadata
#[tauri::command]
fn probe_file(path: String) -> Result<AudioFileInfo, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            &path,
        ])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe output: {}", e))?;

    // Find the audio stream
    let streams = json["streams"]
        .as_array()
        .ok_or("No streams found")?;

    let audio_stream = streams
        .iter()
        .find(|s| s["codec_type"].as_str() == Some("audio"))
        .ok_or("No audio stream found")?;

    let format = &json["format"];

    // Extract metadata from format tags
    let tags = &format["tags"];

    let codec = audio_stream["codec_name"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    let duration = format["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);

    let sample_rate = audio_stream["sample_rate"]
        .as_str()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let channels = audio_stream["channels"]
        .as_u64()
        .unwrap_or(0) as u32;

    let bitrate = format["bit_rate"]
        .as_str()
        .and_then(|b| b.parse::<u64>().ok());

    let title = tags["title"]
        .as_str()
        .or_else(|| tags["TITLE"].as_str())
        .map(|s| s.to_string());

    let artist = tags["artist"]
        .as_str()
        .or_else(|| tags["ARTIST"].as_str())
        .map(|s| s.to_string());

    let album = tags["album"]
        .as_str()
        .or_else(|| tags["ALBUM"].as_str())
        .map(|s| s.to_string());

    Ok(AudioFileInfo {
        path,
        codec,
        duration,
        sample_rate,
        channels,
        bitrate,
        title,
        artist,
        album,
    })
}

/// Health check — verify ffprobe is available
#[tauri::command]
fn check_ffmpeg() -> Result<String, String> {
    let output = Command::new("ffprobe")
        .arg("-version")
        .output()
        .map_err(|e| format!("ffprobe not found: {}", e))?;

    let version = String::from_utf8_lossy(&output.stdout);
    let first_line = version.lines().next().unwrap_or("unknown");
    Ok(first_line.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![probe_file, check_ffmpeg])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
