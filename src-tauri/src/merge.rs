use crate::binaries::{aac_encoder, run_ffmpeg_with_progress, ConvertGuard, CANCEL_FLAG, IS_CONVERTING};
use crate::concat::{
    add_metadata_and_cover, concat_aac_files, set_chap_byte_offsets,
    strip_mp3_for_concat, OutputFormat,
};
use rayon::prelude::*;
use crate::probe::probe_all_files;
use crate::transcode::transcode_parallel;
use crate::types::{MergeConfig, MergeProgress, Stage};
use crate::util::{
    categorize_error, is_temp_path, path_str, unique_output_path,
    validate_concat_path, validate_filename,
};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::Emitter;

/// Core merge logic, callable without Tauri. The `emit` closure receives progress updates.
pub fn merge_audio_files_core<F>(config: MergeConfig, emit: F) -> Result<String, String>
where
    F: Fn(Stage, f64, &str) + Sync,
{
    emit(Stage::Preparing, 0.0, "Analyzing files");

    if config.files.is_empty() {
        return Err("No files to merge.".to_string());
    }
    validate_filename(&config.output_filename)?;
    for file in &config.files {
        validate_concat_path(&file.path)?;
    }

    if CANCEL_FLAG.load(Ordering::Relaxed) {
        return Err("Cancelled by user".to_string());
    }

    let file_paths: Vec<String> = config.files.iter().map(|f| f.path.clone()).collect();
    let (probed, durations) = if let Some(ref cached) = config.durations {
        if cached.len() == config.files.len() {
            let probed = probe_all_files(file_paths)?;
            if probed.is_empty() {
                return Err("No valid audio files to merge.".to_string());
            }
            (probed, cached.clone())
        } else {
            let probed = probe_all_files(file_paths)?;
            if probed.is_empty() {
                return Err("No valid audio files to merge.".to_string());
            }
            let durations: Vec<f64> = probed.iter().map(|f| f.duration).collect();
            (probed, durations)
        }
    } else {
        let probed = probe_all_files(file_paths)?;
        if probed.is_empty() {
            return Err("No valid audio files to merge.".to_string());
        }
        let durations: Vec<f64> = probed.iter().map(|f| f.duration).collect();
        (probed, durations)
    };

    let force = config.force_transcode;

    let all_aac = !force && probed.iter().all(|f| f.codec == "aac");
    let all_mp3 = !force && probed.iter().all(|f| f.codec == "mp3");
    let all_alac = !force && probed.iter().all(|f| f.codec == "alac");
    let uniform_aac = all_aac && {
        let sr = probed[0].sample_rate;
        let ch = probed[0].channels;
        probed.iter().all(|f| f.sample_rate == sr && f.channels == ch)
    };
    let uniform_mp3 = all_mp3 && {
        let sr = probed[0].sample_rate;
        let ch = probed[0].channels;
        probed.iter().all(|f| f.sample_rate == sr && f.channels == ch)
    };
    let uniform_alac = all_alac && {
        let sr = probed[0].sample_rate;
        let ch = probed[0].channels;
        probed.iter().all(|f| f.sample_rate == sr && f.channels == ch)
    };

    let tmp_dir = tempfile::tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let output_dir = PathBuf::from(&config.output_dir);
    if !output_dir.exists() {
        fs::create_dir_all(&output_dir)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    let want_alac = config.output_codec.as_deref() == Some("alac");

    // MP3 streams are only well-supported inside an MP3 container — Apple players
    // (Preview, QuickLook, iTunes/Music) refuse MP3 inside MP4/M4B even though
    // the spec allows it. By default we therefore output `.mp3` with ID3v2
    // chapters when remuxing MP3. The user can opt into MP3-in-M4B via
    // `wrap_in_mp4` (frontend's "Original wrapped in M4B" format) to get
    // Apple Books chapter UI at the cost of Preview playback.
    let output_format = if !want_alac && (uniform_mp3 || all_mp3) && !config.wrap_in_mp4 {
        OutputFormat::Mp3
    } else {
        OutputFormat::Mp4
    };
    let ext = match output_format {
        OutputFormat::Mp3 => "mp3",
        OutputFormat::Mp4 => "m4b",
    };

    let output_path = unique_output_path(&output_dir, &config.output_filename, ext);
    let output_str = output_path.to_str().ok_or("Invalid output path")?;

    let channels_arg = if config.mono { "1" } else { "2" };
    let bitrate_arg = format!("{}k", config.bitrate);

    if want_alac {
        let mut sr_counts: HashMap<u32, u32> = HashMap::new();
        for p in &probed {
            if p.sample_rate > 0 { *sr_counts.entry(p.sample_rate).or_insert(0) += 1; }
        }
        let target_sr = sr_counts
            .into_iter()
            .max_by_key(|&(_, c)| c)
            .map(|(sr, _)| sr)
            .unwrap_or(44_100);

        let mut ch_counts: HashMap<u32, u32> = HashMap::new();
        for p in &probed {
            if p.channels > 0 { *ch_counts.entry(p.channels).or_insert(0) += 1; }
        }
        let target_ch = ch_counts
            .into_iter()
            .max_by_key(|&(_, c)| c)
            .map(|(ch, _)| ch)
            .unwrap_or(2);
        let target_ch_str = target_ch.to_string();

        emit(Stage::Transcoding, 5.0, "Encoding to ALAC (lossless)");

        let all_items: Vec<(usize, String)> = config.files.iter().enumerate()
            .map(|(i, f)| (i, f.path.clone()))
            .collect();

        let transcoded = transcode_parallel(
            &all_items, tmp_dir.path(), "alac", &bitrate_arg, Some(&target_ch_str),
            Some(target_sr), &durations, &emit, 5.0, 90.0,
        )?;

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        emit(Stage::Merging, 90.0, "Concatenating ALAC files");
        let intermediate = concat_aac_files(&transcoded, tmp_dir.path())?;
        let intermediate_str = path_str(&intermediate)?;

        emit(Stage::Chapters, 95.0, "Adding chapter metadata");
        add_metadata_and_cover(
            intermediate_str,
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
            output_format,
        )?;

    } else if uniform_aac {
        emit(Stage::Merging, 5.0, "Remuxing AAC files (no re-encoding)");

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        let concat_list = tmp_dir.path().join("concat.txt");
        let mut f = fs::File::create(&concat_list)
            .map_err(|e| format!("Failed to create concat list: {}", e))?;
        for file in &config.files {
            validate_concat_path(&file.path)?;
            writeln!(f, "file '{}'", file.path.replace('\'', "'\\''"))
                .map_err(|e| format!("Failed to write concat list: {}", e))?;
        }

        let intermediate = tmp_dir.path().join("merged.m4a");
        let concat_list_str = path_str(&concat_list)?;
        let intermediate_str = path_str(&intermediate)?.to_string();
        let total: f64 = durations.iter().sum();
        let pct_start = 5.0_f64;
        let pct_end = 90.0_f64;
        run_ffmpeg_with_progress(
            &[
                "-y",
                "-progress", "pipe:1",
                "-f", "concat", "-safe", "0",
                "-i", concat_list_str,
                "-map", "0:a",
                "-c", "copy",
                &intermediate_str,
            ],
            total,
            |secs| {
                let frac = if total > 0.0 { (secs / total).min(1.0) } else { 0.0 };
                let pct = pct_start + (pct_end - pct_start) * frac;
                emit(Stage::Merging, pct, "Remuxing AAC files (no re-encoding)");
            },
            "remux",
        )?;

        emit(Stage::Chapters, 92.0, "Adding chapter metadata");
        add_metadata_and_cover(
            &intermediate_str,
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
            output_format,
        )?;

    } else if all_aac {
        let mut sr_counts: HashMap<u32, u32> = HashMap::new();
        for p in &probed {
            if p.sample_rate > 0 { *sr_counts.entry(p.sample_rate).or_insert(0) += 1; }
        }
        let target_sr = sr_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(sr, _)| sr)
            .unwrap_or(44_100);

        emit(Stage::Transcoding, 5.0, "Normalizing sample rates");

        let mismatched_items: Vec<(usize, String)> = config.files.iter().enumerate()
            .filter(|(i, _)| probed[*i].sample_rate != target_sr)
            .map(|(i, f)| (i, f.path.clone()))
            .collect();

        let transcoded = transcode_parallel(
            &mismatched_items, tmp_dir.path(), aac_encoder(), &bitrate_arg, Some(channels_arg),
            Some(target_sr), &durations, &emit, 5.0, 90.0,
        )?;

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        let mut transcode_map: HashMap<usize, PathBuf> =
            mismatched_items.iter().map(|(idx, _)| *idx).zip(transcoded).collect();

        let mut all_paths: Vec<PathBuf> = Vec::new();
        for (i, file) in config.files.iter().enumerate() {
            if let Some(path) = transcode_map.remove(&i) {
                all_paths.push(path);
            } else {
                all_paths.push(PathBuf::from(&file.path));
            }
        }

        emit(Stage::Merging, 90.0, "Concatenating normalized files");
        let intermediate = concat_aac_files(&all_paths, tmp_dir.path())?;
        let intermediate_str = path_str(&intermediate)?;

        emit(Stage::Chapters, 95.0, "Adding chapter metadata");
        add_metadata_and_cover(
            intermediate_str,
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
            output_format,
        )?;

    } else if uniform_mp3 {
        emit(Stage::Merging, 5.0, "Cleaning MP3 frames for concat");

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        // Pre-strip each source MP3 of its leading Xing/Info silent frame and
        // all ID3 metadata, in parallel. Records each stripped audio size so
        // we can emit accurate CHAP byte offsets after the metadata pass.
        let stripped: Vec<Result<PathBuf, String>> = config.files.par_iter()
            .enumerate()
            .map(|(i, file)| {
                validate_concat_path(&file.path)?;
                let out = tmp_dir.path().join(format!("stripped_{:04}.mp3", i));
                strip_mp3_for_concat(&file.path, &out)?;
                Ok(out)
            })
            .collect();

        let mut stripped_paths: Vec<PathBuf> = Vec::with_capacity(stripped.len());
        for r in stripped { stripped_paths.push(r?); }

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        let concat_list = tmp_dir.path().join("concat.txt");
        let mut f = fs::File::create(&concat_list)
            .map_err(|e| format!("Failed to create concat list: {}", e))?;
        for p in &stripped_paths {
            let s = path_str(p)?;
            validate_concat_path(s)?;
            writeln!(f, "file '{}'", s.replace('\'', "'\\''"))
                .map_err(|e| format!("Failed to write concat list: {}", e))?;
        }

        let intermediate = tmp_dir.path().join("merged.mp3");
        let concat_list_str = path_str(&concat_list)?;
        let intermediate_str = path_str(&intermediate)?.to_string();
        let total: f64 = durations.iter().sum();
        let pct_start = 30.0_f64;
        let pct_end = 90.0_f64;
        run_ffmpeg_with_progress(
            &[
                "-y",
                "-progress", "pipe:1",
                "-f", "concat", "-safe", "0",
                "-i", concat_list_str,
                "-map", "0:a",
                "-c", "copy",
                "-write_xing", "0",
                "-id3v2_version", "0",
                "-fflags", "+bitexact",
                &intermediate_str,
            ],
            total,
            |secs| {
                let frac = if total > 0.0 { (secs / total).min(1.0) } else { 0.0 };
                let pct = pct_start + (pct_end - pct_start) * frac;
                emit(Stage::Merging, pct, "Concatenating MP3 frames");
            },
            "remux",
        )?;

        emit(Stage::Chapters, 92.0, "Adding chapter metadata");
        add_metadata_and_cover(
            &intermediate_str,
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
            output_format,
        )?;

        if output_format == OutputFormat::Mp3 {
            emit(Stage::Chapters, 97.0, "Indexing chapter byte offsets");
            set_chap_byte_offsets(output_str, &durations)?;
        }

    } else if all_mp3 {
        // Non-uniform MP3: re-encode outliers to MP3 at mode sample rate / channels,
        // then lossless-concat the full set and mux into M4B. Majority stays bit-perfect.
        let mut sr_counts: HashMap<u32, u32> = HashMap::new();
        for p in &probed {
            if p.sample_rate > 0 { *sr_counts.entry(p.sample_rate).or_insert(0) += 1; }
        }
        let target_sr = sr_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(sr, _)| sr)
            .unwrap_or(44_100);

        let mut ch_counts: HashMap<u32, u32> = HashMap::new();
        for p in &probed {
            if p.channels > 0 { *ch_counts.entry(p.channels).or_insert(0) += 1; }
        }
        let target_ch = ch_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(ch, _)| ch)
            .unwrap_or(2);
        let target_ch_str = target_ch.to_string();

        emit(Stage::Transcoding, 5.0, "Re-encoding outlier MP3 files");

        let outlier_items: Vec<(usize, String)> = config.files.iter().enumerate()
            .filter(|(i, _)| probed[*i].sample_rate != target_sr || probed[*i].channels != target_ch)
            .map(|(i, f)| (i, f.path.clone()))
            .collect();

        let transcoded = transcode_parallel(
            &outlier_items, tmp_dir.path(), "libmp3lame", "0k", Some(&target_ch_str),
            Some(target_sr), &durations, &emit, 5.0, 85.0,
        )?;

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        let mut transcode_map: HashMap<usize, PathBuf> =
            outlier_items.iter().map(|(idx, _)| *idx).zip(transcoded).collect();

        // Resolve each chapter's source path (transcoded or original), then
        // strip all of them so the merged stream is a clean linear sequence
        // without per-file Xing/Info headers or ID3 tags. Track stripped
        // sizes for the CHAP byte-offset patch.
        emit(Stage::Merging, 85.0, "Cleaning MP3 frames for concat");
        let resolved: Vec<String> = config.files.iter().enumerate()
            .map(|(i, file)| {
                if let Some(p) = transcode_map.remove(&i) {
                    path_str(&p).map(|s| s.to_string())
                } else {
                    validate_concat_path(&file.path)?;
                    Ok(file.path.clone())
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        let stripped: Vec<Result<PathBuf, String>> = resolved.par_iter()
            .enumerate()
            .map(|(i, p)| {
                let out = tmp_dir.path().join(format!("stripped_{:04}.mp3", i));
                strip_mp3_for_concat(p, &out)?;
                Ok(out)
            })
            .collect();

        let mut stripped_paths: Vec<PathBuf> = Vec::with_capacity(stripped.len());
        for r in stripped { stripped_paths.push(r?); }

        emit(Stage::Merging, 88.0, "Concatenating MP3 frames");
        let concat_list = tmp_dir.path().join("concat.txt");
        let mut f = fs::File::create(&concat_list)
            .map_err(|e| format!("Failed to create concat list: {}", e))?;
        for p in &stripped_paths {
            let s = path_str(p)?;
            validate_concat_path(s)?;
            writeln!(f, "file '{}'", s.replace('\'', "'\\''"))
                .map_err(|e| format!("Failed to write concat list: {}", e))?;
        }

        let intermediate = tmp_dir.path().join("merged.mp3");
        let concat_list_str = path_str(&concat_list)?;
        let intermediate_str = path_str(&intermediate)?.to_string();
        run_ffmpeg_with_progress(
            &[
                "-y", "-f", "concat", "-safe", "0",
                "-i", concat_list_str,
                "-map", "0:a",
                "-c", "copy",
                "-write_xing", "0",
                "-id3v2_version", "0",
                "-fflags", "+bitexact",
                &intermediate_str,
            ],
            0.0, |_| {}, "concat",
        )?;

        emit(Stage::Chapters, 92.0, "Adding chapter metadata");
        add_metadata_and_cover(
            &intermediate_str,
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
            output_format,
        )?;

        if output_format == OutputFormat::Mp3 {
            emit(Stage::Chapters, 97.0, "Indexing chapter byte offsets");
            set_chap_byte_offsets(output_str, &durations)?;
        }

    } else if uniform_alac {
        emit(Stage::Merging, 5.0, "Remuxing ALAC files (no re-encoding)");

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        let concat_list = tmp_dir.path().join("concat.txt");
        let mut f = fs::File::create(&concat_list)
            .map_err(|e| format!("Failed to create concat list: {}", e))?;
        for file in &config.files {
            validate_concat_path(&file.path)?;
            writeln!(f, "file '{}'", file.path.replace('\'', "'\\''"))
                .map_err(|e| format!("Failed to write concat list: {}", e))?;
        }

        let intermediate = tmp_dir.path().join("merged.m4a");
        let concat_list_str = path_str(&concat_list)?;
        let intermediate_str = path_str(&intermediate)?.to_string();
        let total: f64 = durations.iter().sum();
        let pct_start = 5.0_f64;
        let pct_end = 90.0_f64;
        run_ffmpeg_with_progress(
            &[
                "-y",
                "-progress", "pipe:1",
                "-f", "concat", "-safe", "0",
                "-i", concat_list_str,
                "-map", "0:a",
                "-c", "copy",
                &intermediate_str,
            ],
            total,
            |secs| {
                let frac = if total > 0.0 { (secs / total).min(1.0) } else { 0.0 };
                let pct = pct_start + (pct_end - pct_start) * frac;
                emit(Stage::Merging, pct, "Remuxing ALAC files (no re-encoding)");
            },
            "remux",
        )?;

        emit(Stage::Chapters, 92.0, "Adding chapter metadata");
        add_metadata_and_cover(
            &intermediate_str,
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
            output_format,
        )?;

    } else if all_alac {
        let mut sr_counts: HashMap<u32, u32> = HashMap::new();
        for p in &probed {
            if p.sample_rate > 0 { *sr_counts.entry(p.sample_rate).or_insert(0) += 1; }
        }
        let target_sr = sr_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(sr, _)| sr)
            .unwrap_or(44_100);

        let mut ch_counts: HashMap<u32, u32> = HashMap::new();
        for p in &probed {
            if p.channels > 0 { *ch_counts.entry(p.channels).or_insert(0) += 1; }
        }
        let target_ch = ch_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(ch, _)| ch)
            .unwrap_or(2);
        let target_ch_str = target_ch.to_string();

        emit(Stage::Transcoding, 5.0, "Normalizing ALAC files");

        let mismatched_items: Vec<(usize, String)> = config.files.iter().enumerate()
            .filter(|(i, _)| probed[*i].sample_rate != target_sr || probed[*i].channels != target_ch)
            .map(|(i, f)| (i, f.path.clone()))
            .collect();

        let transcoded = transcode_parallel(
            &mismatched_items, tmp_dir.path(), "alac", &bitrate_arg, Some(&target_ch_str),
            Some(target_sr), &durations, &emit, 5.0, 90.0,
        )?;

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        let mut transcode_map: HashMap<usize, PathBuf> =
            mismatched_items.iter().map(|(idx, _)| *idx).zip(transcoded).collect();

        let mut all_paths: Vec<PathBuf> = Vec::new();
        for (i, file) in config.files.iter().enumerate() {
            if let Some(path) = transcode_map.remove(&i) {
                all_paths.push(path);
            } else {
                all_paths.push(PathBuf::from(&file.path));
            }
        }

        emit(Stage::Merging, 90.0, "Concatenating normalized files");
        let intermediate = concat_aac_files(&all_paths, tmp_dir.path())?;
        let intermediate_str = path_str(&intermediate)?;

        emit(Stage::Chapters, 95.0, "Adding chapter metadata");
        add_metadata_and_cover(
            intermediate_str,
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
            output_format,
        )?;

    } else {
        let mut sr_counts: HashMap<u32, u32> = HashMap::new();
        for p in &probed {
            if p.sample_rate > 0 { *sr_counts.entry(p.sample_rate).or_insert(0) += 1; }
        }
        let target_sr = sr_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(sr, _)| sr)
            .unwrap_or(44_100);

        if force {
            emit(Stage::Transcoding, 5.0, "Transcoding all files to AAC");
        } else {
            emit(Stage::Transcoding, 5.0, "Transcoding non-AAC files");
        }

        let all_items: Vec<(usize, String)> = if force {
            config.files.iter().enumerate()
                .map(|(i, f)| (i, f.path.clone()))
                .collect()
        } else {
            let non_aac_items: Vec<(usize, String)> = config.files.iter().enumerate()
                .filter(|(i, _)| probed[*i].codec != "aac")
                .map(|(i, f)| (i, f.path.clone()))
                .collect();

            let mismatched_aac_items: Vec<(usize, String)> = config.files.iter().enumerate()
                .filter(|(i, _)| probed[*i].codec == "aac" && probed[*i].sample_rate != target_sr)
                .map(|(i, f)| (i, f.path.clone()))
                .collect();

            let mut items = non_aac_items;
            items.extend(mismatched_aac_items);
            items
        };

        let transcoded = transcode_parallel(
            &all_items, tmp_dir.path(), aac_encoder(), &bitrate_arg, Some(channels_arg),
            Some(target_sr), &durations, &emit, 5.0, 90.0,
        )?;

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        let mut transcode_map: HashMap<usize, PathBuf> =
            all_items.iter().map(|(idx, _)| *idx).zip(transcoded).collect();

        let mut all_paths: Vec<PathBuf> = Vec::new();
        for (i, file) in config.files.iter().enumerate() {
            if let Some(path) = transcode_map.remove(&i) {
                all_paths.push(path);
            } else {
                all_paths.push(PathBuf::from(&file.path));
            }
        }

        emit(Stage::Merging, 90.0, "Concatenating all files");
        let intermediate = concat_aac_files(&all_paths, tmp_dir.path())?;
        let intermediate_str = path_str(&intermediate)?;

        emit(Stage::Chapters, 95.0, "Adding chapter metadata");
        add_metadata_and_cover(
            intermediate_str,
            output_str,
            &config,
            &durations,
            tmp_dir.path(),
            output_format,
        )?;
    }

    if let Some(ref cover_path) = config.cover_art_path {
        if is_temp_path(cover_path) {
            let _ = fs::remove_file(cover_path);
        }
    }

    emit(Stage::Done, 100.0, "Audio file created successfully!");
    Ok(output_str.to_string())
}

