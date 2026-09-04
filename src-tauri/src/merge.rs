use crate::binaries::{aac_encoder, run_ffmpeg_with_progress, ConvertGuard, CANCEL_FLAG, IS_CONVERTING};
use crate::concat::{
    add_metadata_and_cover, concat_aac_files, set_chap_byte_offsets,
    strip_mp3_for_concat, OutputFormat,
};
use rayon::prelude::*;
use crate::cover::validate_cover_image;
use crate::probe::{probe_all_files, probe_single_file};
use crate::transcode::transcode_parallel;
use crate::types::{AudioFileInfo, MergeConfig, MergeProgress, Stage};
use crate::util::{
    categorize_error, is_temp_path, path_str, unique_output_path,
    validate_concat_path, validate_filename,
};
use std::collections::HashMap;
use std::fs;
use std::hash::Hash;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::Emitter;

/// Returns the most frequent value across `iter` (filtering out items mapped
/// to `0`), or `default` if no items are present. Ties resolve to the larger
/// value: HashMap iteration order is randomised per process, so without an
/// explicit rule a two-file set would pick a different target on every run.
pub fn most_common_nonzero<I, T>(iter: I, default: T) -> T
where
    I: IntoIterator<Item = T>,
    T: Eq + Hash + Copy + Default + Ord,
{
    let mut counts: HashMap<T, u32> = HashMap::new();
    for v in iter {
        if v != T::default() {
            *counts.entry(v).or_insert(0) += 1;
        }
    }
    counts.into_iter().max_by_key(|&(v, c)| (c, v)).map(|(v, _)| v).unwrap_or(default)
}

/// The stream parameters the pass-through files impose on the concatenated
/// stream. The concat demuxer copies packets verbatim, so every file that is
/// not re-encoded must already match all of these: a mono chapter between
/// stereo ones, or a 22.05 kHz one between 44.1 kHz ones, yields a file
/// Apple's decoder rejects; an HE-AAC chapter next to LC plays at the wrong
/// speed (SBR doubles the output rate); 24-bit ALAC next to 16-bit fails to
/// decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamTarget {
    pub sample_rate: u32,
    pub channels: u32,
    pub aac_profile: Option<String>,
    pub bit_depth: u32,
}

impl StreamTarget {
    /// The majority value of each parameter across the set.
    pub fn majority(probed: &[AudioFileInfo]) -> Self {
        let mut profiles: HashMap<Option<&str>, u32> = HashMap::new();
        for p in probed {
            *profiles.entry(p.aac_profile.as_deref()).or_insert(0) += 1;
        }
        let aac_profile = profiles
            .into_iter()
            .max_by_key(|&(v, c)| (c, v.map(|s| s.to_string())))
            .and_then(|(v, _)| v.map(|s| s.to_string()));
        StreamTarget {
            sample_rate: most_common_nonzero(probed.iter().map(|p| p.sample_rate), 44_100),
            channels: most_common_nonzero(probed.iter().map(|p| p.channels), 2),
            aac_profile,
            bit_depth: most_common_nonzero(probed.iter().map(|p| p.bit_depth), 0),
        }
    }

    fn matches(&self, p: &AudioFileInfo) -> bool {
        p.sample_rate == self.sample_rate
            && p.channels == self.channels
            && p.aac_profile == self.aac_profile
            && (self.bit_depth == 0 || p.bit_depth == 0 || p.bit_depth == self.bit_depth)
    }

    /// Every file that must be re-encoded so it can sit next to the rest.
    /// If the majority is itself something we cannot encode to (HE-AAC — the
    /// bundled encoders produce LC only), nothing can pass through and every
    /// file is an outlier.
    pub fn outliers(&self, probed: &[AudioFileInfo]) -> Vec<usize> {
        let unencodable_majority = matches!(self.aac_profile.as_deref(), Some(p) if p != "LC");
        probed
            .iter()
            .enumerate()
            .filter(|(_, p)| unencodable_majority || !self.matches(p))
            .map(|(i, _)| i)
            .collect()
    }

    /// True when every file already matches (nothing needs re-encoding).
    pub fn is_uniform(&self, probed: &[AudioFileInfo]) -> bool {
        self.outliers(probed).is_empty()
    }
}

