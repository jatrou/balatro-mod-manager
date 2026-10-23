//! Steamodded and Talisman mod framework installer.
//!
//! This module provides functionality to install, uninstall, and manage
//! the Steamodded and Talisman mod frameworks for Balatro. These frameworks
//! are required dependencies for many community mods.
//!
//! # Features
//!
//! - Fetch available versions from GitHub releases
//! - Install specific versions or the latest from the default branch
//! - Detect existing installations
//! - Uninstall mod frameworks
//! - Rate limiting for GitHub API requests
//!
//! # Example
//!
//! ```ignore
//! let installer = ModInstaller::new(ModType::Steamodded);
//! let versions = installer.get_available_versions().await?;
//! installer.install_version(&versions[0]).await?;
//! ```

use anyhow::{Context, Result, anyhow};
use log::info;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tokio::fs as tokio_fs;
use tokio::time::sleep;
use zip::ZipArchive;

use crate::rate_limiter::{
    RateLimitResult, check_github_rate_limit, parse_rate_limit_headers, update_github_rate_limit,
};

#[derive(Debug, Serialize, Deserialize)]
struct Release {
    tag_name: String,
    name: String,
    assets: Vec<ReleaseAsset>,
    published_at: String,
    html_url: String,
    prerelease: bool,
    zipball_url: String, // Add this field
}

fn stable_release_versions(releases: Vec<Release>) -> Vec<String> {
    releases
        .into_iter()
        .filter(|release| !release.prerelease)
        .map(|release| release.tag_name)
        .collect()
}

fn unused_retired_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("framework");
    for sequence in 0.. {
        let suffix = if sequence == 0 {
            String::new()
        } else {
            format!("-{sequence}")
        };
        let candidate = parent.join(format!(".bmm-retired-{name}{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// Remove a retired framework while keeping its root `.lovelyignore` until
/// every loadable child has been deleted. A locked child therefore leaves a
/// disabled backup instead of an active partial copy.
fn remove_disabled_directory(path: &PathBuf) -> std::io::Result<()> {
    let marker = path.join(".lovelyignore");
    if !marker.is_file() {
        return Err(std::io::Error::other(
            "refusing to remove framework backup without .lovelyignore",
        ));
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_name() == ".lovelyignore" {
            continue;
        }
        let entry_path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() && !file_type.is_symlink() {
            fs::remove_dir_all(entry_path)?;
        } else {
            fs::remove_file(entry_path)?;
        }
    }

    fs::remove_file(marker)?;
    fs::remove_dir(path)
}

#[derive(Debug, Clone)]
pub enum ModType {
    Steamodded,
    Talisman,
}

impl std::fmt::Display for ModType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ModType::Steamodded => write!(f, "Steamodded"),
            ModType::Talisman => write!(f, "Talisman"),
        }
    }
}

impl ModType {
    fn get_repo_url(&self) -> &str {
        match self {
            ModType::Steamodded => "Steamodded/smods",
            ModType::Talisman => "MathIsFun0/Talisman",
        }
    }

    pub async fn check_installation(&self) -> bool {
        match self {
            ModType::Steamodded => {
                let installer = ModInstaller::new(ModType::Steamodded);
                installer.is_installed()
            }
            ModType::Talisman => {
                let installer = ModInstaller::new(ModType::Talisman);
                installer.is_installed()
            }
        }
    }

    fn matches_installation_dir(&self, name: &str) -> bool {
        let normalized = name.to_ascii_lowercase();
        match self {
            ModType::Steamodded => {
                normalized == "smods"
                    || normalized == "steamodded"
                    || normalized.starts_with("smods-")
                    || normalized.starts_with("steamodded-smods-")
            }
            ModType::Talisman => normalized == "talisman",
        }
    }

    fn installation_dir_priority(&self, path: &Path) -> (u8, String) {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let rank = match self {
            ModType::Steamodded if name == "steamodded" || name == "smods" => 0,
            ModType::Talisman if name == "talisman" => 0,
            _ => 1,
        };
        (rank, name)
    }

    fn choose_installation_path(
        &self,
        installation_path: &std::path::Path,
        archive_root: &str,
        existing_installations: &[PathBuf],
    ) -> PathBuf {
        existing_installations
            .first()
            .cloned()
            .unwrap_or_else(|| match self {
                ModType::Steamodded => installation_path.join(archive_root),
                ModType::Talisman => installation_path.join("Talisman"),
            })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}
pub struct ModInstaller {
    client: reqwest::Client,
    pub mod_type: ModType,
}

impl ModInstaller {
    pub fn new(mod_type: ModType) -> Self {
        Self {
            client: crate::http::shared_client(),
            mod_type,
        }
    }

