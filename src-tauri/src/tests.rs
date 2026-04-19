//! Synthetic-fixture integration tests.
//!
//! Generate tiny sine-wave clips per format via ffmpeg, then exercise the
//! merge pipeline end-to-end without requiring any external audiobook files.

use crate::binaries::{FFMPEG_PATH, FFPROBE_PATH};
use crate::merge::merge_audiobook_core;
use crate::plan::get_merge_plan;
use crate::probe::probe_all_files;
use crate::types::{FileEntry, FilePlanInfo, MergeConfig};
use crate::util::clean_chapter_name;
use std::path::Path;

/// Generate a sine-wave audio file in `codec` format at `path`.
fn gen_sine(path: &Path, seconds: f64, sample_rate: u32, codec: &str) {
    let sine = format!("sine=frequency=440:duration={seconds}:sample_rate={sample_rate}");
    let mut cmd = std::process::Command::new(FFMPEG_PATH.as_str());
    cmd.args(["-y", "-f", "lavfi", "-i", &sine]);
    match codec {
        "wav" => { cmd.args(["-c:a", "pcm_s16le"]); }
        "flac" => { cmd.args(["-c:a", "flac"]); }
        "alac" => { cmd.args(["-c:a", "alac", "-f", "ipod"]); }
        "mp3" => { cmd.args(["-c:a", "libmp3lame", "-b:a", "128k"]); }
        "aac" | "m4b" => { cmd.args(["-c:a", "aac", "-b:a", "128k", "-f", "ipod"]); }
        "wma" => { cmd.args(["-c:a", "wmav2", "-b:a", "128k", "-f", "asf"]); }
        _ => panic!("unsupported codec: {codec}"),
    };
    cmd.arg(path.to_str().unwrap());
    let out = cmd.output().expect("ffmpeg spawn failed");
    if !out.status.success() {
        panic!(
            "ffmpeg gen_sine failed for codec={codec}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn gen_cover(path: &Path) {
    let out = std::process::Command::new(FFMPEG_PATH.as_str())
        .args([
            "-y", "-f", "lavfi",
            "-i", "color=c=orange:s=64x64:d=1",
            "-frames:v", "1",
            path.to_str().unwrap(),
        ])
        .output()
        .expect("ffmpeg spawn failed");
    assert!(out.status.success(), "cover gen failed: {}", String::from_utf8_lossy(&out.stderr));
}

fn make_fixtures(dir: &Path, count: usize, codec: &str) -> Vec<String> {
    let ext = match codec {
        "alac" | "aac" => "m4a",
        "m4b" => "m4b",
        other => other,
    };
    (1..=count)
        .map(|i| {
            let path = dir.join(format!("chapter_{i}.{ext}"));
            gen_sine(&path, 1.5, 44_100, codec);
            path.to_string_lossy().to_string()
        })
        .collect()
}

fn plan_infos(paths: &[String]) -> Vec<FilePlanInfo> {
    probe_all_files(paths.to_vec())
        .expect("probe failed")
        .into_iter()
        .map(|f| FilePlanInfo {
            path: f.path,
            codec: f.codec,
            sample_rate: f.sample_rate,
            channels: f.channels,
            duration: f.duration,
        })
        .collect()
}

fn probe_output(path: &str) -> serde_json::Value {
    let out = std::process::Command::new(FFPROBE_PATH.as_str())
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format", "-show_chapters", "-show_streams",
            path,
        ])
        .output()
        .expect("ffprobe spawn failed");
    assert!(out.status.success(), "ffprobe failed: {}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("invalid ffprobe JSON")
}

fn test_config(paths: &[String], out_dir: &Path, out_name: &str) -> MergeConfig {
    MergeConfig {
        files: paths.iter().map(|p| FileEntry {
            path: p.clone(),
            chapter_name: clean_chapter_name(Path::new(p).file_stem().unwrap().to_str().unwrap()),
        }).collect(),
        output_dir: out_dir.to_string_lossy().to_string(),
        output_filename: out_name.to_string(),
        title: Some("Test Book".to_string()),
        artist: Some("Test Author".to_string()),
        album: Some("Test Series".to_string()),
        narrator: Some("Test Narrator".to_string()),
        year: Some("2025".to_string()),
        cover_art_path: None,
        bitrate: 64,
        mono: false,
        force_transcode: false,
        durations: None,
        output_codec: None,
    }
}

fn run_merge(config: MergeConfig) -> String {
    merge_audiobook_core(config, |_, _, _| {}).expect("merge_audiobook_core failed")
}

fn assert_chapters(output: &str, expected: usize) -> f64 {
    let json = probe_output(output);
    let duration: f64 = json["format"]["duration"].as_str().unwrap().parse().unwrap();
    assert!(duration > 0.5 * expected as f64, "duration too short: {duration}");
    let chapters = json["chapters"].as_array().unwrap();
    assert_eq!(chapters.len(), expected, "expected {expected} chapters, got {}", chapters.len());
    duration
}

fn format_tag(json: &serde_json::Value, key: &str) -> Option<String> {
    json["format"]["tags"][key].as_str().map(|s| s.to_string())
}

// ── Probe tests (codec normalization) ───────────────────────────────────

#[test]
fn probe_normalizes_wav() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 1, "wav");
    let probed = probe_all_files(paths).expect("probe failed");
    assert_eq!(probed[0].codec, "wav");
}

