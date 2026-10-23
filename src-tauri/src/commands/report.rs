use std::fs;

const ISSUE_URL: &str = "https://github.com/jatrou/balatro-mod-manager/issues/new";

/// Build a GitHub issue draft for the maintained fork.
///
/// This command intentionally performs no network request and does not collect
/// logs or hardware details. The frontend opens the returned URL so the user
/// can review and submit the issue in their browser.
#[tauri::command]
pub async fn submit_report(
    title: String,
    description: String,
    mm_version: String,
) -> Result<String, String> {
    Ok(build_issue_url(&title, &description, &mm_version))
}

fn build_issue_url(title: &str, description: &str, mm_version: &str) -> String {
    let body = format!(
        "{}\n\n---\nBalatro Mod Manager version: {}\n\nLogs and system information are not attached automatically. Add only the diagnostics you choose to share.",
        description.trim(),
        mm_version.trim()
    );

    format!(
        "{ISSUE_URL}?title={}&body={}",
        urlencoding::encode(title.trim()),
        urlencoding::encode(&body)
    )
}

/// Returns the latest log's filename and text, if any.
#[tauri::command]
pub async fn get_latest_log() -> Result<(String, String), String> {
    latest_log().ok_or_else(|| "No logs found.".to_string())
}

#[tauri::command]
pub async fn get_logs_folder() -> Result<String, String> {
    let dir = bmm_lib::config_dir()
        .ok_or_else(|| "config directory not found".to_string())?
        .join("Balatro")
        .join("logs");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().to_string())
}

fn latest_log() -> Option<(String, String)> {
    let dir = bmm_lib::config_dir()?.join("Balatro").join("logs");
    let entries = fs::read_dir(&dir).ok()?;
    let mut files: Vec<_> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            path.extension().is_some_and(|extension| extension == "log")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("bmm_"))
        })
        .collect();

    files.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    let path = files.last()?.path();
    let filename = path.file_name()?.to_string_lossy().to_string();
    let text = fs::read_to_string(&path).ok()?;
    Some((filename, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_draft_targets_maintained_fork_and_encodes_user_text() {
        let url = build_issue_url(
            "Lovely update & crash",
            "Steps:\n1. Update Lovely\n2. Launch",
            "0.5.0-beta+1",
        );

        assert!(url.starts_with("https://github.com/jatrou/balatro-mod-manager/issues/new?"));
        assert!(url.contains("title=Lovely%20update%20%26%20crash"));
        assert!(url.contains("Steps%3A%0A1.%20Update%20Lovely%0A2.%20Launch"));
        assert!(url.contains("0.5.0-beta%2B1"));
    }

    #[test]
    fn issue_draft_explains_that_diagnostics_are_opt_in() {
        let url = build_issue_url("Bug", "Details", "1.2.3");
        let encoded_body = url.split("&body=").nth(1).unwrap();
        let body = urlencoding::decode(encoded_body).unwrap();

        assert!(body.contains("Logs and system information are not attached automatically"));
        assert!(!body.contains("CPU:"));
        assert!(!body.contains("GPU:"));
    }
}
