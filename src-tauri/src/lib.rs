use base64::Engine;
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tauri::Emitter;

// ── Global state ─────────────────────────────────────────────────────────────

static CANCEL_FLAG: AtomicBool = AtomicBool::new(false);
static IS_CONVERTING: AtomicBool = AtomicBool::new(false);

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

// ── Preflight check ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PreflightResult {
    pub ok: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[tauri::command]
fn preflight_check(files: Vec<String>, output_dir: String) -> PreflightResult {
    let warnings = Vec::new();
    let mut errors = Vec::new();

    // Check all input files still exist and are readable
    for path in &files {
        let p = Path::new(path);
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or(path);
        if !p.exists() {
            errors.push(format!("Source file was moved or deleted: {}. Re-add your files and try again.", name));
        } else if fs::metadata(p).is_err() {
            errors.push(format!("Could not read {}. The file may be corrupted.", name));
        }
    }

    // Check output directory is writable
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
        let test_file = out.join(".bindery_write_test");
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

    // Check ffmpeg/ffprobe are available
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        errors.push("ffmpeg is required but not installed. Install it with: brew install ffmpeg".to_string());
    }
    if Command::new("ffprobe").arg("-version").output().is_err() {
        errors.push("ffprobe is required but not installed. Install it with: brew install ffmpeg".to_string());
    }

    PreflightResult {
        ok: errors.is_empty(),
        warnings,
        errors,
    }
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
    let results: Vec<Result<AudioFileInfo, String>> = paths
        .par_iter()
        .map(|path| probe_single_file(path))
        .collect();

    results.into_iter().collect()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProbeResult {
    pub files: Vec<AudioFileInfo>,
    pub warnings: Vec<String>,
}

#[tauri::command]
fn probe_files(paths: Vec<String>) -> Result<ProbeResult, String> {
    let mut files = Vec::new();
    let mut warnings = Vec::new();

    // Filter out 0-byte files before probing
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

    // Probe in parallel, collecting results
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

/// Returns (data_uri, file_path) — data_uri for UI display, file_path for embedding in output
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoverArtResult {
    pub data_uri: String,
    pub file_path: String,
}

#[tauri::command]
fn get_cover_art(paths: Vec<String>) -> Option<CoverArtResult> {
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
                    return Some(CoverArtResult {
                        data_uri: format!("data:{};base64,{}", mime, b64),
                        file_path: cover_path.to_str().unwrap_or("").to_string(),
                    });
                }
            }
        }
    }

    // 2. Extract embedded art from first file to a persistent temp location
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
                return Some(CoverArtResult {
                    data_uri: format!("data:image/jpeg;base64,{}", b64),
                    file_path: tmp.to_str().unwrap_or("").to_string(),
                });
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

// ── Parallel transcoding helper ─────────────────────────────────────────────

