use crate::types::{FilePlanInfo, MergePlan};

#[tauri::command]
pub fn get_merge_plan(files: Vec<FilePlanInfo>) -> Result<MergePlan, String> {
    if files.is_empty() {
        return Err("No files provided".to_string());
    }

    let all_aac = files.iter().all(|f| f.codec == "aac");
    let all_mp3 = files.iter().all(|f| f.codec == "mp3");
    let total_duration: f64 = files.iter().map(|f| f.duration).sum();

    if all_aac {
        let sr = files[0].sample_rate;
        let ch = files[0].channels;
        let uniform = files.iter().all(|f| f.sample_rate == sr && f.channels == ch);

        if uniform {
            return Ok(MergePlan {
                strategy: "remux".to_string(),
                needs_transcode: vec![],
                total_duration,
            });
        }

        return Ok(MergePlan {
            strategy: "transcode_aac".to_string(),
            needs_transcode: files.iter().map(|f| f.path.clone()).collect(),
            total_duration,
        });
    }

    if all_mp3 {
        return Ok(MergePlan {
            strategy: "transcode_mp3".to_string(),
            needs_transcode: files.iter().map(|f| f.path.clone()).collect(),
            total_duration,
        });
    }

    let needs_transcode: Vec<String> = files
        .iter()
        .filter(|f| f.codec != "aac")
        .map(|f| f.path.clone())
        .collect();

    Ok(MergePlan {
        strategy: "transcode_mixed".to_string(),
        needs_transcode,
        total_duration,
    })
}