    pub fn is_installed(&self) -> bool {
        match self.get_installation_path() {
            Ok(path) => fs::read_dir(path)
                .map(|mut entries| {
                    entries.any(|e| {
                        e.ok()
                            .map(|entry| {
                                let binding = entry.file_name();
                                let name = binding.to_str().unwrap_or("");
                                self.mod_type.matches_installation_dir(name)
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    fn get_installation_path(&self) -> Result<PathBuf> {
        let mod_path = crate::local_mod_detection::resolve_mods_dir_path()
            .map_err(|e| anyhow!(e))
            .context("Failed to resolve mods directory")?;
        fs::create_dir_all(&mod_path).with_context(|| {
            format!("Failed to create mods directory at {}", mod_path.display())
        })?;
        Ok(mod_path)
    }

    pub async fn get_latest_release(&self) -> Result<String> {
        let versions = self.get_available_versions().await?;
        versions
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("{} has no stable GitHub releases", self.mod_type))
    }

    pub async fn get_available_versions(&self) -> Result<Vec<String>> {
        // Check rate limit before making request
        match check_github_rate_limit() {
            RateLimitResult::Proceed => {}
            RateLimitResult::Wait(delay) => {
                log::debug!(
                    "Rate limit backpressure: waiting {:?} before GitHub API call",
                    delay
                );
                sleep(delay).await;
            }
            RateLimitResult::Limited(until) => {
                log::warn!("GitHub API rate limited, reset in {:?}", until);
                return Err(anyhow!(
                    "GitHub API rate limit exceeded. Please wait {} seconds.",
                    until.as_secs()
                ));
            }
        }

        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Balatro-Mod-Manager/1.0"),
        );
        headers.insert(
            "Accept",
            HeaderValue::from_static("application/vnd.github.v3+json"),
        );

        let url = format!(
            "https://api.github.com/repos/{}/releases",
            self.mod_type.get_repo_url()
        );

        log::debug!("Fetching {} releases from: {}", self.mod_type, url);

        let response = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| {
                log::error!("Failed to fetch {} releases: {}", self.mod_type, e);
                e
            })
            .context("Failed to fetch Steamodded releases")?;

        // Update rate limit state from response headers
        let (remaining, reset) = parse_rate_limit_headers(response.headers());
        let is_429 = response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS;
        update_github_rate_limit(remaining, reset, is_429);

        if !response.status().is_success() {
            let status = response.status();
            log::error!(
                "GitHub API error for {}: status={}, rate_limit_remaining={:?}",
                self.mod_type,
                status,
                remaining
            );
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "GitHub API error: {} - {}",
                status,
                error_text
            ));
        }

        log::debug!("Successfully fetched {} releases", self.mod_type);

        let releases: Vec<Release> = response
            .json()
            .await
            .context("Failed to decode Steamodded releases")?;

        // GitHub returns releases newest-first by creation date. Keep that
        // authoritative ordering: lexical sorting makes calendar versions
        // such as 26.829.0 incorrectly outrank the newer 26.1002.0.
        let versions = stable_release_versions(releases);

        Ok(versions)
    }

    async fn get_default_branch_download_url(&self) -> Result<String> {
        use reqwest::header::{ACCEPT, USER_AGENT};

        let api_url = format!(
            "https://api.github.com/repos/{}",
            self.mod_type.get_repo_url()
        );
        log::debug!(
            "Fetching default branch for {}",
            self.mod_type.get_repo_url()
        );

        let response = self
            .client
            .get(&api_url)
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header(USER_AGENT, "Balatro-Mod-Manager/1.0")
            .send()
            .await
            .map_err(|e| {
                log::error!("Failed to fetch repo info for {}: {}", self.mod_type, e);
                e
            })?
            .error_for_status()
            .map_err(|e| {
                log::error!("GitHub API error for {}: {}", self.mod_type, e);
                e
            })?;

        let body = response.json::<serde_json::Value>().await?;
        body["default_branch"]
            .as_str()
            .map(|b| {
                format!(
                    "https://github.com/{}/archive/refs/heads/{b}.zip",
                    self.mod_type.get_repo_url(),
                )
            })
            .ok_or(anyhow::anyhow!(
                "repo {} has no default branch",
                self.mod_type.get_repo_url()
            ))
    }

    pub async fn install_version(&self, version: &str) -> Result<String> {
        let installation_path = self.get_installation_path()?;
        let url = match version {
            "newest" => self.get_default_branch_download_url().await?,
            _ => format!(
                "https://github.com/{}/archive/refs/tags/{}.zip",
                self.mod_type.get_repo_url(),
                version
            ),
        };

        info!(
            "Installing {:?} version {} to {:?}",
            self.mod_type, version, installation_path
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Balatro-Mod-Manager/1.0"),
        );

        // Download the zip file
        log::debug!("Downloading {} from: {}", self.mod_type, url);
        let response = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| {
                log::error!("Failed to download {}: {}", self.mod_type, e);
                e
            })?;

        if !response.status().is_success() {
            log::error!(
                "Download failed for {}: HTTP {}",
                self.mod_type,
                response.status()
            );
            return Err(anyhow!("Download failed: HTTP {}", response.status()));
        }

        let bytes = response.bytes().await?;
        log::debug!("Downloaded {} bytes for {}", bytes.len(), self.mod_type);

        // Use a unique staging directory so concurrent installs cannot delete
        // or overwrite each other's extraction state.
        let temp_dir = tempfile::Builder::new()
            .prefix(".bmm-framework-staging-")
            .tempdir_in(&installation_path)?;

        // Extract to temp directory
        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor)?;
        archive.extract(temp_dir.path())?;

        // Find the root directory name (GitHub format: Steamodded-smods-commitHash)
        let extracted_roots: Vec<PathBuf> = fs::read_dir(temp_dir.path())?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        if extracted_roots.len() != 1 {
            return Err(anyhow!(
                "Expected one root directory in {} archive, found {}",
                self.mod_type,
                extracted_roots.len()
            ));
        }
        let extracted_root = &extracted_roots[0];
        let root_dir = extracted_root
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("Invalid directory name"))?;

        let mut existing_installations: Vec<PathBuf> = fs::read_dir(&installation_path)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .map(|name| self.mod_type.matches_installation_dir(name))
                    .unwrap_or(false)
            })
            .map(|entry| entry.path())
            .collect();
        existing_installations.sort_by_key(|path| self.mod_type.installation_dir_priority(path));

