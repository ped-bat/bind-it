use crate::binaries::run_ffmpeg_with_progress;
use crate::types::MergeConfig;
use crate::util::{generate_ffmetadata, path_str, validate_concat_path};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Mp4,
    Mp3,
}

/// Re-mux an MP3 stripped of: leading Xing/Info/LAME header frame, ID3v2 tag,
/// attached pictures, ID3v1 trailer, and per-stream metadata.
///
/// Why: when concatenating MP3 files, each source's Xing/Info silent header
/// frame and any ID3 tags get embedded mid-stream. Some players (Apple Books
/// in particular) misinterpret these as track/chapter boundaries or fail to
/// compute durations. Pre-stripping keeps the merged MP3 a clean linear
/// sequence of audio frames.
pub fn strip_mp3_for_concat(input: &str, output: &Path) -> Result<(), String> {
    let output_str = path_str(output)?.to_string();
    run_ffmpeg_with_progress(
        &[
            "-y",
            "-ss", "0.026",
            "-i", input,
            // Drop any attached-picture / video stream so the audio-only
            // output doesn't need an ID3v2 header to carry it.
            "-map", "0:a:0",
            "-vn",
            "-c:a", "copy",
            "-write_xing", "0",
            "-id3v2_version", "0",
            "-map_metadata", "-1",
            "-fflags", "+bitexact",
            "-f", "mp3",
            &output_str,
        ],
        0.0, |_| {}, "strip",
    )
}

/// Decode a single MPEG audio frame header. Returns (frame_size_bytes,
/// frame_duration_ms) for Layer III frames (the only layer that's relevant
/// for typical audio-file MP3s). Returns None for invalid/non-Layer-III headers so the
/// caller can resync.
fn mp3_frame_info(h: u32) -> Option<(usize, u64)> {
    if (h >> 21) & 0x7FF != 0x7FF { return None; }
    let version = (h >> 19) & 0x3; // 0=2.5, 1=reserved, 2=2, 3=1
    let layer = (h >> 17) & 0x3;   // 0=reserved, 1=III, 2=II, 3=I
    let bitrate_idx = ((h >> 12) & 0xF) as usize;
    let sr_idx = ((h >> 10) & 0x3) as usize;
    let padding = ((h >> 9) & 0x1) as usize;

    // Layer III is the only one we expect from typical audio-file sources.
    if version == 1 || layer != 1 || bitrate_idx == 0 || bitrate_idx == 15 || sr_idx == 3 {
        return None;
    }
    let is_v1 = version == 3;

    let bitrate_kbps: u32 = if is_v1 {
        [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320][bitrate_idx]
    } else {
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160][bitrate_idx]
    };

    let sr: u32 = match version {
        3 => [44100, 48000, 32000][sr_idx],
        2 => [22050, 24000, 16000][sr_idx],
        0 => [11025, 12000, 8000][sr_idx],
        _ => return None,
    };

    let samples_per_frame: u32 = if is_v1 { 1152 } else { 576 };
    // Layer III frame size: floor(samples/8 * bitrate_bps / sample_rate) + padding
    let frame_size = (samples_per_frame as usize / 8)
        * (bitrate_kbps as usize) * 1000
        / (sr as usize)
        + padding;
    let dur_ms = (samples_per_frame as u64 * 1000) / (sr as u64);

    Some((frame_size, dur_ms))
}

