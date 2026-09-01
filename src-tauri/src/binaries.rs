use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

// ── Binary resolution ────────────────────────────────────────────────────────
// First check for bundled sidecar binaries (next to the app executable),
// then fall back to system PATH, then OS-specific install locations.

fn find_binary(name: &str) -> String {
    let exe_name = if cfg!(windows) {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(&exe_name);
            if candidate.exists() {
                if let Some(s) = candidate.to_str() {
                    return s.to_string();
                }
            }
        }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(&exe_name);
            if candidate.exists() {
                if let Some(s) = candidate.to_str() {
                    return s.to_string();
                }
            }
        }
    }

    let candidates: Vec<String> = if cfg!(target_os = "macos") {
        vec![
            format!("/opt/homebrew/bin/{}", name),
            format!("/usr/local/bin/{}", name),
            format!("/usr/bin/{}", name),
            format!("/opt/local/bin/{}", name),
        ]
    } else if cfg!(target_os = "linux") {
        vec![
            format!("/usr/bin/{}", name),
            format!("/usr/local/bin/{}", name),
            format!("/snap/bin/{}", name),
            format!("/opt/ffmpeg/bin/{}", name),
        ]
    } else if cfg!(windows) {
        let mut v: Vec<String> = Vec::new();
        for env_var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Ok(base) = std::env::var(env_var) {
                v.push(format!("{}\\ffmpeg\\bin\\{}.exe", base, name));
            }
        }
        v.push(format!("C:\\ffmpeg\\bin\\{}.exe", name));
        v
    } else {
        Vec::new()
    };

    for path in &candidates {
        if Path::new(path).exists() {
            return path.clone();
        }
    }
    exe_name
}

pub static FFMPEG_PATH: LazyLock<String> = LazyLock::new(|| find_binary("ffmpeg"));
pub static FFPROBE_PATH: LazyLock<String> = LazyLock::new(|| find_binary("ffprobe"));

// On Windows a GUI-subsystem app still spawns children with visible consoles;
// CREATE_NO_WINDOW suppresses them. Probing/transcoding runs many ffmpeg
// processes in parallel, so without this flag the screen fills with flashing
// console windows.
fn command(path: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn ffmpeg() -> Command {
    command(FFMPEG_PATH.as_str())
}

pub fn ffprobe() -> Command {
    command(FFPROBE_PATH.as_str())
}

/// User-facing hint for when ffmpeg/ffprobe can't be started. The bundled
/// sidecars make this rare, so the likeliest cause is a broken install.
pub fn ffmpeg_install_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "ffmpeg could not be started. Try reinstalling Bind it, or install ffmpeg with: brew install ffmpeg"
    } else if cfg!(windows) {
        "ffmpeg could not be started. Try reinstalling Bind it, or install ffmpeg from ffmpeg.org and add it to PATH."
    } else {
        "ffmpeg could not be started. Try reinstalling Bind it, or install ffmpeg with your package manager (e.g. sudo apt install ffmpeg)."
    }
}

// ── AAC encoder detection ────────────────────────────────────────────────────
// Picks the highest-quality AAC encoder available in the bundled ffmpeg.
// Preference: libfdk_aac > aac_at (macOS AudioToolbox) > aac (native, fallback).

pub static AAC_ENCODER: LazyLock<&'static str> = LazyLock::new(|| {
    let output = ffmpeg()
        .args(["-hide_banner", "-encoders"])
        .output();
    let listing = match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr),
        Err(_) => return "aac",
    };
    let has = |name: &str| {
        listing.lines().any(|l| {
            let trimmed = l.trim_start();
            trimmed.starts_with("A") && trimmed.contains(&format!(" {} ", name))
        })
    };
    if has("libfdk_aac") { "libfdk_aac" }
    else if has("aac_at") { "aac_at" }
    else { "aac" }
});

pub fn aac_encoder() -> &'static str { *AAC_ENCODER }

// ── Global cancellation state ────────────────────────────────────────────────

pub static CANCEL_FLAG: AtomicBool = AtomicBool::new(false);
pub static IS_CONVERTING: AtomicBool = AtomicBool::new(false);

pub struct ConvertGuard;

impl Drop for ConvertGuard {
    fn drop(&mut self) {
        IS_CONVERTING.store(false, Ordering::SeqCst);
    }
}

// ── Generic ffmpeg runner ────────────────────────────────────────────────────
//
// Spawns ffmpeg with the given args, drains stderr to a buffer (so the pipe
// can never deadlock), reads stdout line-by-line and parses ffmpeg's
// `-progress pipe:1` output to call `on_progress` with seconds completed.
// Pass total_duration = 0.0 to skip progress parsing.
//
// Polls CANCEL_FLAG; kills the child on cancellation.

pub fn run_ffmpeg_with_progress<F: Fn(f64)>(
    args: &[&str],
    total_duration: f64,
    on_progress: F,
    op_label: &str,
) -> Result<(), String> {
    let mut child = ffmpeg()
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg {} failed to start: {}", op_label, e))?;

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let progress_seconds = std::sync::Arc::new(std::sync::Mutex::new(0.0_f64));
    let stdout_thread = stdout_pipe.map(|s| {
        let progress = std::sync::Arc::clone(&progress_seconds);
        let parse = total_duration > 0.0;
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(s);
            for line in reader.lines().map_while(Result::ok) {
                if !parse { continue; }
                if let Some(rest) = line.strip_prefix("out_time_ms=") {
                    if let Ok(us) = rest.trim().parse::<i64>() {
                        let secs = (us.max(0) as f64) / 1_000_000.0;
                        if let Ok(mut g) = progress.lock() { *g = secs; }
                    }
                } else if let Some(rest) = line.strip_prefix("out_time_us=") {
                    if let Ok(us) = rest.trim().parse::<i64>() {
                        let secs = (us.max(0) as f64) / 1_000_000.0;
                        if let Ok(mut g) = progress.lock() { *g = secs; }
                    }
                }
            }
        })
    });

    let stderr_buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let stderr_thread = stderr_pipe.map(|mut s| {
        let buf = std::sync::Arc::clone(&stderr_buf);
        std::thread::spawn(move || {
            let mut sink = String::new();
            std::io::Read::read_to_string(&mut s, &mut sink).ok();
            if let Ok(mut g) = buf.lock() { *g = sink; }
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
                if total_duration > 0.0 && last_emit.elapsed().as_millis() > 250 {
                    last_emit = std::time::Instant::now();
                    let secs = progress_seconds.lock().map(|g| *g).unwrap_or(0.0);
                    on_progress(secs);
                }
                std::thread::sleep(std::time::Duration::from_millis(80));
            }
            Err(e) => return Err(format!("ffmpeg {} wait error: {}", op_label, e)),
        }
    };

    if let Some(t) = stdout_thread { let _ = t.join(); }
    if let Some(t) = stderr_thread { let _ = t.join(); }

    if !status.success() {
        let stderr = stderr_buf.lock().map(|g| g.clone()).unwrap_or_default();
        return Err(format!("ffmpeg {} failed: {}", op_label, stderr));
    }
    if total_duration > 0.0 {
        on_progress(total_duration);
    }
    Ok(())
}

#[tauri::command]
pub fn check_ffmpeg() -> Result<String, String> {
    let output = ffprobe()
        .arg("-version")
        .output()
        .map_err(|e| format!("ffprobe not found: {}", e))?;

    let version = String::from_utf8_lossy(&output.stdout);
    let first_line = version.lines().next().unwrap_or("unknown");
    Ok(first_line.to_string())
}