        // Keep an existing folder name when updating so database paths remain
        // stable, including case variants on Windows filesystems.
        let final_dir = self.mod_type.choose_installation_path(
            &installation_path,
            root_dir,
            &existing_installations,
        );

        // Multiple folders may exist from older BMM versions. Their effective
        // state is enabled if any copy is active. Preserve that state on the
        // replacement and disable duplicates before publishing it.
        let preserve_disabled = !existing_installations.is_empty()
            && existing_installations
                .iter()
                .all(|path| crate::installer::has_disabled_marker(path));
        if preserve_disabled {
            crate::installer::apply_disabled_marker(extracted_root)?;
        }

        let duplicate_paths: Vec<PathBuf> = existing_installations
            .iter()
            .filter(|path| **path != final_dir)
            .cloned()
            .collect();
        let mut injected_markers = Vec::new();
        for duplicate in &duplicate_paths {
            let marker = duplicate.join(".lovelyignore");
            if !marker.exists() {
                if let Err(error) = fs::write(&marker, "") {
                    for prior in injected_markers.iter().rev() {
                        let _ = fs::remove_file(prior);
                    }
                    return Err(error).with_context(|| {
                        format!("Failed to disable duplicate at {}", duplicate.display())
                    });
                }
                injected_markers.push(marker);
            }
        }

        if let Err(error) = crate::installer::commit_staged_directory(extracted_root, &final_dir) {
            for marker in injected_markers.iter().rev() {
                let _ = fs::remove_file(marker);
            }
            return Err(error.into());
        }

        // Older versions of BMM installed changing Steamodded archive root
        // names side-by-side. Retire duplicates under an inert name first; if
        // cleanup is blocked, .lovelyignore keeps the leftover from loading.
        for old_path in duplicate_paths {
            if !old_path.exists() {
                continue;
            }
            let retired_path = unused_retired_path(&old_path);
            let cleanup_path = match fs::rename(&old_path, &retired_path) {
                Ok(()) => retired_path,
                Err(error) => {
                    log::warn!(
                        "Updated {}, but could not rename disabled duplicate {}: {}",
                        self.mod_type,
                        old_path.display(),
                        error
                    );
                    old_path
                }
            };
            if let Err(error) = remove_disabled_directory(&cleanup_path) {
                log::warn!(
                    "Updated {}, but could not remove disabled duplicate {}: {}",
                    self.mod_type,
                    cleanup_path.display(),
                    error
                );
            }
        }

        info!(
            "Successfully installed {:?} version {} to {:?}",
            self.mod_type, version, final_dir
        );

