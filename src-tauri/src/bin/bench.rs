use bindery_lib::{merge_audiobook_core, FileEntry, MergeConfig};
use std::time::Instant;

fn main() {
    let bench_dir = "/tmp/bindery-bench";
    let output_dir = "/tmp/bindery-bench-output";
    std::fs::create_dir_all(output_dir).unwrap();

    // Collect files
    let mut paths: Vec<String> = std::fs::read_dir(bench_dir)
        .unwrap()
        .filter_map(|e| {
            let p = e.ok()?.path();
            if p.extension()?.to_str()? == "mp3" {
                Some(p.to_str()?.to_string())
            } else {
                None
            }
        })
        .collect();
    paths.sort();

    println!("Benchmarking: {} MP3 files (5 min each = 100 min total)", paths.len());
    println!("CPU cores: {}", num_cpus::get());

    // Probe
    let t0 = Instant::now();
    let probed = bindery_lib::probe_all_files(paths.clone()).unwrap();
    let probe_time = t0.elapsed();
    println!("Probe: {:.1}ms", probe_time.as_millis());

    let total_dur: f64 = probed.iter().map(|f| f.duration).sum();
    println!("Total duration: {:.0}s ({:.1} min)", total_dur, total_dur / 60.0);

    // Merge
    let files: Vec<FileEntry> = probed.iter().map(|p| FileEntry {
        path: p.path.clone(),
        chapter_name: p.chapter_name.clone(),
    }).collect();

    let _ = std::fs::remove_file(format!("{}/bench_output.m4b", output_dir));

    let config = MergeConfig {
        files,
        output_dir: output_dir.to_string(),
        output_filename: "bench_output".to_string(),
        title: Some("Benchmark Book".to_string()),
        artist: Some("Test".to_string()),
        album: None,
        narrator: None,
        year: None,
        cover_art_path: None,
        bitrate: 64,
        mono: true,
        force_transcode: false,
        durations: None,
    };

    let t1 = Instant::now();
    let result = merge_audiobook_core(config, |stage, pct, msg| {
        let elapsed = t1.elapsed().as_secs_f64();
        println!("  [{:>12}] {:5.1}% ({:.1}s) — {}", stage, pct, elapsed, msg);
    });

    let merge_time = t1.elapsed();

    match result {
        Ok(path) => {
            let size = std::fs::metadata(&path).unwrap().len();
            println!("\nOutput: {} ({:.1} MB)", path, size as f64 / 1_048_576.0);
        }
        Err(e) => println!("\nERROR: {}", e),
    }

    println!("\n=== TIMING ===");
    println!("Probe:     {:.1}ms", probe_time.as_millis());
    println!("Merge:     {:.1}s", merge_time.as_secs_f64());
    println!("Total:     {:.1}s", (probe_time + merge_time).as_secs_f64());
    println!("Realtime:  {:.0}x", total_dur / merge_time.as_secs_f64());
}