#[tauri::command]
pub fn merge_audio_files(app: tauri::AppHandle, config: MergeConfig) -> Result<(), String> {
    if IS_CONVERTING.swap(true, Ordering::SeqCst) {
        return Err("A conversion is already in progress".to_string());
    }
    CANCEL_FLAG.store(false, Ordering::SeqCst);

    std::thread::spawn(move || {
        let _guard = ConvertGuard;
        let app_for_progress = app.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            merge_audio_files_core(config, move |stage, percent, message| {
                let _ = app_for_progress.emit("merge-progress", MergeProgress {
                    stage: stage.as_str().to_string(),
                    percent,
                    message: message.to_string(),
                });
            })
        }));

        match result {
            Ok(Ok(path)) => {
                if CANCEL_FLAG.load(Ordering::Relaxed) {
                    let _ = app.emit("merge-cancelled", ());
                } else {
                    let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let _ = app.emit("merge-complete", serde_json::json!({
                        "path": path,
                        "size_bytes": size_bytes,
                    }));
                }
            }
            Ok(Err(e)) => {
                if e.contains("Cancelled") {
                    let _ = app.emit("merge-cancelled", ());
                } else {
                    eprintln!("[bind-it] merge failed: {}", e);
                    let msg = categorize_error(&e);
                    let _ = app.emit("merge-error", msg);
                }
            }
            Err(panic) => {
                let msg = if let Some(s) = panic.downcast_ref::<&'static str>() {
                    format!("Internal error: {}", s)
                } else if let Some(s) = panic.downcast_ref::<String>() {
                    format!("Internal error: {}", s)
                } else {
                    "Internal error: unexpected failure during conversion".to_string()
                };
                eprintln!("[bind-it] merge panicked: {}", msg);
                let _ = app.emit("merge-error", msg);
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn cancel_merge() {
    CANCEL_FLAG.store(true, Ordering::SeqCst);
}
