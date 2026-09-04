use crate::binaries::{ffmpeg, CANCEL_FLAG};
use crate::types::Stage;
use crate::util::{path_str, short_filename};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// aac_at (AudioToolbox) rejects bitrates above roughly 3.5 bits per sample
/// per channel at 44.1 kHz and up, with a practical ceiling of 320 kbps. Below
/// 44.1 kHz the real limit is lower — measured with the bundled ffmpeg:
/// 22.05 kHz mono takes 64 kbps and rejects 66 kbps, 22.05 kHz stereo takes
/// 128 kbps and rejects 140 kbps — so use 2.9 bits/sample/channel there.
/// A rejected bitrate does not fail loudly: ffmpeg writes an empty file and
/// exits 0. Clamp the requested bitrate so the encoder can always open.
pub fn clamp_aac_bitrate(bitrate: &str, sample_rate: u32, channels: u32) -> String {
    let Ok(kbps) = bitrate.trim_end_matches('k').parse::<u32>() else {
        return bitrate.to_string();
    };
    let milli_bits_per_sample: u64 = if sample_rate >= 44_100 { 3_500 } else { 2_900 };
    let ceiling = ((sample_rate as u64 * channels as u64 * milli_bits_per_sample) / 1_000_000) as u32;
    let max_kbps = ceiling.clamp(32, 320);
    format!("{}k", kbps.min(max_kbps))
}