/// Walk the audio frames of a finished MP3 and return absolute file byte
/// offsets corresponding to each chapter start time (ms, cumulative from
/// start of the merged file). The leading Xing/Info silent frame written by
/// the muxer is detected and skipped, so chapter 0 maps to the first real
/// audio frame rather than the silent Xing.
fn chapter_byte_offsets_by_time(data: &[u8], chapter_starts_ms: &[u64]) -> Result<Vec<u32>, String> {
    if data.len() < 10 || &data[..3] != b"ID3" {
        return Err("Output is missing an ID3v2 header".to_string());
    }
    let tag_size = ((data[6] as u32 & 0x7f) << 21)
        | ((data[7] as u32 & 0x7f) << 14)
        | ((data[8] as u32 & 0x7f) << 7)
        | (data[9] as u32 & 0x7f);
    let mut pos: usize = (10 + tag_size) as usize;

    // Detect a leading Xing/Info frame and skip past it so chapter 0 lands on
    // the first real audio sample, not on the muxer's silent header frame.
    if pos + 4 <= data.len() {
        let h = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        if let Some((fsize, _)) = mp3_frame_info(h) {
            let scan_end = (pos + fsize).min(data.len());
            let body = &data[pos..scan_end];
            let has_xing = body.windows(4)
                .any(|w| w == b"Xing" || w == b"Info" || w == b"VBRI");
            if has_xing { pos += fsize; }
        }
    }

    let mut t_ms: u64 = 0;
    let mut offsets: Vec<u32> = Vec::with_capacity(chapter_starts_ms.len());
    let mut next: usize = 0;

    while pos + 4 <= data.len() && next < chapter_starts_ms.len() {
        // Resync on any byte that isn't a frame-sync.
        if data[pos] != 0xFF || (data[pos + 1] & 0xE0) != 0xE0 {
            pos += 1;
            continue;
        }
        let h = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        let (frame_size, dur_ms) = match mp3_frame_info(h) {
            Some(v) => v,
            None => { pos += 1; continue; }
        };

        // A chapter starts on the frame whose audio reaches its target time.
        while next < chapter_starts_ms.len() && chapter_starts_ms[next] <= t_ms {
            offsets.push(pos as u32);
            next += 1;
        }

        pos += frame_size;
        t_ms = t_ms.saturating_add(dur_ms);
    }

    // Any chapters past the audio (shouldn't happen, but be defensive) get
    // pinned at the last byte of the file.
    while next < chapter_starts_ms.len() {
        offsets.push((data.len() - 1) as u32);
        next += 1;
    }

    Ok(offsets)
}

