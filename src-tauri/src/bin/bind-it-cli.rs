use bind_it_lib::merge::merge_audio_files_core;
use bind_it_lib::probe::probe_all_files;
use bind_it_lib::types::{FileEntry, MergeConfig, Stage};
use bind_it_lib::util::clean_chapter_name;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

const AUDIO_EXTS: &[&str] = &["mp3", "m4a", "m4b", "aac", "wav", "flac", "wma"];
const COVER_STEMS: &[&str] = &["cover", "folder", "front", "artwork"];
const COVER_EXTS: &[&str] = &["jpg", "jpeg", "png"];
const NATIVE_M4B_CODECS: &[&str] = &["aac", "mp3", "alac"];

struct Args {
    input: PathBuf,
    output: Option<PathBuf>,
    compress: bool,
    bitrate: u32,
    mono: bool,
    recursive: bool,
    dry_run: bool,
    overwrite: bool,
}

fn print_help() {
    println!(
        "bind-it-cli — batch-merge audio file folders into a single chaptered file\n\
\n\
Usage:\n\
  bind-it-cli <INPUT_DIR> [OPTIONS]\n\
\n\
Behavior:\n\
  Scans INPUT_DIR for folders containing audio files. Each such folder is\n\
  merged into a single <folder-name> output with chapters (.m4b, or .mp3\n\
  when the source set is uniform MP3), written next to the source folder\n\
  by default.\n\
\n\
Options:\n\
  -o, --output <DIR>   Write all outputs to DIR instead of alongside inputs\n\
      --compress       Re-encode to AAC instead of lossless remux\n\
      --bitrate <K>    AAC bitrate when compressing (default: 128)\n\
      --mono           Force mono (only meaningful with --compress)\n\
  -r, --recursive      Walk the full tree; default stops at immediate children\n\
      --dry-run        List the books that would be merged, don't run ffmpeg\n\
      --overwrite      Replace existing output files (default skips them)\n\
  -h, --help           Show this help\n"
    );
}

fn parse_args() -> Result<Args, String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut compress = false;
    let mut bitrate: u32 = 128;
    let mut mono = false;
    let mut recursive = false;
    let mut dry_run = false;
    let mut overwrite = false;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "-o" | "--output" => {
                i += 1;
                let v = raw.get(i).ok_or("--output requires a path")?;
                output = Some(PathBuf::from(v));
            }
            "--compress" => compress = true,
            "--bitrate" => {
                i += 1;
                let v = raw.get(i).ok_or("--bitrate requires a number")?;
                bitrate = v
                    .parse()
                    .map_err(|e| format!("invalid --bitrate '{v}': {e}"))?;
            }
            "--mono" => mono = true,
            "-r" | "--recursive" => recursive = true,
            "--dry-run" => dry_run = true,
            "--overwrite" => overwrite = true,
            s if s.starts_with('-') => return Err(format!("unknown option: {s}")),
            s => {
                if input.is_some() {
                    return Err(format!("unexpected argument: {s}"));
                }
                input = Some(PathBuf::from(s));
            }
        }
        i += 1;
    }
    let input = input.ok_or_else(|| "missing INPUT_DIR (see --help)".to_string())?;
    Ok(Args {
        input,
        output,
        compress,
        bitrate,
        mono,
        recursive,
        dry_run,
        overwrite,
    })
}