#[test]
fn probe_normalizes_flac() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 1, "flac");
    let probed = probe_all_files(paths).expect("probe failed");
    assert_eq!(probed[0].codec, "flac");
}

#[test]
fn probe_normalizes_alac() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 1, "alac");
    let probed = probe_all_files(paths).expect("probe failed");
    assert_eq!(probed[0].codec, "alac");
}

#[test]
fn probe_normalizes_mp3() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 1, "mp3");
    let probed = probe_all_files(paths).expect("probe failed");
    assert_eq!(probed[0].codec, "mp3");
}

#[test]
fn probe_normalizes_aac() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 1, "aac");
    let probed = probe_all_files(paths).expect("probe failed");
    assert_eq!(probed[0].codec, "aac");
}

// ── Merge plan tests ────────────────────────────────────────────────────

#[test]
fn plan_aac_remux_synthetic() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 3, "aac");
    let plan = get_merge_plan(plan_infos(&paths)).expect("plan failed");
    assert_eq!(plan.strategy, "remux", "uniform AAC should remux");
}

#[test]
fn plan_wav_transcode() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 2, "wav");
    let plan = get_merge_plan(plan_infos(&paths)).expect("plan failed");
    assert_ne!(plan.strategy, "remux", "WAV must transcode");
    assert_eq!(plan.needs_transcode.len(), paths.len());
}

#[test]
fn plan_flac_transcode() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 2, "flac");
    let plan = get_merge_plan(plan_infos(&paths)).expect("plan failed");
    assert_ne!(plan.strategy, "remux");
    assert_eq!(plan.needs_transcode.len(), paths.len());
}

#[test]
fn plan_alac_transcode() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 2, "alac");
    let plan = get_merge_plan(plan_infos(&paths)).expect("plan failed");
    assert_ne!(plan.strategy, "remux", "ALAC must transcode to AAC");
    assert_eq!(plan.needs_transcode.len(), paths.len());
}

// ── End-to-end merge tests per format ───────────────────────────────────

#[test]
fn merge_wav_to_m4b() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 2, "wav");
    let output = run_merge(test_config(&paths, tmp.path(), "wav_merged"));
    assert_chapters(&output, 2);
}

#[test]
fn merge_flac_to_m4b() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 2, "flac");
    let output = run_merge(test_config(&paths, tmp.path(), "flac_merged"));
    assert_chapters(&output, 2);
}

#[test]
fn merge_alac_to_m4b() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 2, "alac");
    let output = run_merge(test_config(&paths, tmp.path(), "alac_merged"));
    assert_chapters(&output, 2);
}

#[test]
fn merge_mp3_synthetic() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 2, "mp3");
    let output = run_merge(test_config(&paths, tmp.path(), "mp3_merged"));
    assert_chapters(&output, 2);
}

#[test]
fn merge_aac_remux_synthetic() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 3, "aac");
    let output = run_merge(test_config(&paths, tmp.path(), "aac_remux"));
    assert_chapters(&output, 3);
}

#[test]
fn merge_mixed_formats() {
    let tmp = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    paths.push({
        let p = tmp.path().join("01_mp3.mp3");
        gen_sine(&p, 1.5, 44_100, "mp3");
        p.to_string_lossy().to_string()
    });
    paths.push({
        let p = tmp.path().join("02_wav.wav");
        gen_sine(&p, 1.5, 44_100, "wav");
        p.to_string_lossy().to_string()
    });
    paths.push({
        let p = tmp.path().join("03_flac.flac");
        gen_sine(&p, 1.5, 44_100, "flac");
        p.to_string_lossy().to_string()
    });
    let output = run_merge(test_config(&paths, tmp.path(), "mixed_merged"));
    assert_chapters(&output, 3);
}

// ── Metadata + cover art ────────────────────────────────────────────────