/// Patch the start_offset / end_offset fields of every ID3v2 CHAP frame in a
/// finished `.mp3` file so they point at real byte positions, replacing the
/// "unset" sentinel 0xFFFFFFFF that ffmpeg writes by default.
///
/// Per the ID3v2 chapter-frame spec, players SHOULD treat 0xFFFFFFFF as
/// "use the time fields instead". Apple Books does not — it reads those
/// bytes as real file offsets, lands far past EOF, and renders every chapter
/// as 0:00. Filling in real offsets fixes chapter durations and seek targets
/// in Books while remaining valid for every other player.
///
/// Offsets are computed by walking the actual MP3 frame stream of the output
/// file, mapping each chapter's start time to the byte position of the frame
/// at that time. This is robust against the muxer's leading Xing frame and
/// any MP3 padding subtleties that defeat naive size-summing approaches.
///
/// `chapter_durations_seconds` must list each chapter's duration in file
/// order; their cumulative sum yields the chapter start times.
pub fn set_chap_byte_offsets(path: &str, chapter_durations_seconds: &[f64]) -> Result<(), String> {
    if chapter_durations_seconds.is_empty() {
        return Ok(());
    }
    let mut data = fs::read(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;

    let mut chapter_starts_ms: Vec<u64> = Vec::with_capacity(chapter_durations_seconds.len());
    let mut cum_ms: u64 = 0;
    for d in chapter_durations_seconds {
        chapter_starts_ms.push(cum_ms);
        cum_ms = cum_ms.saturating_add((d.max(0.0) * 1000.0) as u64);
    }

    let starts = chapter_byte_offsets_by_time(&data, &chapter_starts_ms)?;

    let id3_version_major = data[3];
    let tag_size = ((data[6] as u32 & 0x7f) << 21)
        | ((data[7] as u32 & 0x7f) << 14)
        | ((data[8] as u32 & 0x7f) << 7)
        | (data[9] as u32 & 0x7f);

    let mut pos: usize = 10;
    let tag_end = (10 + tag_size as usize).min(data.len());
    let mut chap_idx: usize = 0;
    let file_end_inclusive: u32 = (data.len() - 1) as u32;

    while pos + 10 <= tag_end {
        let fid = &data[pos..pos + 4];
        if fid == [0u8; 4] { break; }

        let s = &data[pos + 4..pos + 8];
        let frame_size = if id3_version_major >= 4 {
            ((s[0] as u32 & 0x7f) << 21) | ((s[1] as u32 & 0x7f) << 14)
                | ((s[2] as u32 & 0x7f) << 7) | (s[3] as u32 & 0x7f)
        } else {
            ((s[0] as u32) << 24) | ((s[1] as u32) << 16)
                | ((s[2] as u32) << 8) | (s[3] as u32)
        } as usize;

        if fid == b"CHAP" {
            let body_start = pos + 10;
            let body_end = (body_start + frame_size).min(data.len());
            let mut z = body_start;
            while z < body_end && data[z] != 0 { z += 1; }
            if z + 17 > body_end {
                return Err(format!("Malformed CHAP frame at byte {}", pos));
            }
            let off_pos = z + 1 + 8;

            if let Some(&start) = starts.get(chap_idx) {
                let end_inclusive: u32 = starts.get(chap_idx + 1)
                    .map(|n| n.saturating_sub(1))
                    .unwrap_or(file_end_inclusive);
                data[off_pos..off_pos + 4].copy_from_slice(&start.to_be_bytes());
                data[off_pos + 4..off_pos + 8].copy_from_slice(&end_inclusive.to_be_bytes());
            }
            chap_idx += 1;
        }

        pos += 10 + frame_size;
    }

    fs::write(path, &data).map_err(|e| format!("Failed to write {}: {}", path, e))?;
    Ok(())
}

pub fn concat_aac_files(files: &[PathBuf], tmp_dir: &Path) -> Result<PathBuf, String> {
    let concat_list = tmp_dir.join("concat.txt");
    let mut f = fs::File::create(&concat_list)
        .map_err(|e| format!("Failed to create concat list: {}", e))?;
    for path in files {
        let s = path_str(path)?;
        validate_concat_path(s)?;
        writeln!(f, "file '{}'", s.replace('\'', "'\\''"))
            .map_err(|e| format!("Failed to write concat list: {}", e))?;
    }

    let output = tmp_dir.join("merged.m4a");
    let concat_list_str = path_str(&concat_list)?;
    let output_str = path_str(&output)?.to_string();
    run_ffmpeg_with_progress(
        &[
            "-y", "-f", "concat", "-safe", "0",
            "-i", concat_list_str,
            "-map", "0:a",
            "-c", "copy",
            &output_str,
        ],
        0.0,
        |_| {},
        "concat",
    )?;

    Ok(output)
}

pub fn add_metadata_and_cover(
    input: &str,
    output: &str,
    config: &MergeConfig,
    durations: &[f64],
    tmp_dir: &Path,
    format: OutputFormat,
) -> Result<(), String> {
    let metadata_file = tmp_dir.join("ffmetadata.txt");
    let metadata_content = generate_ffmetadata(&config.files, durations, config);
    fs::write(&metadata_file, &metadata_content)
        .map_err(|e| format!("Failed to write metadata: {}", e))?;
    let metadata_str = path_str(&metadata_file)?.to_string();

    let cover = config.cover_art_path.as_ref().filter(|p| Path::new(p).exists());
    let has_cover = cover.is_some();

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(), input.into(),
        "-i".into(), metadata_str,
    ];

    if let Some(cover) = cover {
        args.extend_from_slice(&[
            "-i".into(), cover.clone(),
        ]);
    }

    args.extend_from_slice(&[
        "-map_metadata".into(), "1".into(),
        // Drop any per-stream tags the source MP3s carried (iTunes-specific
        // PRIV/TXXX, play counters, etc.) so the output starts clean.
        "-map_metadata:s:a:0".into(), "-1".into(),
    ]);

    if has_cover {
        args.extend_from_slice(&[
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
    ]);

    match format {
        OutputFormat::Mp4 => {
            args.extend_from_slice(&[
                "-f".into(), "mp4".into(),
            ]);
        }
        // ID3v2.3 has the broadest player support for CHAP/CTOC chapter frames
        // and APIC cover art (Apple Books, audio-file apps, foobar2000, VLC, etc.).
        // Xing header is required for accurate VBR duration — without it some
        // players (incl. Apple Books) misread duration and behave erratically.
        OutputFormat::Mp3 => {
            args.extend_from_slice(&[
                "-id3v2_version".into(), "3".into(),
                "-write_id3v2".into(), "1".into(),
                "-write_xing".into(), "1".into(),
                "-f".into(), "mp3".into(),
            ]);
        }
    }

    args.push(output.into());

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_ffmpeg_with_progress(&arg_refs, 0.0, |_| {}, "metadata")
}
