use crate::types::ResolvedPaths;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[tauri::command]
pub fn resolve_audio_paths(paths: Vec<String>) -> ResolvedPaths {
    let audio_exts = ["mp3", "m4a", "m4b", "aac", "wav", "flac", "wma"];

    let folder_name = if paths.len() == 1 {
        let p = Path::new(&paths[0]);
        if p.is_dir() {
            p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
        } else {
            p.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        }
    } else if paths.len() > 1 {
        let parents: HashSet<_> = paths.iter()
            .filter_map(|p| Path::new(p).parent().and_then(|pp| pp.to_str()))
            .collect();
        if parents.len() == 1 {
            let parent = Path::new(parents.into_iter().next().unwrap());
            parent.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
        } else {
            None
        }
    } else {
        None
    };

    let mut result = Vec::new();
    for path in &paths {
        let p = Path::new(path);
        if p.is_dir() {
            scan_dir_recursive(p, &audio_exts, &mut result);
        } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            if audio_exts.contains(&ext.to_lowercase().as_str()) {
                result.push(path.clone());
            }
        }
    }
    result.sort();

    ResolvedPaths { paths: result, folder_name }
}

fn scan_dir_recursive(dir: &Path, exts: &[&str], result: &mut Vec<String>) {
    let mut visited: HashSet<(u64, u64)> = HashSet::new();
    scan_dir_inner(dir, exts, result, &mut visited);
}

#[cfg(unix)]
fn dir_id(path: &Path) -> Option<(u64, u64)> {
    fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

#[cfg(not(unix))]
fn dir_id(_path: &Path) -> Option<(u64, u64)> { None }

fn scan_dir_inner(dir: &Path, exts: &[&str], result: &mut Vec<String>, visited: &mut HashSet<(u64, u64)>) {
    if let Some(id) = dir_id(dir) {
        if !visited.insert(id) { return; }
    }
    if let Ok(entries) = fs::read_dir(dir) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(meta) = fs::symlink_metadata(&path) {
                    if meta.file_type().is_symlink() {
                        continue;
                    }
                }
                scan_dir_inner(&path, exts, result, visited);
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if exts.contains(&ext.to_lowercase().as_str()) {
                    if let Some(s) = path.to_str() {
                        result.push(s.to_string());
                    }
                }
            }
        }
    }
}