        Ok(final_dir.to_string_lossy().to_string())
    }

    pub async fn uninstall(&self) -> Result<()> {
        let installation_path = self.get_installation_path()?;
        let mods_dir = installation_path;

        if !mods_dir.exists() {
            info!("Mods directory not found");
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&mods_dir).await?;
        let mut found = false;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir()
                && let Some(dir_name) = path.file_name().and_then(|n| n.to_str())
                && self.mod_type.matches_installation_dir(dir_name)
            {
                found = true;
                info!("Removing mod directory: {path:?}");
                tokio_fs::remove_dir_all(&path).await?;
            }
        }

        if !found {
            info!("No {} installations found", self.mod_type);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mod_type_display() {
        assert_eq!(format!("{}", ModType::Steamodded), "Steamodded");
        assert_eq!(format!("{}", ModType::Talisman), "Talisman");
    }

    #[test]
    fn test_mod_type_get_repo_url() {
        assert_eq!(ModType::Steamodded.get_repo_url(), "Steamodded/smods");
        assert_eq!(ModType::Talisman.get_repo_url(), "MathIsFun0/Talisman");
    }

    #[test]
    fn test_release_versions_preserve_github_newest_first_order() {
        let release = |tag_name: &str, prerelease: bool| Release {
            tag_name: tag_name.to_string(),
            name: tag_name.to_string(),
            assets: Vec::new(),
            published_at: String::new(),
            html_url: String::new(),
            prerelease,
            zipball_url: String::new(),
        };

        let versions = stable_release_versions(vec![
            release("26.1002.0", false),
            release("26.829.0", false),
            release("27.1.0-beta", true),
        ]);
        assert_eq!(
            versions,
            vec!["26.1002.0".to_string(), "26.829.0".to_string()]
        );
    }

    #[test]
    fn test_installation_directory_matching_handles_current_github_names() {
        assert!(ModType::Steamodded.matches_installation_dir("smods-26.1002.0"));
        assert!(ModType::Steamodded.matches_installation_dir("Steamodded-smods-legacy"));
        assert!(ModType::Steamodded.matches_installation_dir("Steamodded"));
        assert!(ModType::Steamodded.matches_installation_dir("SMODS"));
        assert!(ModType::Steamodded.matches_installation_dir("SMODS-MAIN"));
        assert!(!ModType::Steamodded.matches_installation_dir("Talisman"));
        assert!(ModType::Talisman.matches_installation_dir("talisman"));
        assert!(!ModType::Talisman.matches_installation_dir("smods-main"));
    }

    #[test]
    fn test_manual_installation_paths_are_preferred_deterministically() {
        let mut paths = [
            PathBuf::from("smods-26.1002.0"),
            PathBuf::from("SMODS"),
            PathBuf::from("Steamodded-smods-old"),
        ];
        paths.sort_by_key(|path| ModType::Steamodded.installation_dir_priority(path));

        assert_eq!(paths[0], PathBuf::from("SMODS"));

        let lower_talisman = PathBuf::from("talisman");
        assert_eq!(
            ModType::Talisman
                .installation_dir_priority(&lower_talisman)
                .0,
            0
        );

        let mods_dir = PathBuf::from("Mods");
        let existing_talisman = mods_dir.join("talisman");
        assert_eq!(
            ModType::Talisman.choose_installation_path(
                &mods_dir,
                "Talisman-v2.7",
                std::slice::from_ref(&existing_talisman),
            ),
            existing_talisman
        );
    }

    #[test]
    fn test_retired_cleanup_requires_disabled_marker() {
        let temp = tempfile::tempdir().unwrap();
        let retired = temp.path().join(".bmm-retired-smods-old");
        fs::create_dir_all(&retired).unwrap();
        fs::write(retired.join("loader.lua"), "loader").unwrap();

        assert!(remove_disabled_directory(&retired).is_err());
        assert!(retired.join("loader.lua").exists());

        fs::write(retired.join(".lovelyignore"), "").unwrap();
        remove_disabled_directory(&retired).unwrap();
        assert!(!retired.exists());
    }

    #[test]
    fn test_is_installed_with_no_dir() {
        // ModInstaller::is_installed returns false when the mods directory doesn't exist
        // This tests the fallback behavior
        let installer = ModInstaller::new(ModType::Steamodded);
        // We can't fully test is_installed without mocking the filesystem,
        // but we can verify the struct initializes correctly
        assert!(matches!(installer.mod_type, ModType::Steamodded));
    }

    #[test]
    fn test_mod_installer_new() {
        let steamodded = ModInstaller::new(ModType::Steamodded);
        assert!(matches!(steamodded.mod_type, ModType::Steamodded));

        let talisman = ModInstaller::new(ModType::Talisman);
        assert!(matches!(talisman.mod_type, ModType::Talisman));
    }
}
