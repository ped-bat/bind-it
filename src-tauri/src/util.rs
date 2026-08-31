use crate::types::{FileEntry, MergeConfig};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub fn path_str(p: &Path) -> Result<&str, String> {
    p.to_str().ok_or_else(|| format!("Path contains invalid UTF-8: {:?}", p))
}

pub fn validate_concat_path(p: &str) -> Result<(), String> {
    if p.contains('\n') || p.contains('\r') {
        return Err(format!(
            "Refusing to merge file with newline in path: {:?}", p
        ));
    }
    Ok(())
}

pub fn validate_filename(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Output filename is empty.".to_string());
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err("Output filename contains path separators.".to_string());
    }
    if name == "." || name == ".." {
        return Err("Invalid output filename.".to_string());
    }
    Ok(())
}

/// Truncate a path to its filename, capped at 40 chars with ellipsis.
pub fn short_filename(path: &str) -> String {
    let name = Path::new(path).file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);
    const MAX: usize = 40;
    if name.chars().count() <= MAX { return name.to_string(); }
    let mut out: String = name.chars().take(MAX - 1).collect();
    out.push('…');
    out
}

/// Returns true if `path` lives inside the system temp directory.
pub fn is_temp_path(path: &str) -> bool {
    let Ok(tmp) = std::fs::canonicalize(std::env::temp_dir()) else {
        return false;
    };
    let Ok(canonical) = std::fs::canonicalize(path) else {
        return false;
    };
    canonical.starts_with(&tmp)
}

pub fn unique_output_path(dir: &Path, filename: &str, ext: &str) -> PathBuf {
    let lower = filename.to_lowercase();
    let name = if lower.ends_with(".m4b") || lower.ends_with(".mp3") {
        &filename[..filename.len() - 4]
    } else {
        filename
    };
    let candidate = dir.join(format!("{}.{}", name, ext));
    if !candidate.exists() {
        return candidate;
    }
    for i in 1..=999 {
        let candidate = dir.join(format!("{} ({}).{}", name, i, ext));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{} (999).{}", name, ext))
}

pub fn categorize_error(err: &str) -> String {
    if err.contains("Permission denied") || err.contains("EACCES") {
        "Permission denied — choose a different output folder or check folder permissions.".to_string()
    } else if err.contains("No space left") || err.contains("Disk full") || err.contains("ENOSPC") {
        "Not enough disk space. Free up space or choose a different drive.".to_string()
    } else if err.contains("No such file or directory") {
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
    } else if err.contains("No audio stream")
        || err.contains("Invalid data found when processing input")
        || err.contains("could not find codec")
    {
        if let Some(name) = extract_source_filename(err) {
            return format!("Could not read {}. The file may be corrupted.", name);
        }
        "Could not read a source file. The file may be corrupted.".to_string()
    } else {
        let tail = extract_stderr_tail(err);
        if tail.is_empty() {
            format!("Conversion failed: {}", err)
        } else {
            format!("Conversion failed — {}", tail)
        }
    }
}

/// Pulls the source path out of errors shaped like
/// `"Transcode failed for /abs/path.mp3: <stderr>"`.
fn extract_source_filename(err: &str) -> Option<String> {
    let marker = " failed for ";
    let start = err.find(marker)?;
    let rest = &err[start + marker.len()..];
    let path_end = rest.find(": ").unwrap_or(rest.len());
    let path = &rest[..path_end];
    Path::new(path).file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

/// Returns the last non-empty line of an ffmpeg stderr blob, trimmed.
/// Useful for surfacing the actual failure reason without the full log.
fn extract_stderr_tail(err: &str) -> String {
    err.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

pub fn clean_chapter_name(filename: &str) -> String {
    // Pattern A: "Chapter NN - …", "Part NN - …", "Track NN - …", "Section NN - …".
    // Always strip — the leading word is filler.
    static WORD_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?:Chapter|Part|Track|Section)\s*\d+\s*[-–—.]\s*").expect("WORD_PREFIX regex")
    });
    // Pattern B: two numeric prefixes back-to-back, e.g. "01 - 02 - Title" or
    // "01 - 01 - Title". The FIRST one is the unique track/index across files;
    // the second is often a static disc number repeated across the whole set.
    // Keep the first, drop the second — otherwise sets like
    // "NN - 01 - Author - Album" collapse to identical chapter titles.
    static DOUBLE_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(\d{1,3}\s*[-–—.]\s*)\d{1,3}\s*[-–—.]\s*").expect("DOUBLE_PREFIX regex")
    });
    // Pattern C: a single numeric prefix, e.g. "01 - Title". Strip it.
    static SINGLE_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^\d{1,3}\s*[-–—.]\s*").expect("SINGLE_PREFIX regex")
    });

    let name = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);

    let cleaned = if WORD_PREFIX.is_match(name) {
        WORD_PREFIX.replace(name, "").to_string()
    } else if DOUBLE_PREFIX.is_match(name) {
        DOUBLE_PREFIX.replace(name, "$1").to_string()
    } else {
        SINGLE_PREFIX.replace(name, "").to_string()
    };

    let result = cleaned.trim().to_string();
    if result.is_empty() { name.trim().to_string() } else { result }
}

fn escape_ffmetadata(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '=' | ';' | '#' | '\\' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            '\n' => {
                escaped.push('\\');
                escaped.push('n');
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

pub fn generate_ffmetadata(files: &[FileEntry], durations: &[f64], metadata: &MergeConfig) -> String {
    let mut meta = String::from(";FFMETADATA1\n");

    if let Some(ref t) = metadata.title {
        meta.push_str(&format!("title={}\n", escape_ffmetadata(t)));
    }
    if let Some(ref a) = metadata.artist {
        meta.push_str(&format!("artist={}\n", escape_ffmetadata(a)));
    }
    if let Some(ref al) = metadata.album {
        meta.push_str(&format!("album={}\n", escape_ffmetadata(al)));
    }
    if let Some(ref n) = metadata.narrator {
        meta.push_str(&format!("composer={}\n", escape_ffmetadata(n)));
    }
    if let Some(ref y) = metadata.year {
        meta.push_str(&format!("date={}\n", escape_ffmetadata(y)));
    }
    meta.push_str("genre=Audiobook\n");
    meta.push('\n');

    let mut cumulative_ms: u64 = 0;
    for (i, file) in files.iter().enumerate() {
        let duration_ms = (durations[i] * 1000.0) as u64;
        meta.push_str("[CHAPTER]\n");
        meta.push_str("TIMEBASE=1/1000\n");
        meta.push_str(&format!("START={}\n", cumulative_ms));
        cumulative_ms += duration_ms;
        meta.push_str(&format!("END={}\n", cumulative_ms));
        meta.push_str(&format!("title={}\n", escape_ffmetadata(&file.chapter_name)));
        meta.push('\n');
    }

    meta
}
