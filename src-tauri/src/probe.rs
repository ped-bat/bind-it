use crate::binaries::ffprobe;
use crate::types::{AudioFileInfo, ProbeResult};
use crate::util::clean_chapter_name;
use rayon::prelude::*;
use std::fs;
use std::path::Path;

pub fn probe_all_files(paths: Vec<String>) -> Result<Vec<AudioFileInfo>, String> {
    let results: Vec<Result<AudioFileInfo, String>> = paths
        .par_iter()
        .map(|path| probe_single_file(path))
        .collect();

    results.into_iter().collect()
}

#[tauri::command]
pub fn probe_files(paths: Vec<String>) -> Result<ProbeResult, String> {
    let mut files = Vec::new();
    let mut warnings = Vec::new();

    let mut valid_paths = Vec::new();
    for path in &paths {
        match fs::metadata(path) {
            Ok(meta) if meta.len() == 0 => {
                let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
                warnings.push(format!("Skipped 0-byte file: {}", name));
            }
            Err(e) => {
                let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
                warnings.push(format!("Cannot read {}: {}", name, e));
            }
            _ => valid_paths.push(path.clone()),
        }
    }

    let results: Vec<(String, Result<AudioFileInfo, String>)> = valid_paths
        .par_iter()
        .map(|path| (path.clone(), probe_single_file(path)))
        .collect();

    for (path, result) in results {
        match result {
            Ok(info) => files.push(info),
            Err(e) => {
                let name = Path::new(&path).file_name().and_then(|n| n.to_str()).unwrap_or(&path);
                warnings.push(format!("Skipped {}: {}", name, e));
            }
        }
    }

    if files.is_empty() && !warnings.is_empty() {
        return Err(format!("No valid audio files found. {}", warnings.join("; ")));
    }

    Ok(ProbeResult { files, warnings })
}

pub fn probe_single_file(path: &str) -> Result<AudioFileInfo, String> {
    let output = ffprobe()
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            path,
        ])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe failed for {}: {}",
            path,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe output: {}", e))?;

    let streams = json["streams"].as_array().ok_or("No streams found")?;
    let audio_stream = streams
        .iter()
        .find(|s| s["codec_type"].as_str() == Some("audio"))
        .ok_or("No audio stream found")?;

    let format = &json["format"];
    let tags = &format["tags"];

    let codec_name = audio_stream["codec_name"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    let codec = match codec_name.as_str() {
        "aac" => "aac".to_string(),
        "mp3" | "mp3float" => "mp3".to_string(),
        "flac" => "flac".to_string(),
        "alac" => "alac".to_string(),
        "pcm_s16le" | "pcm_s24le" | "pcm_s32le" | "pcm_f32le" | "pcm_f64le"
        | "pcm_u8" | "pcm_s16be" | "pcm_s24be" | "pcm_s32be" => "wav".to_string(),
        "wmav1" | "wmav2" | "wmapro" | "wmalossless" => "wma".to_string(),
        other => other.to_string(),
    };

    let duration = format["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);

    let sample_rate = audio_stream["sample_rate"]
        .as_str()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let channels = audio_stream["channels"].as_u64().unwrap_or(0) as u32;

    let bitrate = format["bit_rate"]
        .as_str()
        .and_then(|b| b.parse::<u64>().ok());

    // ffmpeg/ffprobe normalizes tag case inconsistently across containers and
    // demuxers (MP3 ID3 → lower, MP4 → mixed, etc.). Match case-insensitively
    // by scanning the tags object once instead of guessing common variants.
    let get_tag = |key: &str| -> Option<String> {
        let target = key.to_lowercase();
        tags.as_object()?.iter().find_map(|(k, v)| {
            if k.to_lowercase() == target {
                v.as_str().map(|s| s.to_string())
            } else {
                None
            }
        })
    };

    let filename = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let chapter_name = clean_chapter_name(&filename);

    let file_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    Ok(AudioFileInfo {
        path: path.to_string(),
        filename,
        chapter_name,
        codec,
        duration,
        sample_rate,
        channels,
        bitrate,
        title: get_tag("title"),
        artist: get_tag("artist"),
        album: get_tag("album"),
        narrator: get_tag("narrator")
            .or_else(|| get_tag("composer"))
            .or_else(|| get_tag("album_artist")),
        year: get_tag("date").or_else(|| get_tag("year")),
        file_size,
    })
}