#[test]
fn merge_tags_metadata_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 2, "mp3");
    let config = MergeConfig {
        files: paths.iter().map(|p| FileEntry {
            path: p.clone(),
            chapter_name: clean_chapter_name(Path::new(p).file_stem().unwrap().to_str().unwrap()),
        }).collect(),
        output_dir: tmp.path().to_string_lossy().to_string(),
        output_filename: "tagged".to_string(),
        title: Some("My Book".to_string()),
        artist: Some("Author Name".to_string()),
        album: Some("The Series".to_string()),
        narrator: Some("The Narrator".to_string()),
        year: Some("2024".to_string()),
        cover_art_path: None,
        bitrate: 64,
        mono: false,
        force_transcode: false,
        durations: None,
        output_codec: None,
    };
    let output = run_merge(config);
    let json = probe_output(&output);
    assert_eq!(format_tag(&json, "title").as_deref(), Some("My Book"));
    assert_eq!(format_tag(&json, "artist").as_deref(), Some("Author Name"));
    assert_eq!(format_tag(&json, "album").as_deref(), Some("The Series"));
    let date = format_tag(&json, "date").or_else(|| format_tag(&json, "year"));
    assert!(date.as_deref().map(|d| d.starts_with("2024")).unwrap_or(false),
        "expected year 2024, got {date:?}");
    let composer = format_tag(&json, "composer");
    assert_eq!(composer.as_deref(), Some("The Narrator"),
        "narrator is stored as composer tag");
}

#[test]
fn merge_embeds_cover_art() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 2, "mp3");
    let cover = tmp.path().join("cover.png");
    gen_cover(&cover);
    let mut config = test_config(&paths, tmp.path(), "with_cover");
    config.cover_art_path = Some(cover.to_string_lossy().to_string());
    let output = run_merge(config);
    let json = probe_output(&output);
    let streams = json["streams"].as_array().unwrap();
    let has_video = streams.iter().any(|s| s["codec_type"] == "video");
    assert!(has_video, "output should contain embedded cover art stream");
}

#[test]
fn merge_mono_produces_single_channel() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 2, "mp3");
    let mut config = test_config(&paths, tmp.path(), "mono");
    config.mono = true;
    let output = run_merge(config);
    let json = probe_output(&output);
    let audio = json["streams"].as_array().unwrap().iter()
        .find(|s| s["codec_type"] == "audio").unwrap();
    assert_eq!(audio["channels"].as_u64(), Some(1), "mono output should be 1-channel");
}

#[test]
fn merge_preserves_chapter_names() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 3, "mp3");
    let config = test_config(&paths, tmp.path(), "named_chapters");
    let output = run_merge(config);
    let json = probe_output(&output);
    let chapters = json["chapters"].as_array().unwrap();
    assert_eq!(chapters.len(), 3);
    for (i, ch) in chapters.iter().enumerate() {
        let title = ch["tags"]["title"].as_str().unwrap_or("");
        assert!(!title.is_empty(), "chapter {i} should have a title");
    }
}

#[test]
fn merge_force_transcode_on_aac() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = make_fixtures(tmp.path(), 2, "aac");
    let mut config = test_config(&paths, tmp.path(), "forced");
    config.force_transcode = true;
    config.bitrate = 48;
    let output = run_merge(config);
    assert_chapters(&output, 2);
}

/// Regression: long input would previously hang because ffmpeg's stdout/stderr
/// pipes filled up and were never drained. Uses a cover-art stream to maximize
/// decoder chatter on stderr.
#[test]
fn merge_long_mp3_with_cover_does_not_hang() {
    let tmp = tempfile::tempdir().unwrap();
    let mp3 = tmp.path().join("01_long.mp3");
    let mut cmd = std::process::Command::new(FFMPEG_PATH.as_str());
    cmd.args([
        "-hide_banner", "-loglevel", "warning", "-y",
        "-f", "lavfi", "-i", "sine=frequency=440:duration=90:sample_rate=44100",
        "-c:a", "libmp3lame", "-b:a", "128k",
        mp3.to_str().unwrap(),
    ]);
    assert!(cmd.status().unwrap().success());

    let config = test_config(&[mp3.to_string_lossy().to_string()], tmp.path(), "long_merged");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = run_merge(config);
        let _ = tx.send(out);
    });
    let output = rx.recv_timeout(std::time::Duration::from_secs(60))
        .expect("transcode hung — pipe likely not being drained");
    assert!(Path::new(&output).exists());
}

#[test]
fn merge_lossless_alac_output() {
    let tmp = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    paths.push({
        let p = tmp.path().join("01.flac");
        gen_sine(&p, 1.5, 44_100, "flac");
        p.to_string_lossy().to_string()
    });
    paths.push({
        let p = tmp.path().join("02.wav");
        gen_sine(&p, 1.5, 44_100, "wav");
        p.to_string_lossy().to_string()
    });
    let mut config = test_config(&paths, tmp.path(), "alac_out");
    config.output_codec = Some("alac".to_string());
    let output = run_merge(config);
    let json = probe_output(&output);
    let audio = json["streams"].as_array().unwrap().iter()
        .find(|s| s["codec_type"] == "audio").unwrap();
    assert_eq!(audio["codec_name"].as_str(), Some("alac"),
        "lossless output should be ALAC");
    assert_chapters(&output, 2);
}
