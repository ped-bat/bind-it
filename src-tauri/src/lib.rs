use base64::Engine;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Emitter;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AudioFileInfo {
    pub path: String,
    pub filename: String,
    pub chapter_name: String,
    pub codec: String,
    pub duration: f64,
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate: Option<u64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub narrator: Option<String>,
    pub year: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MergeConfig {
    pub files: Vec<FileEntry>,
    pub output_dir: String,
    pub output_filename: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub narrator: Option<String>,
    pub year: Option<String>,
    pub cover_art_path: Option<String>,
    pub bitrate: u32,     // kbps
    pub mono: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileEntry {
    pub path: String,
    pub chapter_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MergeProgress {
    pub stage: String,
    pub percent: f64,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MergePlan {
    pub strategy: String, // "remux", "transcode_mp3", "transcode_mixed"
    pub needs_transcode: Vec<String>,
    pub total_duration: f64,
}

// ── Filename cleaner ────────────────────────────────────────────────────────

pub fn clean_chapter_name(filename: &str) -> String {
    // Strip extension
    let name = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);

    // Strip common numbering prefixes: "01 - ", "01. ", "Chapter 1 - ", "Part 01 - ", etc.
    let re = Regex::new(r"^(?:(?:Chapter|Part|Track|Section)\s*\d+\s*[-–—.]\s*|\d{1,3}\s*[-–—.]\s*)").unwrap();
    let cleaned = re.replace(name, "").to_string();

    let result = cleaned.trim().to_string();
    if result.is_empty() {
        name.trim().to_string()
    } else {
        result
    }
}

// ── Audio probing ───────────────────────────────────────────────────────────

pub fn probe_all_files(paths: Vec<String>) -> Result<Vec<AudioFileInfo>, String> {
    let mut results = Vec::new();
    for path in paths {
        results.push(probe_single_file(&path)?);
    }
    Ok(results)
}

#[tauri::command]
fn probe_files(paths: Vec<String>) -> Result<Vec<AudioFileInfo>, String> {
    probe_all_files(paths)
}

pub fn probe_single_file(path: &str) -> Result<AudioFileInfo, String> {
    let output = Command::new("ffprobe")
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

    // Normalize codec name
    let codec = match codec_name.as_str() {
        "aac" => "aac".to_string(),
        "mp3" | "mp3float" => "mp3".to_string(),
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

    let get_tag = |key: &str| -> Option<String> {
        tags[key]
            .as_str()
            .or_else(|| tags[key.to_uppercase().as_str()].as_str())
            .map(|s| s.to_string())
    };

    let filename = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let chapter_name = clean_chapter_name(&filename);

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
    })
}

// ── Cover art ───────────────────────────────────────────────────────────────

#[tauri::command]
fn get_cover_art(paths: Vec<String>) -> Option<String> {
    if paths.is_empty() {
        return None;
    }

    // 1. Check for cover.jpg / cover.png in source folder
    if let Some(parent) = Path::new(&paths[0]).parent() {
        for name in &["cover.jpg", "cover.jpeg", "cover.png", "folder.jpg"] {
            let cover_path = parent.join(name);
            if cover_path.exists() {
                if let Ok(data) = fs::read(&cover_path) {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                    let ext = cover_path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
                    let mime = if ext == "png" { "image/png" } else { "image/jpeg" };
                    return Some(format!("data:{};base64,{}", mime, b64));
                }
            }
        }
    }

    // 2. Extract embedded art from first file
    let tmp = std::env::temp_dir().join("bindery_cover.jpg");
    let result = Command::new("ffmpeg")
        .args([
            "-y", "-i", &paths[0],
            "-an", "-vcodec", "copy",
            tmp.to_str().unwrap(),
        ])
        .output();

    if let Ok(output) = result {
        if output.status.success() && tmp.exists() && tmp.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            if let Ok(data) = fs::read(&tmp) {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                let _ = fs::remove_file(&tmp);
                return Some(format!("data:image/jpeg;base64,{}", b64));
            }
        }
    }

    None
}

// ── Merge plan ──────────────────────────────────────────────────────────────

#[tauri::command]
fn get_merge_plan(paths: Vec<String>) -> Result<MergePlan, String> {
    let files = probe_all_files(paths)?;

    let all_aac = files.iter().all(|f| f.codec == "aac");
    let all_mp3 = files.iter().all(|f| f.codec == "mp3");
    let total_duration: f64 = files.iter().map(|f| f.duration).sum();

    if all_aac {
        // Check if sample rate and channels are uniform
        let sr = files[0].sample_rate;
        let ch = files[0].channels;
        let uniform = files.iter().all(|f| f.sample_rate == sr && f.channels == ch);

        if uniform {
            return Ok(MergePlan {
                strategy: "remux".to_string(),
                needs_transcode: vec![],
                total_duration,
            });
        }

        // All AAC but mismatched sample rates/channels — need to normalize
        return Ok(MergePlan {
            strategy: "transcode_aac".to_string(),
            needs_transcode: files.iter().map(|f| f.path.clone()).collect(),
            total_duration,
        });
    }

    if all_mp3 {
        return Ok(MergePlan {
            strategy: "transcode_mp3".to_string(),
            needs_transcode: files.iter().map(|f| f.path.clone()).collect(),
            total_duration,
        });
    }

    // Mixed
    let needs_transcode: Vec<String> = files
        .iter()
        .filter(|f| f.codec != "aac")
        .map(|f| f.path.clone())
        .collect();

    Ok(MergePlan {
        strategy: "transcode_mixed".to_string(),
        needs_transcode,
        total_duration,
    })
}

// ── Chapter metadata generation ─────────────────────────────────────────────

pub fn generate_ffmetadata(files: &[FileEntry], durations: &[f64], metadata: &MergeConfig) -> String {
    let mut meta = String::from(";FFMETADATA1\n");

    if let Some(ref t) = metadata.title {
        meta.push_str(&format!("title={}\n", t));
    }
    if let Some(ref a) = metadata.artist {
        meta.push_str(&format!("artist={}\n", a));
    }
    if let Some(ref al) = metadata.album {
        meta.push_str(&format!("album={}\n", al));
    }
    if let Some(ref n) = metadata.narrator {
        meta.push_str(&format!("composer={}\n", n));
    }
    if let Some(ref y) = metadata.year {
        meta.push_str(&format!("date={}\n", y));
    }
    meta.push('\n');

    let mut cumulative_ms: u64 = 0;
    for (i, file) in files.iter().enumerate() {
        let duration_ms = (durations[i] * 1000.0) as u64;
        meta.push_str("[CHAPTER]\n");
        meta.push_str("TIMEBASE=1/1000\n");
        meta.push_str(&format!("START={}\n", cumulative_ms));
        cumulative_ms += duration_ms;
        meta.push_str(&format!("END={}\n", cumulative_ms));
        meta.push_str(&format!("title={}\n", file.chapter_name));
        meta.push('\n');
    }

    meta
}

// ── Merge pipeline ──────────────────────────────────────────────────────────

/// Core merge logic, callable without Tauri. The `emit` closure receives progress updates.
pub fn merge_audiobook_core<F>(config: MergeConfig, emit: F) -> Result<String, String>
where
    F: Fn(&str, f64, &str),
{

    emit("preparing", 0.0, "Analyzing files…");

    // Probe all files to get durations
    let file_paths: Vec<String> = config.files.iter().map(|f| f.path.clone()).collect();
    let probed = probe_all_files(file_paths)?;
    let durations: Vec<f64> = probed.iter().map(|f| f.duration).collect();

    let all_aac = probed.iter().all(|f| f.codec == "aac");
    let all_mp3 = probed.iter().all(|f| f.codec == "mp3");
    let uniform_aac = all_aac && {
        let sr = probed[0].sample_rate;
        let ch = probed[0].channels;
        probed.iter().all(|f| f.sample_rate == sr && f.channels == ch)
    };

    let tmp_dir = tempfile::tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let output_path = PathBuf::from(&config.output_dir)
        .join(format!("{}.m4b", &config.output_filename));
    let output_str = output_path.to_str().ok_or("Invalid output path")?;

    let channels_arg = if config.mono { "1" } else { "2" };
    let bitrate_arg = format!("{}k", config.bitrate);

    if uniform_aac {
        // ── Path 1: Concat demuxer (no re-encoding) ────────────────────────
        emit("merging", 10.0, "Remuxing AAC files (no re-encoding)…");

        let concat_list = tmp_dir.path().join("concat.txt");
        let mut f = fs::File::create(&concat_list)
            .map_err(|e| format!("Failed to create concat list: {}", e))?;
        for file in &config.files {
            writeln!(f, "file '{}'", file.path.replace('\'', "'\\''"))
                .map_err(|e| format!("Failed to write concat list: {}", e))?;
        }

        // Concat to intermediate file
        let intermediate = tmp_dir.path().join("merged.m4a");
        let status = Command::new("ffmpeg")
            .args([
                "-y", "-f", "concat", "-safe", "0",
                "-i", concat_list.to_str().unwrap(),
                "-c", "copy",
                intermediate.to_str().unwrap(),
            ])
            .output()
            .map_err(|e| format!("ffmpeg concat failed: {}", e))?;

        if !status.status.success() {
            return Err(format!("ffmpeg concat failed: {}", String::from_utf8_lossy(&status.stderr)));
        }

        emit("chapters", 60.0, "Adding chapter metadata…");
        add_metadata_and_cover(
            intermediate.to_str().unwrap(),
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
        )?;

    } else if all_aac {
        // ── Path 1b: All AAC but mismatched sample rates — normalize ────────
        // Pick the most common sample rate as the target
        let mut sr_counts = std::collections::HashMap::new();
        for p in &probed {
            *sr_counts.entry(p.sample_rate).or_insert(0u32) += 1;
        }
        let target_sr = sr_counts.into_iter().max_by_key(|&(_, count)| count).unwrap().0;
        let target_sr_arg = target_sr.to_string();

        emit("transcoding", 10.0, "Normalizing sample rates…");

        let total = config.files.len();
        let mut temp_aac_files: Vec<PathBuf> = Vec::new();

        for (i, file) in config.files.iter().enumerate() {
            let pct = 10.0 + (50.0 * (i as f64) / total as f64);
            emit("transcoding", pct, &format!("Normalizing file {} of {}…", i + 1, total));

            let temp_aac = tmp_dir.path().join(format!("part_{:04}.m4a", i));
            let status = Command::new("ffmpeg")
                .args([
                    "-y", "-i", &file.path,
                    "-c:a", "aac", "-b:a", &bitrate_arg,
                    "-ar", &target_sr_arg,
                    "-ac", channels_arg,
                    "-vn",
                    temp_aac.to_str().unwrap(),
                ])
                .output()
                .map_err(|e| format!("ffmpeg transcode failed: {}", e))?;

            if !status.status.success() {
                return Err(format!(
                    "Transcode failed for {}: {}",
                    file.path,
                    String::from_utf8_lossy(&status.stderr)
                ));
            }
            temp_aac_files.push(temp_aac);
        }

        emit("merging", 60.0, "Concatenating normalized files…");
        let intermediate = concat_aac_files(&temp_aac_files, tmp_dir.path())?;

        emit("chapters", 80.0, "Adding chapter metadata…");
        add_metadata_and_cover(
            intermediate.to_str().unwrap(),
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
        )?;

    } else if all_mp3 {
        // ── Path 2: Transcode all MP3 to AAC ───────────────────────────────
        emit("transcoding", 10.0, "Transcoding MP3 files to AAC…");

        let total = config.files.len();
        let mut temp_aac_files: Vec<PathBuf> = Vec::new();

        for (i, file) in config.files.iter().enumerate() {
            let pct = 10.0 + (50.0 * (i as f64) / total as f64);
            emit("transcoding", pct, &format!("Transcoding file {} of {}…", i + 1, total));

            let temp_aac = tmp_dir.path().join(format!("part_{:04}.m4a", i));
            let status = Command::new("ffmpeg")
                .args([
                    "-y", "-i", &file.path,
                    "-c:a", "aac", "-b:a", &bitrate_arg,
                    "-ac", channels_arg,
                    "-vn",
                    temp_aac.to_str().unwrap(),
                ])
                .output()
                .map_err(|e| format!("ffmpeg transcode failed: {}", e))?;

            if !status.status.success() {
                return Err(format!(
                    "Transcode failed for {}: {}",
                    file.path,
                    String::from_utf8_lossy(&status.stderr)
                ));
            }
            temp_aac_files.push(temp_aac);
        }

        emit("merging", 60.0, "Concatenating transcoded files…");
        let intermediate = concat_aac_files(&temp_aac_files, tmp_dir.path())?;

        emit("chapters", 80.0, "Adding chapter metadata…");
        add_metadata_and_cover(
            intermediate.to_str().unwrap(),
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
        )?;

    } else {
        // ── Path 3: Mixed — transcode MP3s, then concat all ─────────────────
        emit("transcoding", 10.0, "Transcoding non-AAC files…");

        let mp3_count = probed.iter().filter(|f| f.codec != "aac").count();
        let mut mp3_idx = 0;
        let mut aac_paths: Vec<PathBuf> = Vec::new();

        for (i, file) in config.files.iter().enumerate() {
            if probed[i].codec == "aac" {
                aac_paths.push(PathBuf::from(&file.path));
            } else {
                let pct = 10.0 + (40.0 * (mp3_idx as f64) / mp3_count as f64);
                emit("transcoding", pct, &format!("Transcoding {} to AAC…", probed[i].filename));

                let temp_aac = tmp_dir.path().join(format!("part_{:04}.m4a", i));
                let status = Command::new("ffmpeg")
                    .args([
                        "-y", "-i", &file.path,
                        "-c:a", "aac", "-b:a", &bitrate_arg,
                        "-ac", channels_arg,
                        "-vn",
                        temp_aac.to_str().unwrap(),
                    ])
                    .output()
                    .map_err(|e| format!("ffmpeg transcode failed: {}", e))?;

                if !status.status.success() {
                    return Err(format!(
                        "Transcode failed for {}: {}",
                        file.path,
                        String::from_utf8_lossy(&status.stderr)
                    ));
                }
                aac_paths.push(temp_aac);
                mp3_idx += 1;
            }
        }

        emit("merging", 55.0, "Concatenating all files…");
        let intermediate = concat_aac_files(&aac_paths, tmp_dir.path())?;

        emit("chapters", 80.0, "Adding chapter metadata…");
        add_metadata_and_cover(
            intermediate.to_str().unwrap(),
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
        )?;
    }

    emit("done", 100.0, "Audiobook created successfully!");
    Ok(output_str.to_string())
}

#[tauri::command]
fn merge_audiobook(app: tauri::AppHandle, config: MergeConfig) -> Result<String, String> {
    merge_audiobook_core(config, |stage, percent, message| {
        let _ = app.emit("merge-progress", MergeProgress {
            stage: stage.to_string(),
            percent,
            message: message.to_string(),
        });
    })
}

pub fn concat_aac_files(files: &[PathBuf], tmp_dir: &Path) -> Result<PathBuf, String> {
    let concat_list = tmp_dir.join("concat.txt");
    let mut f = fs::File::create(&concat_list)
        .map_err(|e| format!("Failed to create concat list: {}", e))?;
    for path in files {
        writeln!(f, "file '{}'", path.to_str().unwrap().replace('\'', "'\\''"))
            .map_err(|e| format!("Failed to write concat list: {}", e))?;
    }

    let output = tmp_dir.join("merged.m4a");
    let status = Command::new("ffmpeg")
        .args([
            "-y", "-f", "concat", "-safe", "0",
            "-i", concat_list.to_str().unwrap(),
            "-c", "copy",
            output.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| format!("ffmpeg concat failed: {}", e))?;

    if !status.status.success() {
        return Err(format!(
            "ffmpeg concat failed: {}",
            String::from_utf8_lossy(&status.stderr)
        ));
    }

    Ok(output)
}

pub fn add_metadata_and_cover(
    input: &str,
    output: &str,
    config: &MergeConfig,
    durations: &[f64],
    tmp_dir: &Path,
) -> Result<(), String> {
    // Generate ffmetadata file
    let metadata_file = tmp_dir.join("ffmetadata.txt");
    let metadata_content = generate_ffmetadata(&config.files, durations, config);
    fs::write(&metadata_file, &metadata_content)
        .map_err(|e| format!("Failed to write metadata: {}", e))?;

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(), input.into(),
        "-i".into(), metadata_file.to_str().unwrap().into(),
        "-map_metadata".into(), "1".into(),
    ];

    // Handle cover art
    let has_cover = config.cover_art_path.as_ref().map_or(false, |p| {
        Path::new(p).exists()
    });

    if has_cover {
        let cover = config.cover_art_path.as_ref().unwrap();
        args.extend_from_slice(&[
            "-i".into(), cover.clone(),
            "-map".into(), "0:a".into(),
            "-map".into(), "2:v".into(),
            "-c:v".into(), "copy".into(),
            "-disposition:v:0".into(), "attached_pic".into(),
        ]);
    } else {
        args.extend_from_slice(&[
            "-map".into(), "0:a".into(),
        ]);
    }

    args.extend_from_slice(&[
        "-c:a".into(), "copy".into(),
        "-f".into(), "mp4".into(),
        output.into(),
    ]);

    let status = Command::new("ffmpeg")
        .args(&args)
        .output()
        .map_err(|e| format!("ffmpeg metadata failed: {}", e))?;

    if !status.status.success() {
        return Err(format!(
            "ffmpeg metadata failed: {}",
            String::from_utf8_lossy(&status.stderr)
        ));
    }

    Ok(())
}

// ── Health check ────────────────────────────────────────────────────────────

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

// ── App entry ───────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            probe_files,
            get_cover_art,
            get_merge_plan,
            merge_audiobook,
            check_ffmpeg,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
