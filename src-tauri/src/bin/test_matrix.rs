use bindery_lib::{merge_audiobook_core, probe_all_files, FileEntry, MergeConfig};
use std::path::PathBuf;
use std::process::Command;

const TEST_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test-audiobooks");

fn main() {
    let output_dir = std::env::temp_dir().join("bindery_test_output");
    std::fs::create_dir_all(&output_dir).unwrap();

    let cases = vec![
        ("pure-aac", "Remux path (all AAC, uniform SR)"),
        ("pure-mp3", "Transcode path (all MP3)"),
        ("mixed", "Mixed path (AAC + MP3)"),
        ("single-file", "Single file edge case"),
        ("special-chars", "Filenames with special characters"),
        ("no-metadata", "Files with no embedded metadata"),
        ("mismatched-sr", "AAC files with different sample rates"),
    ];

    let mut passed = 0;
    let mut failed = 0;

    for (dir_name, description) in &cases {
        println!("\n{}", "=".repeat(60));
        println!("TEST: {} — {}", dir_name, description);
        println!("{}", "=".repeat(60));

        let test_path = PathBuf::from(TEST_DIR).join(dir_name);
        if !test_path.exists() {
            println!("  SKIP: directory does not exist");
            continue;
        }

        match run_test_case(dir_name, &test_path, &output_dir) {
            Ok(()) => {
                println!("  PASS");
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL: {}", e);
                failed += 1;
            }
        }
    }

    println!("\n{}", "=".repeat(60));
    println!("RESULTS: {} passed, {} failed", passed, failed);
    println!("{}", "=".repeat(60));

    if failed > 0 {
        std::process::exit(1);
    }
}

fn run_test_case(name: &str, test_path: &PathBuf, output_dir: &PathBuf) -> Result<(), String> {
    // Collect audio files
    let mut paths: Vec<String> = std::fs::read_dir(test_path)
        .map_err(|e| format!("read dir: {}", e))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let ext = path.extension()?.to_str()?.to_lowercase();
            if ["m4a", "mp3", "aac", "flac", "ogg"].contains(&ext.as_str()) {
                Some(path.to_str()?.to_string())
            } else {
                None
            }
        })
        .collect();
    paths.sort();

    if paths.is_empty() {
        return Err("No audio files found".to_string());
    }

    println!("  Files: {}", paths.len());

    // Probe files
    let probed = probe_all_files(paths.clone())?;
    for p in &probed {
        println!(
            "    {} — codec:{} sr:{} ch:{} dur:{:.1}s",
            p.filename, p.codec, p.sample_rate, p.channels, p.duration
        );
    }

    // Build merge config
    let files: Vec<FileEntry> = probed
        .iter()
        .map(|p| FileEntry {
            path: p.path.clone(),
            chapter_name: p.chapter_name.clone(),
        })
        .collect();

    let output_filename = format!("test_{}", name);
    let output_path = output_dir.join(format!("{}.m4b", &output_filename));

    // Remove previous output if exists
    let _ = std::fs::remove_file(&output_path);

    let config = MergeConfig {
        files,
        output_dir: output_dir.to_str().unwrap().to_string(),
        output_filename,
        title: Some(format!("Test: {}", name)),
        artist: Some("Test Author".to_string()),
        album: None,
        narrator: None,
        year: None,
        cover_art_path: None,
        bitrate: 64,
        mono: false,
        force_transcode: false,
        durations: None,
    };

    // Run merge
    println!("  Merging...");
    let result = merge_audiobook_core(config, |stage, pct, msg| {
        println!("    [{:>12}] {:5.1}% — {}", stage, pct, msg);
    })?;
    println!("  Output: {}", result);

    // Verify output exists
    if !PathBuf::from(&result).exists() {
        return Err("Output file does not exist".to_string());
    }

    let file_size = std::fs::metadata(&result)
        .map_err(|e| format!("metadata: {}", e))?
        .len();
    println!("  Size: {} bytes", file_size);
    if file_size == 0 {
        return Err("Output file is empty".to_string());
    }

    // Verify with ffprobe — check it's valid and has chapters
    let ffprobe_out = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_chapters",
            "-show_format",
            &result,
        ])
        .output()
        .map_err(|e| format!("ffprobe: {}", e))?;

    if !ffprobe_out.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&ffprobe_out.stderr)
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&ffprobe_out.stdout)
        .map_err(|e| format!("parse ffprobe: {}", e))?;

    // Check chapters
    let chapters = json["chapters"].as_array();
    let num_chapters = chapters.map(|c| c.len()).unwrap_or(0);
    let expected_chapters = probed.len();
    println!("  Chapters: {} (expected {})", num_chapters, expected_chapters);

    if num_chapters != expected_chapters {
        return Err(format!(
            "Wrong chapter count: got {}, expected {}",
            num_chapters, expected_chapters
        ));
    }

    // Print chapter details
    if let Some(chapters) = chapters {
        for ch in chapters {
            let start: f64 = ch["start_time"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            let end: f64 = ch["end_time"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            let title = ch["tags"]["title"].as_str().unwrap_or("(no title)");
            println!("    Ch: {:.1}s–{:.1}s \"{}\"", start, end, title);
        }
    }

    // Check format
    let format_name = json["format"]["format_name"]
        .as_str()
        .unwrap_or("unknown");
    println!("  Format: {}", format_name);

    if !format_name.contains("mp4") && !format_name.contains("m4a") && !format_name.contains("mov") {
        return Err(format!("Unexpected format: {}", format_name));
    }

    Ok(())
}