fn folder_audio_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if AUDIO_EXTS.contains(&ext.to_lowercase().as_str()) {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn find_cover(dir: &Path) -> Option<PathBuf> {
    for stem in COVER_STEMS {
        for ext in COVER_EXTS {
            let candidate = dir.join(format!("{stem}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
            // Try capitalized stem too.
            let cap = {
                let mut c = stem.chars();
                c.next()
                    .map(|first| first.to_uppercase().to_string() + c.as_str())
                    .unwrap_or_default()
            };
            let candidate = dir.join(format!("{cap}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn collect_books(
    dir: &Path,
    recursive: bool,
    depth: usize,
    seen: &mut HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) {
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if !seen.insert(canonical) {
        return;
    }
    if !folder_audio_files(dir).is_empty() {
        out.push(dir.to_path_buf());
    }
    // depth==0 always descends one level so "give me a library dir" works.
    let descend = recursive || depth == 0;
    if !descend {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut subs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    subs.sort();
    for sub in subs {
        if let Ok(meta) = std::fs::symlink_metadata(&sub) {
            if meta.file_type().is_symlink() {
                continue;
            }
        }
        collect_books(&sub, recursive, depth + 1, seen, out);
    }
}

fn decide_output_codec(files: &[PathBuf]) -> Option<String> {
    // Mirrors the frontend logic: if all files share the same native-to-M4B codec
    // (aac/mp3/alac), let the backend pick remux or outlier-only re-encode.
    // Anything else → ALAC encode to preserve lossless intent.
    let paths: Vec<String> = files.iter().map(|p| p.to_string_lossy().to_string()).collect();
    let probed = match probe_all_files(paths) {
        Ok(p) => p,
        Err(_) => return Some("alac".to_string()),
    };
    if probed.is_empty() {
        return Some("alac".to_string());
    }
    let first = probed[0].codec.as_str();
    let all_same_native = NATIVE_M4B_CODECS.contains(&first)
        && probed.iter().all(|f| f.codec == first);
    if all_same_native {
        None
    } else {
        Some("alac".to_string())
    }
}

enum MergeOutcome {
    Done(String),
    Skipped(String),
    Failed(String),
}

fn merge_one(book: &Path, args: &Args) -> MergeOutcome {
    let files = folder_audio_files(book);
    if files.is_empty() {
        return MergeOutcome::Failed("no audio files".to_string());
    }

    let title = book
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio_files")
        .to_string();

    let output_dir = args
        .output
        .clone()
        .or_else(|| book.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        return MergeOutcome::Failed(format!("mkdir {output_dir:?}: {e}"));
    }

    let output_filename = format!("{title}.m4b");
    let final_path = output_dir.join(&output_filename);
    if final_path.exists() {
        if args.overwrite {
            if let Err(e) = std::fs::remove_file(&final_path) {
                return MergeOutcome::Failed(format!("could not replace existing output: {e}"));
            }
        } else {
            return MergeOutcome::Skipped(final_path.display().to_string());
        }
    }

    let cover = find_cover(book);

    let output_codec = if args.compress {
        None
    } else {
        decide_output_codec(&files)
    };

    let file_entries: Vec<FileEntry> = files
        .iter()
        .map(|p| {
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("chapter");
            FileEntry {
                path: p.to_string_lossy().to_string(),
                chapter_name: clean_chapter_name(stem),
            }
        })
        .collect();

    let config = MergeConfig {
        files: file_entries,
        output_dir: output_dir.to_string_lossy().to_string(),
        output_filename,
        title: Some(title.clone()),
        artist: None,
        album: Some(title),
        narrator: None,
        year: None,
        cover_art_path: cover.map(|p| p.to_string_lossy().to_string()),
        bitrate: args.bitrate,
        mono: args.mono,
        force_transcode: args.compress,
        durations: None,
        output_codec,
        wrap_in_mp4: false,
    };

    let last_percent = std::sync::Mutex::new(-1.0_f64);
    let emit = |_stage: Stage, percent: f64, msg: &str| {
        let mut last = last_percent.lock().unwrap();
        if (percent - *last).abs() < 0.5 && percent < 100.0 {
            return;
        }
        *last = percent;
        let width = 28_usize;
        let filled = ((percent / 100.0) * width as f64).round() as usize;
        let filled = filled.min(width);
        let bar: String = "█".repeat(filled) + &"░".repeat(width - filled);
        print!("\r    {bar} {percent:>5.1}%  {msg}\x1b[K");
        std::io::stdout().flush().ok();
    };

    let result = merge_audio_files_core(config, emit);
    println!();
    match result {
        Ok(path) => MergeOutcome::Done(path),
        Err(e) => MergeOutcome::Failed(e),
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("run `bind-it-cli --help` for usage");
            return ExitCode::from(2);
        }
    };

    if !args.input.exists() {
        eprintln!("error: input does not exist: {}", args.input.display());
        return ExitCode::from(2);
    }
    if !args.input.is_dir() {
        eprintln!("error: input is not a directory: {}", args.input.display());
        return ExitCode::from(2);
    }

    let mut books = Vec::new();
    let mut seen = HashSet::new();
    collect_books(&args.input, args.recursive, 0, &mut seen, &mut books);

    if books.is_empty() {
        eprintln!("no folders with audio files found under {}", args.input.display());
        return ExitCode::from(1);
    }

    println!("Found {} book folder(s):", books.len());
    for b in &books {
        let count = folder_audio_files(b).len();
        println!("  • {}  ({} file(s))", b.display(), count);
    }
    println!();

    if args.dry_run {
        println!("(dry run — nothing written)");
        return ExitCode::SUCCESS;
    }

    let started = Instant::now();
    let mut ok = 0_usize;
    let mut skipped = 0_usize;
    let mut failed: Vec<(PathBuf, String)> = Vec::new();
    for (i, book) in books.iter().enumerate() {
        let title = book
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        println!("[{}/{}] {}", i + 1, books.len(), title);
        match merge_one(book, &args) {
            MergeOutcome::Done(path) => {
                println!("    ✓ {}", path);
                ok += 1;
            }
            MergeOutcome::Skipped(path) => {
                println!("    ↷ skipped (exists): {} — pass --overwrite to replace", path);
                skipped += 1;
            }
            MergeOutcome::Failed(e) => {
                println!("    ✗ {}", e);
                failed.push((book.clone(), e));
            }
        }
    }

    let elapsed = started.elapsed();
    println!();
    println!(
        "Done: {} ok, {} skipped, {} failed, {:.1}s",
        ok,
        skipped,
        failed.len(),
        elapsed.as_secs_f64()
    );

    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
