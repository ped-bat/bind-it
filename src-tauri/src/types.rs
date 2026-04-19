use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AudioFileInfo {
    pub path: String,
    pub filename: String,
    pub chapter_name: String,
    pub codec: String,
    pub duration: f64,
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate: Option<u64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub narrator: Option<String>,
    pub year: Option<String>,
    pub file_size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MergeConfig {
    pub files: Vec<FileEntry>,
    pub output_dir: String,
    pub output_filename: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub narrator: Option<String>,
    pub year: Option<String>,
    pub cover_art_path: Option<String>,
    pub bitrate: u32,
    pub mono: bool,
    #[serde(default)]
    pub force_transcode: bool,
    #[serde(default)]
    pub durations: Option<Vec<f64>>,
    #[serde(default)]
    pub output_codec: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileEntry {
    pub path: String,
    pub chapter_name: String,
}

#[derive(Debug, Clone, Copy)]
pub enum Stage {
    Preparing,
    Transcoding,
    Merging,
    Chapters,
    Done,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Preparing => "preparing",
            Stage::Transcoding => "transcoding",
            Stage::Merging => "merging",
            Stage::Chapters => "chapters",
            Stage::Done => "done",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MergeProgress {
    pub stage: String,
    pub percent: f64,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MergePlan {
    pub strategy: String,
    pub needs_transcode: Vec<String>,
    pub total_duration: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PreflightResult {
    pub ok: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProbeResult {
    pub files: Vec<AudioFileInfo>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoverArtResult {
    pub data_uri: String,
    pub file_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FilePlanInfo {
    pub path: String,
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub duration: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResolvedPaths {
    pub paths: Vec<String>,
    pub folder_name: Option<String>,
}
