use crate::binaries::run_ffmpeg_with_progress;
use crate::types::MergeConfig;
use crate::util::{generate_ffmetadata, path_str, validate_concat_path};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

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
) -> Result<(), String> {
    let metadata_file = tmp_dir.join("ffmetadata.txt");
    let metadata_content = generate_ffmetadata(&config.files, durations, config);
    fs::write(&metadata_file, &metadata_content)
        .map_err(|e| format!("Failed to write metadata: {}", e))?;
    let metadata_str = path_str(&metadata_file)?.to_string();

    let has_cover = config.cover_art_path.as_ref().is_some_and(|p| Path::new(p).exists());

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(), input.into(),
        "-i".into(), metadata_str,
    ];

    if has_cover {
        let cover = config.cover_art_path.as_ref().unwrap();
        args.extend_from_slice(&[
            "-i".into(), cover.clone(),
        ]);
    }

    args.extend_from_slice(&[
        "-map_metadata".into(), "1".into(),
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
        "-f".into(), "mp4".into(),
        output.into(),
    ]);

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_ffmpeg_with_progress(&arg_refs, 0.0, |_| {}, "metadata")
}