/// Bitrate (kbps) to re-encode outliers at on a preserve path: the average of
/// the files kept as-is, so a re-encoded chapter matches its neighbours rather
/// than the compress-mode setting (which the UI hides on this path). Falls
/// back to `fallback_kbps` when no pass-through file reports a bitrate.
fn passthrough_bitrate(probed: &[AudioFileInfo], outliers: &[usize], fallback_kbps: u32) -> u32 {
    let rates: Vec<u64> = probed
        .iter()
        .enumerate()
        .filter(|(i, _)| !outliers.contains(i))
        .filter_map(|(_, p)| p.bitrate)
        .filter(|&b| b > 0)
        .collect();
    if rates.is_empty() {
        return fallback_kbps.clamp(32, 320);
    }
    let avg_kbps = (rates.iter().sum::<u64>() / rates.len() as u64 / 1000) as u32;
    avg_kbps.clamp(32, 320)
}

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

    if CANCEL_FLAG.load(Ordering::SeqCst) {
        return Err("Cancelled by user".to_string());
    }

    let file_paths: Vec<String> = config.files.iter().map(|f| f.path.clone()).collect();
    let probed = probe_all_files(file_paths)?;
    if probed.is_empty() {
        return Err("No valid audio files to merge.".to_string());
    }
    let durations: Vec<f64> = match config.durations.as_ref() {
        Some(cached) if cached.len() == probed.len() => cached.clone(),
        _ => probed.iter().map(|f| f.duration).collect(),
    };

    let force = config.force_transcode;

    // A cover that ffmpeg can't read would otherwise fail the bind at the
    // very last step, after minutes of transcoding, with an opaque error.
    if let Some(ref cover) = config.cover_art_path {
        if std::path::Path::new(cover).exists() {
            validate_cover_image(cover)?;
        }
    }

    let all_aac = !force && probed.iter().all(|f| f.codec == "aac");
    let all_mp3 = !force && probed.iter().all(|f| f.codec == "mp3");
    let all_alac = !force && probed.iter().all(|f| f.codec == "alac");
    let target = StreamTarget::majority(&probed);
    let uniform_aac = all_aac && target.is_uniform(&probed);
    let uniform_mp3 = all_mp3 && {
        let sr = probed[0].sample_rate;
        let ch = probed[0].channels;
        probed.iter().all(|f| f.sample_rate == sr && f.channels == ch)
    };
    let uniform_alac = all_alac && target.is_uniform(&probed);

    let output_dir = PathBuf::from(&config.output_dir);
    if !output_dir.exists() {
        fs::create_dir_all(&output_dir)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    // Work inside the destination folder rather than the system temp dir:
    // the intermediates of a long book run to gigabytes, which overflows a
    // tmpfs /tmp (Fedora, Arch, Debian 13) with an error blaming the wrong
    // disk — and the final rename into place is then always same-volume.
    let tmp_dir = tempfile::Builder::new()
        .prefix(".bind-it-")
        .tempdir_in(&output_dir)
        .map_err(|e| format!("Failed to create temp dir in output folder: {}", e))?;

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

    // Everything is written inside the temp dir and only renamed into place
    // at the very end, so a cancel or failure never leaves a truncated file
    // at the destination.
    let work_path = tmp_dir.path().join(format!("output.{}", ext));
    let work_str = path_str(&work_path)?.to_string();
    let output_str: &str = &work_str;

    let channels_arg = if config.mono { "1" } else { "2" };
    let bitrate_arg = format!("{}k", config.bitrate);

    // Raw ADTS .aac cannot be stream-copied next to MP4-wrapped AAC (the
    // adtstoasc filter fails mid-stream and the concat demuxer truncates the
    // output while exiting 0), and ffprobe only estimates its duration. Wrap
    // each one into an MP4 first — lossless — and take the exact duration.
    let mut sources: Vec<String> = config.files.iter().map(|f| f.path.clone()).collect();
    let mut durations = durations;
    if !force && !want_alac && probed.iter().any(|p| p.is_adts) {
        emit(Stage::Preparing, 2.0, "Wrapping raw AAC files");
        for (i, p) in probed.iter().enumerate() {
            if !p.is_adts {
                continue;
            }
            let wrapped = tmp_dir.path().join(format!("adts_{:04}.m4a", i));
            let wrapped_str = path_str(&wrapped)?.to_string();
            run_ffmpeg_with_progress(
                &["-y", "-i", &sources[i], "-map", "0:a:0", "-c:a", "copy", "-f", "ipod", &wrapped_str],
                0.0,
                |_| {},
                "wrap",
            )?;
            if let Ok(info) = probe_single_file(&wrapped_str) {
                if info.duration > 0.0 {
                    durations[i] = info.duration;
                }
            }
            sources[i] = wrapped_str;
        }
    }

    if want_alac {
        let target_sr = most_common_nonzero(probed.iter().map(|p| p.sample_rate), 44_100);
        let target_ch = most_common_nonzero(probed.iter().map(|p| p.channels), 2);
        let target_ch_str = target_ch.to_string();

        emit(Stage::Transcoding, 5.0, "Encoding to ALAC (lossless)");

        let all_items: Vec<(usize, String)> = config.files.iter().enumerate()
            .map(|(i, f)| (i, f.path.clone()))
            .collect();

        let transcoded = transcode_parallel(
            &all_items, tmp_dir.path(), "alac", &bitrate_arg, Some(&target_ch_str),
            Some(target_sr), true, Some(target.bit_depth), &durations, &emit, 5.0, 90.0,
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
        for path in &sources {
            validate_concat_path(path)?;
            writeln!(f, "file '{}'", path.replace('\'', "'\\''"))
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
        // The pass-through files fix the stream parameters; every outlier is
        // re-encoded to exactly those (rate *and* channels — see
        // normalization_outliers), at a bitrate matching its neighbours.
        let target_sr = target.sample_rate;
        let target_ch_str = target.channels.to_string();
        let outliers = target.outliers(&probed);
        let outlier_bitrate_arg = format!("{}k", passthrough_bitrate(&probed, &outliers, config.bitrate));

        emit(Stage::Transcoding, 5.0, "Normalizing sample rates");

        let mismatched_items: Vec<(usize, String)> = outliers.iter()
            .map(|&i| (i, sources[i].clone()))
            .collect();

        let transcoded = transcode_parallel(
            &mismatched_items, tmp_dir.path(), aac_encoder(), &outlier_bitrate_arg, Some(&target_ch_str),
            Some(target_sr), true, None, &durations, &emit, 5.0, 90.0,
        )?;

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        let mut transcode_map: HashMap<usize, PathBuf> =
            mismatched_items.iter().map(|(idx, _)| *idx).zip(transcoded).collect();

        let mut all_paths: Vec<PathBuf> = Vec::new();
        for (i, source) in sources.iter().enumerate() {
            if let Some(path) = transcode_map.remove(&i) {
                all_paths.push(path);
            } else {
                all_paths.push(PathBuf::from(source));
            }
        }
        debug_assert!(transcode_map.is_empty(), "leftover transcoded files: {:?}", transcode_map.keys());

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
        let target_sr = most_common_nonzero(probed.iter().map(|p| p.sample_rate), 44_100);
        let target_ch = most_common_nonzero(probed.iter().map(|p| p.channels), 2);
        let target_ch_str = target_ch.to_string();

        emit(Stage::Transcoding, 5.0, "Re-encoding outlier MP3 files");

        let outlier_items: Vec<(usize, String)> = config.files.iter().enumerate()
            .filter(|(i, _)| probed[*i].sample_rate != target_sr || probed[*i].channels != target_ch)
            .map(|(i, f)| (i, f.path.clone()))
            .collect();

        let transcoded = transcode_parallel(
            &outlier_items, tmp_dir.path(), "libmp3lame", "0k", Some(&target_ch_str),
            Some(target_sr), true, None, &durations, &emit, 5.0, 85.0,
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
        debug_assert!(transcode_map.is_empty(), "leftover transcoded files: {:?}", transcode_map.keys());

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
        let total: f64 = durations.iter().sum();
        let pct_start = 88.0_f64;
        let pct_end = 91.0_f64;
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
            "concat",
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
        for path in &sources {
            validate_concat_path(path)?;
            writeln!(f, "file '{}'", path.replace('\'', "'\\''"))
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
        let target_sr = target.sample_rate;
        let target_ch_str = target.channels.to_string();

        emit(Stage::Transcoding, 5.0, "Normalizing ALAC files");

        let mismatched_items: Vec<(usize, String)> = target.outliers(&probed).into_iter()
            .map(|i| (i, sources[i].clone()))
            .collect();

        let transcoded = transcode_parallel(
            &mismatched_items, tmp_dir.path(), "alac", &bitrate_arg, Some(&target_ch_str),
            Some(target_sr), true, Some(target.bit_depth), &durations, &emit, 5.0, 90.0,
        )?;

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        let mut transcode_map: HashMap<usize, PathBuf> =
            mismatched_items.iter().map(|(idx, _)| *idx).zip(transcoded).collect();

        let mut all_paths: Vec<PathBuf> = Vec::new();
        for (i, source) in sources.iter().enumerate() {
            if let Some(path) = transcode_map.remove(&i) {
                all_paths.push(path);
            } else {
                all_paths.push(PathBuf::from(source));
            }
        }
        debug_assert!(transcode_map.is_empty(), "leftover transcoded files: {:?}", transcode_map.keys());

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
        let target_sr = most_common_nonzero(probed.iter().map(|p| p.sample_rate), 44_100);

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

            // AAC files only pass through when they already match what the
            // transcoded files will be: the target rate and the chosen
            // channel count. Anything else is re-encoded too.
            let transcoded_ch: u32 = if config.mono { 1 } else { 2 };
            let mismatched_aac_items: Vec<(usize, String)> = config.files.iter().enumerate()
                .filter(|(i, _)| probed[*i].codec == "aac"
                    && (probed[*i].sample_rate != target_sr
                        || probed[*i].channels != transcoded_ch
                        || probed[*i].aac_profile.as_deref().is_some_and(|p| p != "LC")))
                .map(|(i, _)| (i, sources[i].clone()))
                .collect();

            let mut items = non_aac_items;
            items.extend(mismatched_aac_items);
            items
        };

        let transcoded = transcode_parallel(
            &all_items, tmp_dir.path(), aac_encoder(), &bitrate_arg, Some(channels_arg),
            Some(target_sr), !force, None, &durations, &emit, 5.0, 90.0,
        )?;

        if CANCEL_FLAG.load(Ordering::Relaxed) {
            return Err("Cancelled by user".to_string());
        }

        let mut transcode_map: HashMap<usize, PathBuf> =
            all_items.iter().map(|(idx, _)| *idx).zip(transcoded).collect();

        let mut all_paths: Vec<PathBuf> = Vec::new();
        for (i, source) in sources.iter().enumerate() {
            if let Some(path) = transcode_map.remove(&i) {
                all_paths.push(path);
            } else {
                all_paths.push(PathBuf::from(source));
            }
        }
        debug_assert!(transcode_map.is_empty(), "leftover transcoded files: {:?}", transcode_map.keys());

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

    // Defence in depth: ffmpeg's concat demuxer stops at the first entry it
    // cannot read and still exits 0, so a file missing whole chapters would
    // otherwise be reported as a success. Tolerance is generous because
    // input durations are ffprobe estimates (VBR MP3 without a Xing header
    // can be off by a few percent) — this only catches losing real chunks.
    let expected_secs: f64 = durations.iter().sum();
    if expected_secs > 0.0 {
        if let Ok(actual) = crate::probe::probe_single_file(output_str).map(|p| p.duration) {
            let shortfall = expected_secs - actual;
            if shortfall > 10.0 && actual < expected_secs * 0.8 {
                let _ = fs::remove_file(output_str);
                return Err(format!(
                    "Output came out much shorter than its chapters ({:.0}s of {:.0}s), so it was discarded rather than saved incomplete. Re-add your files and try again.",
                    actual, expected_secs
                ));
            }
        }
    }

    if let Some(ref cover_path) = config.cover_art_path {
        if is_temp_path(cover_path) {
            let _ = fs::remove_file(cover_path);
        }
    }

    if CANCEL_FLAG.load(Ordering::Relaxed) {
        return Err("Cancelled by user".to_string());
    }
    let final_path = unique_output_path(&output_dir, &config.output_filename, ext);
    fs::rename(&work_path, &final_path)
        .map_err(|e| format!("Failed to move the finished file into place: {}", e))?;
    let final_str = final_path.to_str().ok_or("Invalid output path")?.to_string();

    emit(Stage::Done, 100.0, "Audio file created successfully!");
    Ok(final_str)
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
                if CANCEL_FLAG.load(Ordering::SeqCst) {
                    let _ = app.emit("merge-cancelled", ());
                } else {
                    let size_bytes = match fs::metadata(&path) {
                        Ok(m) => m.len(),
                        Err(e) => {
                            eprintln!("[bind-it] failed to stat output {}: {}", path, e);
                            0
                        }
                    };
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
                eprintln!("[bind-it] merge panicked: {:?}", panic);
                let msg = if let Some(s) = panic.downcast_ref::<&'static str>() {
                    format!("Internal error: {}", s)
                } else if let Some(s) = panic.downcast_ref::<String>() {
                    format!("Internal error: {}", s)
                } else {
                    "Internal error: unexpected failure during conversion".to_string()
                };
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