#[allow(clippy::too_many_arguments)]
pub fn transcode_parallel<F>(
    items: &[(usize, String)],
    tmp_dir: &Path,
    codec: &str,
    bitrate: &str,
    channels: Option<&str>,
    sample_rate: Option<u32>,
    exact_sample_rate: bool,
    durations: &[f64],
    emit: &F,
    pct_start: f64,
    pct_end: f64,
) -> Result<Vec<PathBuf>, String>
where
    F: Fn(Stage, f64, &str) + Sync,
{
    if items.is_empty() {
        return Ok(vec![]);
    }

    let max_threads = num_cpus::get().max(2) - 1;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(max_threads)
        .build()
        .map_err(|e| format!("Thread pool error: {}", e))?;

    let total_duration: f64 = items.iter().map(|(idx, _)| durations.get(*idx).copied().unwrap_or(0.0)).sum();
    let completed_duration = std::sync::Mutex::new(0.0_f64);
    let pct_range = pct_end - pct_start;
    let total = items.len();
    let completed_count = AtomicUsize::new(0);

    pool.install(|| {
        items
            .par_iter()
            .map(|(idx, path)| {
                if CANCEL_FLAG.load(Ordering::Relaxed) {
                    return Err("Cancelled by user".to_string());
                }

                let file_duration = durations.get(*idx).copied().unwrap_or(0.0);
                let is_aac = matches!(codec, "aac" | "aac_at" | "libfdk_aac");
                let is_mp3 = codec == "libmp3lame";
                let ext = if is_mp3 { "mp3" } else { "m4a" };
                let temp_out = tmp_dir.join(format!("part_{:04}.{}", idx, ext));
                let temp_str = path_str(&temp_out)?.to_string();

                // aac_at caps bitrate by (sample_rate × channels). 22 kHz sources
                // paired with typical voice-content bitrates fail to open the encoder.
                // Upsample to 44.1 kHz minimum when encoding to AAC — unless the
                // caller is normalising outliers to match files that pass through
                // untouched: then the requested rate must be honoured exactly, or
                // the concatenated stream changes sample rate mid-way, ffmpeg drops
                // everything after the change (still exiting 0) and Apple's decoder
                // refuses the file. clamp_aac_bitrate keeps the encoder happy there.
                let effective_sr = match (is_aac, sample_rate) {
                    (true, Some(sr)) if sr < 44_100 && !exact_sample_rate => Some(44_100),
                    (true, None) => Some(44_100),
                    (_, sr) => sr,
                };

                let effective_ch: u32 = match channels {
                    Some("1") => 1,
                    Some("2") => 2,
                    _ => 2,
                };

                let encode_bitrate = if is_aac {
                    clamp_aac_bitrate(bitrate, effective_sr.unwrap_or(44_100), effective_ch)
                } else {
                    bitrate.to_string()
                };

                let mut args = vec![
                    "-y".to_string(),
                    "-progress".to_string(), "pipe:1".to_string(),
                    "-i".to_string(), path.clone(),
                    "-c:a".to_string(), codec.to_string(),
                ];

                if is_aac {
                    args.push("-b:a".to_string());
                    args.push(encode_bitrate);
                    match codec {
                        "aac_at" => {
                            // Constrained VBR — bitrate-targeted but varies per frame.
                            // Meaningfully better quality than CBR at the same avg rate.
                            args.push("-aac_at_mode".to_string());
                            args.push("cvbr".to_string());
                        }
                        "libfdk_aac" => {
                            // libfdk defaults to ABR when -b:a is set — already near-VBR quality.
                            // Explicit -vbr 0 keeps the encoder in ABR mode.
                            args.push("-vbr".to_string());
                            args.push("0".to_string());
                        }
                        _ => {}
                    }
                } else if is_mp3 {
                    // VBR quality 0 ≈ 245 kbps, transparent for spoken-word content.
                    args.push("-q:a".to_string());
                    args.push("0".to_string());
                }

                if let Some(ch) = channels {
                    args.push("-ac".to_string());
                    args.push(ch.to_string());
                }

                args.push("-threads".to_string());
                args.push("0".to_string());
                args.push("-vn".to_string());

                if let Some(sr) = effective_sr {
                    args.push("-ar".to_string());
                    args.push(sr.to_string());
                }

                args.push(temp_str);

                let mut child = ffmpeg()
                    .args(&args)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("ffmpeg transcode failed: {}", e))?;

                // Drain ffmpeg's stdout/stderr on background threads. If we don't,
                // the OS pipe buffer (~64 KB) fills on long files and ffmpeg blocks
                // forever on its next write, hanging the whole app.
                let stdout_pipe = child.stdout.take();
                let stderr_pipe = child.stderr.take();
                let stdout_thread = stdout_pipe.map(|mut s| {
                    std::thread::spawn(move || {
                        let mut buf = [0u8; 4096];
                        while let Ok(n) = std::io::Read::read(&mut s, &mut buf) {
                            if n == 0 { break; }
                        }
                    })
                });
                let stderr_buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
                let stderr_thread = stderr_pipe.map(|mut s| {
                    let buf = std::sync::Arc::clone(&stderr_buf);
                    std::thread::spawn(move || {
                        let mut sink = String::new();
                        std::io::Read::read_to_string(&mut s, &mut sink).ok();
                        if let Ok(mut guard) = buf.lock() { *guard = sink; }
                    })
                });

                let mut last_emit = std::time::Instant::now();

                let status = loop {
                    match child.try_wait() {
                        Ok(Some(status)) => break status,
                        Ok(None) => {
                            if CANCEL_FLAG.load(Ordering::Relaxed) {
                                let _ = child.kill();
                                let _ = child.wait();
                                if let Some(t) = stdout_thread { let _ = t.join(); }
                                if let Some(t) = stderr_thread { let _ = t.join(); }
                                return Err("Cancelled by user".to_string());
                            }
                            if last_emit.elapsed().as_millis() > 500 {
                                last_emit = std::time::Instant::now();
                                if let Ok(meta) = std::fs::metadata(&temp_out) {
                                    let written = meta.len() as f64;
                                    let bitrate_bps: f64 = if is_mp3 {
                                        245_000.0
                                    } else {
                                        match bitrate.trim_end_matches('k').parse::<f64>() {
                                            Ok(v) => v * 1000.0,
                                            Err(_) => 0.0,
                                        }
                                    };
                                    let expected_bytes = bitrate_bps * file_duration / 8.0;
                                    if expected_bytes > 0.0 {
                                        let file_frac = (written / expected_bytes).min(0.95);
                                        let done_dur = completed_duration.lock().map(|d| *d).unwrap_or(0.0);
                                        let overall = (done_dur + file_frac * file_duration) / total_duration;
                                        let pct = pct_start + pct_range * overall.min(1.0);
                                        let done = completed_count.load(Ordering::Relaxed);
                                        let name = short_filename(path);
                                        emit(Stage::Transcoding, pct, &format!("Transcoding {}/{} — {}", done + 1, total, name));
                                    }
                                }
                            }
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                        Err(e) => return Err(format!("ffmpeg wait error: {}", e)),
                    }
                };

                if let Some(t) = stdout_thread { let _ = t.join(); }
                if let Some(t) = stderr_thread { let _ = t.join(); }

                if !status.success() {
                    let stderr = stderr_buf.lock().map(|g| g.clone()).unwrap_or_default();
                    return Err(format!(
                        "Transcode failed for {}: {}",
                        path, stderr
                    ));
                }

                let new_completed_dur = {
                    let mut d = completed_duration.lock().expect("completed_duration lock poisoned");
                    *d += file_duration;
                    *d
                };
                let done = completed_count.fetch_add(1, Ordering::Relaxed) + 1;
                let weighted_pct = if total_duration > 0.0 {
                    pct_start + (pct_range * (new_completed_dur / total_duration).min(1.0))
                } else {
                    pct_start + (pct_range * done as f64 / total as f64)
                };
                let name = short_filename(path);
                emit(
                    Stage::Transcoding,
                    weighted_pct,
                    &format!("Transcoded {}/{} — {}", done.min(total), total, name),
                );

                Ok(temp_out)
            })
            .collect()
    })
}