fn transcode_parallel<F>(
    items: &[(usize, String)],
    tmp_dir: &Path,
    bitrate: &str,
    channels: &str,
    sample_rate: Option<u32>,
    emit: &F,
    pct_start: f64,
    pct_end: f64,
) -> Result<Vec<PathBuf>, String>
where
    F: Fn(&str, f64, &str) + Sync,
{
    if items.is_empty() {
        return Ok(vec![]);
    }

    let max_threads = num_cpus::get().max(2) - 1;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(max_threads)
        .build()
        .map_err(|e| format!("Thread pool error: {}", e))?;

    let completed = AtomicUsize::new(0);
    let total = items.len();
    let pct_range = pct_end - pct_start;

    pool.install(|| {
        items
            .par_iter()
            .map(|(idx, path)| {
                if CANCEL_FLAG.load(Ordering::Relaxed) {
                    return Err("Cancelled by user".to_string());
                }

                let temp_aac = tmp_dir.join(format!("part_{:04}.m4a", idx));
                let temp_str = temp_aac.to_str().unwrap().to_string();

                let mut args = vec![
                    "-y".to_string(), "-i".to_string(), path.clone(),
                    "-c:a".to_string(), "aac".to_string(),
                    "-b:a".to_string(), bitrate.to_string(),
                    "-ac".to_string(), channels.to_string(),
                    "-threads".to_string(), "0".to_string(),
                    "-vn".to_string(),
                ];

                if let Some(sr) = sample_rate {
                    args.push("-ar".to_string());
                    args.push(sr.to_string());
                }

                args.push(temp_str);

                let output = Command::new("ffmpeg")
                    .args(&args)
                    .output()
                    .map_err(|e| format!("ffmpeg transcode failed: {}", e))?;

                if !output.status.success() {
                    return Err(format!(
                        "Transcode failed for {}: {}",
                        path,
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }

                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                emit(
                    "transcoding",
                    pct_start + (pct_range * done as f64 / total as f64),
                    &format!("Transcoded {} of {} files", done, total),
                );

                Ok(temp_aac)
            })
            .collect()
    })
}

// ── Unique filename helper ──────────────────────────────────────────────────

fn unique_output_path(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(format!("{}.m4b", filename));
    if !candidate.exists() {
        return candidate;
    }
    for i in 1..=999 {
        let candidate = dir.join(format!("{} ({}).m4b", filename, i));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{} (999).m4b", filename))
}

// ── Error categorization ────────────────────────────────────────────────────

fn categorize_error(err: &str) -> String {
    if err.contains("Permission denied") || err.contains("EACCES") {
        "Permission denied — choose a different output folder or check folder permissions.".to_string()
    } else if err.contains("No space left") || err.contains("Disk full") || err.contains("ENOSPC") {
        "Not enough disk space. Free up space or choose a different drive.".to_string()
    } else if err.contains("No such file or directory") {
        // Try to extract the filename from the error
        if let Some(pos) = err.find("No such file or directory") {
            let prefix = &err[..pos];
            if let Some(path_part) = prefix.rsplit(&[':', ' '][..]).find(|s| s.contains('/') || s.contains('.')) {
                let name = Path::new(path_part).file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path_part);
                return format!("Source file was moved or deleted: {}. Re-add your files and try again.", name);
            }
        }
        "A source file was moved or deleted. Re-add your files and try again.".to_string()
    } else if err.contains("Failed to run ffprobe") || err.contains("Failed to run ffmpeg")
        || err.contains("ffmpeg transcode failed") || err.contains("No such file or directory: ffmpeg")
        || err.contains("No such file or directory: ffprobe")
    {
        "ffmpeg is required but not installed. Install it with: brew install ffmpeg".to_string()
    } else if err.contains("No audio stream") || err.contains("Invalid data found")
        || err.contains("could not find codec") || err.contains("Invalid argument")
    {
        // Try to extract filename
        if let Some(start) = err.find("for ") {
            let rest = &err[start + 4..];
            let path_end = rest.find(':').unwrap_or(rest.len());
            let path = &rest[..path_end];
            let name = Path::new(path).file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path);
            return format!("Could not read {}. The file may be corrupted.", name);
        }
        "Could not read a source file. The file may be corrupted.".to_string()
    } else {
        format!("Conversion failed: {}", err)
    }
}

// ── Merge pipeline ──────────────────────────────────────────────────────────

/// Core merge logic, callable without Tauri. The `emit` closure receives progress updates.
pub fn merge_audiobook_core<F>(config: MergeConfig, emit: F) -> Result<String, String>
where
    F: Fn(&str, f64, &str) + Sync,
{
    emit("preparing", 0.0, "Analyzing files…");

    if CANCEL_FLAG.load(Ordering::Relaxed) {
        return Err("Cancelled by user".to_string());
    }

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

    // Ensure output directory exists
    let output_dir = PathBuf::from(&config.output_dir);
    if !output_dir.exists() {
        fs::create_dir_all(&output_dir)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    // Generate unique output filename if file already exists
    let output_path = unique_output_path(&output_dir, &config.output_filename);
    let output_str = output_path.to_str().ok_or("Invalid output path")?;

    let channels_arg = if config.mono { "1" } else { "2" };
    let bitrate_arg = format!("{}k", config.bitrate);

    if uniform_aac {
        // ── Path 1: Concat demuxer (no re-encoding) ────────────────────────
        emit("merging", 10.0, "Remuxing AAC files (no re-encoding)…");

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

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
        // ── Path 1b: All AAC but mismatched sample rates — parallel normalize
        let mut sr_counts = HashMap::new();
        for p in &probed {
            *sr_counts.entry(p.sample_rate).or_insert(0u32) += 1;
        }
        let target_sr = sr_counts.into_iter().max_by_key(|&(_, count)| count).unwrap().0;

        emit("transcoding", 10.0, "Normalizing sample rates…");

        let items: Vec<(usize, String)> = config.files.iter().enumerate()
            .map(|(i, f)| (i, f.path.clone()))
            .collect();

        let temp_aac_files = transcode_parallel(
            &items, tmp_dir.path(), &bitrate_arg, channels_arg,
            Some(target_sr), &emit, 10.0, 60.0,
        )?;

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
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
        // ── Path 2: Parallel transcode all MP3 to AAC ───────────────────────
        emit("transcoding", 10.0, "Transcoding MP3 files to AAC…");

        let items: Vec<(usize, String)> = config.files.iter().enumerate()
            .map(|(i, f)| (i, f.path.clone()))
            .collect();

        let temp_aac_files = transcode_parallel(
            &items, tmp_dir.path(), &bitrate_arg, channels_arg,
            None, &emit, 10.0, 60.0,
        )?;

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
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
        // ── Path 3: Mixed — parallel transcode non-AAC, then concat all ─────
        emit("transcoding", 10.0, "Transcoding non-AAC files…");

        let items: Vec<(usize, String)> = config.files.iter().enumerate()
            .filter(|(i, _)| probed[*i].codec != "aac")
            .map(|(i, f)| (i, f.path.clone()))
            .collect();

        let transcoded = transcode_parallel(
            &items, tmp_dir.path(), &bitrate_arg, channels_arg,
            None, &emit, 10.0, 50.0,
        )?;

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        // Build ordered list: original AAC files + transcoded non-AAC
        let mut transcode_map: HashMap<usize, PathBuf> =
            items.iter().map(|(idx, _)| *idx).zip(transcoded).collect();

        let mut all_paths: Vec<PathBuf> = Vec::new();
        for (i, file) in config.files.iter().enumerate() {
            if let Some(path) = transcode_map.remove(&i) {
                all_paths.push(path);
            } else {
                all_paths.push(PathBuf::from(&file.path));
            }
        }

        emit("merging", 55.0, "Concatenating all files…");
        let intermediate = concat_aac_files(&all_paths, tmp_dir.path())?;

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
fn merge_audiobook(app: tauri::AppHandle, config: MergeConfig) -> Result<(), String> {
    if IS_CONVERTING.swap(true, Ordering::SeqCst) {
        return Err("A conversion is already in progress".to_string());
    }
    CANCEL_FLAG.store(false, Ordering::SeqCst);

    std::thread::spawn(move || {
        let result = merge_audiobook_core(config, |stage, percent, message| {
            let _ = app.emit("merge-progress", MergeProgress {
                stage: stage.to_string(),
                percent,
                message: message.to_string(),
            });
        });

        match result {
            Ok(path) => {
                if CANCEL_FLAG.load(Ordering::Relaxed) {
                    let _ = app.emit("merge-cancelled", ());
                } else {
                    let _ = app.emit("merge-complete", path);
                }
            }
            Err(e) => {
                if e.contains("Cancelled") {
                    let _ = app.emit("merge-cancelled", ());
                } else {
                    let msg = categorize_error(&e);
                    let _ = app.emit("merge-error", msg);
                }
            }
        }

        IS_CONVERTING.store(false, Ordering::SeqCst);
    });

    Ok(())
}

#[tauri::command]
fn cancel_merge() {
    CANCEL_FLAG.store(true, Ordering::SeqCst);
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

// ── Path resolution (folder support) ────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResolvedPaths {
    pub paths: Vec<String>,
    pub folder_name: Option<String>,
}

#[tauri::command]
fn resolve_audio_paths(paths: Vec<String>) -> ResolvedPaths {
    let audio_exts = ["mp3", "m4a", "m4b", "aac"];

    // If a single directory was dropped, use its name as the suggested filename
    let folder_name = if paths.len() == 1 {
        let p = Path::new(&paths[0]);
        if p.is_dir() {
            p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
        } else {
            // Files from the same folder — use parent folder name
            p.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        }
    } else if paths.len() > 1 {
        // Multiple files — check if all share the same parent folder
        let parents: std::collections::HashSet<_> = paths.iter()
            .filter_map(|p| Path::new(p).parent().and_then(|pp| pp.to_str()))
            .collect();
        if parents.len() == 1 {
            let parent = Path::new(parents.into_iter().next().unwrap());
            parent.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
        } else {
            None
        }
    } else {
        None
    };

    let mut result = Vec::new();
    for path in &paths {
        let p = Path::new(path);
        if p.is_dir() {
            scan_dir_recursive(p, &audio_exts, &mut result);
        } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            if audio_exts.contains(&ext.to_lowercase().as_str()) {
                result.push(path.clone());
            }
        }
    }
    result.sort();

    ResolvedPaths { paths: result, folder_name }
}



fn scan_dir_recursive(dir: &Path, exts: &[&str], result: &mut Vec<String>) {
    if let Ok(entries) = fs::read_dir(dir) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                scan_dir_recursive(&path, exts, result);
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if exts.contains(&ext.to_lowercase().as_str()) {
                    if let Some(s) = path.to_str() {
                        result.push(s.to_string());
                    }
                }
            }
        }
    }
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
            preflight_check,
            probe_files,
            get_cover_art,
            get_merge_plan,
            merge_audiobook,
            cancel_merge,
            resolve_audio_paths,
            check_ffmpeg,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
