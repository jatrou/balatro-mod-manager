//! Lovely (mod loader/injector) installer and version management.
//!
//! Lovely is the mod injection framework that enables Balatro mods to work.
//! This module handles:
//! - Downloading and installing the Lovely injector
//! - Version checking and updates
//! - Platform-specific installation (Windows DLL, macOS dylib, Linux .so)
//!
//! # Platform-Specific Details
//!
//! - **Windows/Linux (Proton)**: Uses `winmm.dll` for DLL injection
//! - **macOS**: Uses `liblovely.dylib` with `DYLD_INSERT_LIBRARIES`
//! - **Linux (native LOVE)**: Uses `liblovely.so` with `LD_PRELOAD`

use crate::errors::AppError;
use crate::http::{shared_client, shared_no_redirect_client};
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::fs::{self};
#[cfg(target_os = "macos")]
use std::fs::{self, File};
use std::io::{Read, Seek, Write};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Cached Lovely version with timestamp
struct CachedVersion {
    version: String,
    fetched_at: Instant,
}

/// Disk-persisted Lovely version cache
#[derive(serde::Serialize, serde::Deserialize)]
struct PersistedVersionCache {
    version: String,
    /// Unix timestamp in seconds when fetched
    fetched_at_unix: u64,
}

/// Global cache for Lovely version (24 hour TTL)
static VERSION_CACHE: OnceLock<Mutex<Option<CachedVersion>>> = OnceLock::new();
const VERSION_CACHE_TTL_SECS: u64 = 60 * 60 * 24; // 24 hours

/// Lovely 0.10.0 changed its Windows proxy library from `version.dll` to
/// `winmm.dll`. Keep the legacy name here only for detection and migration.
pub const WINDOWS_INJECTOR_FILENAME: &str = "winmm.dll";
pub const LEGACY_WINDOWS_INJECTOR_FILENAME: &str = "version.dll";

/// Get the path to the Lovely version cache file
fn get_version_cache_path() -> Option<PathBuf> {
    crate::config_dir().map(|d| d.join("Balatro").join("lovely_version_cache.json"))
}

/// Load Lovely version from disk cache if still fresh
fn load_persisted_version_cache() -> Option<String> {
    let path = get_version_cache_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    let cached: PersistedVersionCache = serde_json::from_str(&content).ok()?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();

    if now.saturating_sub(cached.fetched_at_unix) < VERSION_CACHE_TTL_SECS {
        log::debug!("Loaded Lovely version from disk cache: {}", cached.version);
        Some(cached.version)
    } else {
        log::debug!("Lovely version disk cache expired");
        None
    }
}

/// Save Lovely version to disk cache
fn save_persisted_version_cache(version: &str) {
    if let Some(path) = get_version_cache_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let cached = PersistedVersionCache {
            version: version.to_string(),
            fetched_at_unix: now,
        };
        if let Ok(json) = serde_json::to_string(&cached) {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("Failed to save Lovely version cache: {}", e);
            } else {
                log::debug!("Saved Lovely version to disk cache: {}", version);
            }
        }
    }
}

/// Returns true when either the current Windows injector or a pre-0.10 legacy
/// injector is present (including BMM's disabled form).
pub fn windows_injector_artifact_exists(game_path: &Path) -> bool {
    [WINDOWS_INJECTOR_FILENAME, LEGACY_WINDOWS_INJECTOR_FILENAME]
        .iter()
        .map(|name| game_path.join(name))
        .any(|path| injector_artifact_exists(&path))
}

fn windows_injector_is_disabled(game_path: &Path) -> bool {
    let active_exists = [WINDOWS_INJECTOR_FILENAME, LEGACY_WINDOWS_INJECTOR_FILENAME]
        .iter()
        .any(|name| game_path.join(name).exists());
    let disabled_exists = [WINDOWS_INJECTOR_FILENAME, LEGACY_WINDOWS_INJECTOR_FILENAME]
        .iter()
        .map(|name| game_path.join(name))
        .filter_map(|path| disabled_injector_path(&path))
        .any(|path| path.exists());
    !active_exists && disabled_exists
}

/// Preserve the legacy rename-based disabled state while replacing an injector.
/// An active file wins if both forms exist, matching `toggle_injector_file`.
#[cfg(any(test, target_os = "linux", target_os = "macos"))]
fn injector_installation_target(active_path: &Path) -> PathBuf {
    let disabled_path =
        disabled_injector_path(active_path).expect("an injector path always has a file name");
    if !active_path.exists() && disabled_path.exists() {
        disabled_path
    } else {
        active_path.to_path_buf()
    }
}

fn move_existing_to_rollback(path: &Path) -> Result<Option<PathBuf>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    let rollback = unused_sibling_path(path, ".bmm-migration-rollback");
    std::fs::rename(path, &rollback).map_err(|e| AppError::FileWrite {
        path: path.to_path_buf(),
        source: format!("Failed to preserve injector for migration: {e}"),
    })?;
    Ok(Some(rollback))
}

fn restore_rollback(path: &Path, rollback: Option<&Path>) {
    let _ = std::fs::remove_file(path);
    if let Some(rollback) = rollback {
        let _ = std::fs::rename(rollback, path);
    }
}

fn remove_rollback(rollback: Option<&Path>) {
    if let Some(rollback) = rollback
        && let Err(error) = std::fs::remove_file(rollback)
    {
        log::warn!(
            "Lovely migration succeeded but rollback cleanup failed at {}: {}",
            rollback.display(),
            error
        );
    }
}

/// Ensures the current Lovely Windows injector exists in the game directory.
///
/// Downloads the DLL from GitHub releases if not present.
/// On Linux, caches the DLL in ~/.config/Balatro/bins/ and copies to game directory
/// each launch, since Steam may verify/restore game files.
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub async fn ensure_windows_injector_exists(game_path: &Path) -> Result<PathBuf, AppError> {
    install_windows_injector(game_path, false, None).await
}

/// Downloads and transactionally replaces the current Lovely Windows injector.
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub async fn update_windows_injector(game_path: &Path, version: &str) -> Result<PathBuf, AppError> {
    install_windows_injector(game_path, true, Some(version)).await
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
async fn install_windows_injector(
    game_path: &Path,
    force_download: bool,
    release_version: Option<&str>,
) -> Result<PathBuf, AppError> {
    #[cfg(target_os = "linux")]
    {
        // On Linux, cache the DLL and copy on each launch to survive Steam file verification
        let config_dir = crate::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
        let bins_dir = config_dir.join("Balatro/bins");
        fs::create_dir_all(&bins_dir).map_err(|e| AppError::DirCreate {
            path: bins_dir.clone(),
            source: e.to_string(),
        })?;

        let game_has_injector = windows_injector_artifact_exists(game_path);
        let disabled = if game_has_injector {
            windows_injector_is_disabled(game_path)
        } else {
            windows_injector_is_disabled(&bins_dir)
        };
        let active_dll = game_path.join(WINDOWS_INJECTOR_FILENAME);
        let dll_path = if disabled {
            disabled_injector_path(&active_dll).expect("a DLL path always has a file name")
        } else {
            active_dll
        };
        let cached_dll = bins_dir.join(WINDOWS_INJECTOR_FILENAME);
        if force_download || !cached_dll.exists() {
            if !force_download && dll_path.exists() {
                fs::copy(&dll_path, &cached_dll).map_err(|e| AppError::FileCopy {
                    source: dll_path.display().to_string(),
                    dest: cached_dll.display().to_string(),
                    source_error: e.to_string(),
                })?;
            } else {
                let version = match release_version {
                    Some(version) => version.to_string(),
                    None => get_latest_lovely_version_fresh().await?,
                };
                let injector = fetch_windows_injector(&version).await?;
                replace_file_transactionally(&cached_dll, injector.as_slice())?;
            }
        }

        // Always copy to the game directory (it may have been removed by Steam),
        // but stage the copy so an interrupted write cannot corrupt a working DLL.
        let source = File::open(&cached_dll).map_err(|e| AppError::FileRead {
            path: cached_dll.clone(),
            source: e.to_string(),
        })?;
        let rollback = move_existing_to_rollback(&dll_path)?;
        if let Err(error) = replace_file_transactionally(&dll_path, source) {
            restore_rollback(&dll_path, rollback.as_deref());
            return Err(error);
        }
        log::debug!(
            "Copied {} from cache {} to {}",
            WINDOWS_INJECTOR_FILENAME,
            cached_dll.display(),
            dll_path.display()
        );

        if let Err(error) = retire_legacy_windows_injectors(game_path) {
            restore_rollback(&dll_path, rollback.as_deref());
            return Err(error);
        }
        remove_rollback(rollback.as_deref());
        retire_legacy_windows_injectors(&bins_dir)?;

        Ok(dll_path)
    }

    #[cfg(target_os = "windows")]
    {
        let disabled = windows_injector_is_disabled(game_path);
        let active_dll = game_path.join(WINDOWS_INJECTOR_FILENAME);
        let dll_path = if disabled {
            disabled_injector_path(&active_dll).expect("a DLL path always has a file name")
        } else {
            active_dll
        };

        // On Windows, download directly to the active or disabled destination,
        // preserving the user's current injector state.
        if force_download || !dll_path.exists() {
            let version = match release_version {
                Some(version) => version.to_string(),
                None => get_latest_lovely_version_fresh().await?,
            };
            let injector = fetch_windows_injector(&version).await?;
            let rollback = move_existing_to_rollback(&dll_path)?;
            if let Err(error) = replace_file_transactionally(&dll_path, injector.as_slice()) {
                restore_rollback(&dll_path, rollback.as_deref());
                return Err(error);
            }
            if let Err(error) = retire_legacy_windows_injectors(game_path) {
                restore_rollback(&dll_path, rollback.as_deref());
                return Err(error);
            }
            remove_rollback(rollback.as_deref());
        } else {
            retire_legacy_windows_injectors(game_path)?;
        }
        Ok(dll_path)
    }
}

#[cfg(target_os = "macos")]
fn detect_architecture() -> Result<&'static str, AppError> {
    use libc::{c_void, size_t, sysctl};

    let mut size: size_t = 0;
    let mut mib = [libc::CTL_HW, libc::HW_MACHINE];

    // First call to get buffer size
    unsafe {
        if sysctl(
            mib.as_mut_ptr(),
            2,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return Err(AppError::SystemDetection(
                "Failed to get architecture buffer size".into(),
            ));
        }
    }

    let mut buf = vec![0u8; size];

    // Second call to get actual value
    unsafe {
        if sysctl(
            mib.as_mut_ptr(),
            2,
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return Err(AppError::SystemDetection(
                "Failed to retrieve architecture".into(),
            ));
        }
    }

    // Convert buffer to string and match architecture
    match String::from_utf8_lossy(&buf).trim_end_matches('\0') {
        "arm64" => Ok("aarch64"),
        "x86_64" => Ok("x86_64"),
        other => Err(AppError::UnsupportedArchitecture(other.into())),
    }
}

pub async fn ensure_lovely_exists() -> Result<PathBuf, AppError> {
    #[cfg(target_os = "macos")]
    {
        let config_dir = crate::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;

        let bins_dir = config_dir.join("Balatro/bins");
        fs::create_dir_all(&bins_dir).map_err(|e| AppError::DirCreate {
            path: bins_dir.clone(),
            source: e.to_string(),
        })?;

        let lovely_path = bins_dir.join("liblovely.dylib");
        let installation_target = injector_installation_target(&lovely_path);
        if !installation_target.exists() {
            let version = get_latest_lovely_version_fresh().await?;
            download_and_install_lovely(&installation_target, &version).await?;
        }

        Ok(lovely_path)
    }

    #[cfg(target_os = "windows")]
    {
        let balatro_paths = crate::finder::get_balatro_paths_cached();
        if balatro_paths.is_empty() {
            return Err(AppError::DirNotFound(PathBuf::from("Balatro installation")));
        }

        // Ensure the current Windows proxy DLL exists in the game directory.
        let game_path = &balatro_paths[0];
        ensure_windows_injector_exists(game_path).await?;

        Ok(game_path.join("Balatro.exe"))
    }

    #[cfg(target_os = "linux")]
    {
        let balatro_paths = crate::finder::get_balatro_paths_cached();
        if balatro_paths.is_empty() {
            return Err(AppError::DirNotFound(PathBuf::from("Balatro installation")));
        }

        // Ensure the current Windows proxy DLL exists in the game directory (Proton/Wine).
        let game_path = &balatro_paths[0];
        ensure_windows_injector_exists(game_path).await?;

        Ok(game_path.join("Balatro.exe"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(AppError::InvalidState(
            "Lovely injection is not supported on this platform.".into(),
        ))
    }
}

/// Download the latest injector and replace the installed copy transactionally.
/// On Windows and Proton/Wine, `game_path` should be the configured Balatro
/// directory so the update cannot accidentally target a different Steam library.
pub async fn update_lovely_at(
    game_path: Option<&Path>,
    version: &str,
) -> Result<PathBuf, AppError> {
    #[cfg(target_os = "macos")]
    {
        let _ = game_path;
        let config_dir = crate::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
        let active_target = config_dir.join("Balatro/bins/liblovely.dylib");
        let target = injector_installation_target(&active_target);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::DirCreate {
                path: parent.to_path_buf(),
                source: e.to_string(),
            })?;
        }
        download_and_install_lovely(&target, version).await?;
        Ok(target)
    }

    #[cfg(target_os = "windows")]
    {
        let discovered;
        let game_path = match game_path {
            Some(path) => path,
            None => {
                discovered = crate::finder::get_balatro_paths_cached();
                discovered
                    .first()
                    .map(PathBuf::as_path)
                    .ok_or_else(|| AppError::DirNotFound(PathBuf::from("Balatro installation")))?
            }
        };
        update_windows_injector(game_path, version).await
    }

    #[cfg(target_os = "linux")]
    {
        let discovered;
        let game_path = match game_path {
            Some(path) => path,
            None => {
                discovered = crate::finder::get_balatro_paths_cached();
                discovered
                    .first()
                    .map(PathBuf::as_path)
                    .ok_or_else(|| AppError::DirNotFound(PathBuf::from("Balatro installation")))?
            }
        };

        // Linux supports both Proton/Wine and native LOVE. Keep both artifacts
        // on the same exact release before callers commit the shared version.
        update_windows_injector(game_path, version).await?;
        update_lovely_so(game_path, version).await
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = game_path;
        Err(AppError::InvalidState(
            "Lovely injection is not supported on this platform.".into(),
        ))
    }
}

#[cfg(target_os = "linux")]
pub async fn ensure_love_binary() -> Result<(PathBuf, Option<PathBuf>), AppError> {
    let config_dir = crate::config_dir()
        .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
    let bins_dir = config_dir.join("Balatro/bins");
    fs::create_dir_all(&bins_dir).map_err(|e| AppError::DirCreate {
        path: bins_dir.clone(),
        source: e.to_string(),
    })?;
    let target_dir = bins_dir.join("love");
    let target_bin = target_dir.join("love");
    if target_bin.exists() {
        // Refresh permissions in case they were lost.
        let perms = std::fs::Permissions::from_mode(0o755);
        let _ = std::fs::set_permissions(&target_bin, perms);
        let lib_dir = target_bin
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("lib"))
            .filter(|p| p.is_dir());
        return Ok((target_bin, lib_dir));
    }

    download_love_appimage_and_extract(&target_dir).await?;
    // Prefer extracted AppImage binary: love/bin/love
    let bin = target_dir.join("bin/love");
    if !bin.exists() {
        return Err(AppError::InvalidState(
            "LOVE AppImage extraction did not produce bin/love".to_string(),
        ));
    }

    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&bin, perms).map_err(|e| AppError::FileWrite {
        path: bin.clone(),
        source: e.to_string(),
    })?;
    let lib_dir = target_dir.join("lib");
    Ok((
        bin,
        if lib_dir.is_dir() {
            Some(lib_dir)
        } else {
            None
        },
    ))
}

#[cfg(target_os = "linux")]
pub async fn ensure_lovely_so_exists(game_path: &Path) -> Result<PathBuf, AppError> {
    install_lovely_so(game_path, false, None).await
}

#[cfg(target_os = "linux")]
pub async fn update_lovely_so(game_path: &Path, version: &str) -> Result<PathBuf, AppError> {
    install_lovely_so(game_path, true, Some(version)).await
}

#[cfg(target_os = "linux")]
async fn install_lovely_so(
    game_path: &Path,
    force_download: bool,
    release_version: Option<&str>,
) -> Result<PathBuf, AppError> {
    let config_dir = crate::config_dir()
        .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
    let bins_dir = config_dir.join("Balatro/bins");
    fs::create_dir_all(&bins_dir).map_err(|e| AppError::DirCreate {
        path: bins_dir.clone(),
        source: e.to_string(),
    })?;

    let cached_active = bins_dir.join("liblovely.so");
    let cached_so = injector_installation_target(&cached_active);
    if force_download || !cached_so.exists() {
        let version = match release_version {
            Some(version) => version.to_string(),
            None => get_latest_lovely_version_fresh().await?,
        };
        download_lovely_linux(&cached_so, &version).await?;
    }

    let target_active = game_path.join("liblovely.so");
    let target_so = if injector_artifact_exists(&target_active) {
        injector_installation_target(&target_active)
    } else if cached_so != cached_active {
        disabled_injector_path(&target_active).expect("an injector path always has a file name")
    } else {
        target_active
    };
    let source = File::open(&cached_so).map_err(|e| AppError::FileRead {
        path: cached_so.clone(),
        source: e.to_string(),
    })?;
    replace_file_transactionally_with_permissions(
        &target_so,
        source,
        Some(std::fs::Permissions::from_mode(0o755)),
    )?;

    Ok(target_so)
}

#[cfg(target_os = "linux")]
async fn download_love_appimage_and_extract(target_dir: &Path) -> Result<(), AppError> {
    // Pin to LOVE 11.5 linux x86_64 tarball (latest as of Balatro/Lovely expectations).
    let appimage_url =
        "https://github.com/love2d/love/releases/download/11.5/love-11.5-x86_64.AppImage";

    log::info!("Downloading LOVE AppImage from: {}", appimage_url);

    let client = shared_client();

    let response = client.get(appimage_url).send().await.map_err(|e| {
        log::error!("Failed to connect to LOVE AppImage download: {}", e);
        AppError::Network(format!("Failed to download LOVE AppImage: {e}"))
    })?;

    if !response.status().is_success() {
        log::error!(
            "LOVE AppImage download failed with HTTP {}",
            response.status()
        );
        return Err(AppError::Network(format!(
            "LOVE AppImage download failed: HTTP {}",
            response.status()
        )));
    }

    let bytes = response.bytes().await.map_err(|e| {
        log::error!("Failed to read LOVE AppImage response body: {}", e);
        AppError::Network(format!("Failed to read LOVE AppImage bytes: {e}"))
    })?;

    log::debug!("Downloaded LOVE AppImage: {} bytes", bytes.len());

    let temp_dir = tempfile::tempdir().map_err(|e| AppError::FileWrite {
        path: PathBuf::from("temp directory"),
        source: e.to_string(),
    })?;
    let temp_appimage = temp_dir.path().join("love.AppImage");
    fs::write(&temp_appimage, &bytes).map_err(|e| AppError::FileWrite {
        path: temp_appimage.clone(),
        source: e.to_string(),
    })?;
    // Ensure the AppImage is executable before extraction.
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&temp_appimage, perms).map_err(|e| AppError::FileWrite {
        path: temp_appimage.clone(),
        source: e.to_string(),
    })?;

    if target_dir.exists() {
        fs::remove_dir_all(target_dir).map_err(|e| AppError::DirCreate {
            path: target_dir.to_path_buf(),
            source: e.to_string(),
        })?;
    }
    fs::create_dir_all(target_dir).map_err(|e| AppError::DirCreate {
        path: target_dir.to_path_buf(),
        source: e.to_string(),
    })?;

    // Extract the AppImage contents into target_dir/squashfs-root then move its contents up.
    let mut extract = Command::new(&temp_appimage);
    extract
        .arg("--appimage-extract")
        .env("APPIMAGE_EXTRACT_AND_RUN", "1")
        .current_dir(target_dir);
    let status = extract
        .status()
        .map_err(|e| AppError::InvalidState(format!("Failed to extract LOVE AppImage: {e}")))?;
    if !status.success() {
        return Err(AppError::InvalidState(format!(
            "LOVE AppImage extraction failed with status {status}"
        )));
    }

    let squash_root = target_dir.join("squashfs-root");
    if !squash_root.exists() {
        return Err(AppError::InvalidState(
            "LOVE AppImage extraction did not produce squashfs-root".to_string(),
        ));
    }

    for ent in (fs::read_dir(&squash_root).map_err(|e| AppError::DirCreate {
        path: squash_root.clone(),
        source: e.to_string(),
    })?)
    .flatten()
    {
        let src = ent.path();
        let dst = target_dir.join(ent.file_name());
        let _ = fs::rename(&src, &dst);
    }

    let _ = fs::remove_dir_all(&squash_root);

    Ok(())
}

/// Query GitHub for the latest Lovely release tag (e.g., "0.8.0").
/// Results are cached for 24 hours to avoid blocking game launches.
/// Cache persists to disk to survive app restarts.
pub async fn get_latest_lovely_version() -> Result<String, AppError> {
    resolve_latest_lovely_version(true).await
}

/// Resolve GitHub's current release without consulting the 24-hour cache.
/// Explicit updates use this so the recorded version and downloaded asset are
/// guaranteed to refer to the same release.
pub async fn get_latest_lovely_version_fresh() -> Result<String, AppError> {
    resolve_latest_lovely_version(false).await
}

async fn resolve_latest_lovely_version(use_cache: bool) -> Result<String, AppError> {
    let cache = VERSION_CACHE.get_or_init(|| Mutex::new(None));
    if use_cache {
        let guard = cache.lock().await;
        if let Some(cached) = guard.as_ref()
            && cached.fetched_at.elapsed() < Duration::from_secs(VERSION_CACHE_TTL_SECS)
        {
            log::debug!("Using cached Lovely version: {}", cached.version);
            return Ok(cached.version.clone());
        }
    }

    if use_cache && let Some(version) = load_persisted_version_cache() {
        // Populate in-memory cache from disk
        let mut guard = cache.lock().await;
        *guard = Some(CachedVersion {
            version: version.clone(),
            fetched_at: Instant::now(),
        });
        return Ok(version);
    }

    // Cache miss or expired - fetch from GitHub
    log::debug!("Querying GitHub for latest Lovely release version");

    let client = shared_no_redirect_client();

    let resp = client
        .get("https://github.com/ethangreen-dev/lovely-injector/releases/latest")
        .send()
        .await
        .map_err(|e| {
            log::error!("Failed to query Lovely releases: {}", e);
            AppError::Network(e.to_string())
        })?;

    if !resp.status().is_redirection() {
        return Err(AppError::Network(format!(
            "Lovely latest-release lookup returned HTTP {}",
            resp.status()
        )));
    }

    // GitHub returns a 3xx with a Location header to /tag/vX.Y.Z
    let location = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    if !location.contains("/releases/tag/") {
        return Err(AppError::InvalidState(format!(
            "Lovely latest-release redirect did not contain a release tag: {location}"
        )));
    }

    // Extract the tag part after "/tag/" and strip leading 'v'
    let version = location
        .split('/')
        .next_back()
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();

    if version.is_empty() {
        log::error!("Failed to parse Lovely version from URL: {}", location);
        return Err(AppError::InvalidState(
            "Failed to resolve latest Lovely version tag".to_string(),
        ));
    }

    // Update in-memory cache
    {
        let mut guard = cache.lock().await;
        *guard = Some(CachedVersion {
            version: version.clone(),
            fetched_at: Instant::now(),
        });
    }

    // Persist to disk for next app launch
    save_persisted_version_cache(&version);

    log::debug!("Latest Lovely version: {}", version);
    Ok(version)
}

/// Remove currently installed Lovely artifacts so a clean reinstall can occur.
pub async fn remove_installed_lovely() -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        let config_dir = crate::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
        let bins_dir = config_dir.join("Balatro/bins");
        let lovely_path = bins_dir.join("liblovely.dylib");
        if lovely_path.exists() {
            tokio::fs::remove_file(&lovely_path)
                .await
                .map_err(|e| AppError::FileWrite {
                    path: lovely_path.clone(),
                    source: e.to_string(),
                })?;
        }
        if let Some(disabled_lovely_path) = disabled_injector_path(&lovely_path)
            && disabled_lovely_path.exists()
        {
            tokio::fs::remove_file(&disabled_lovely_path)
                .await
                .map_err(|e| AppError::FileWrite {
                    path: disabled_lovely_path.clone(),
                    source: e.to_string(),
                })?;
        }
        Ok(())
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        // Remove from game directory
        let balatro_paths = crate::finder::get_balatro_paths_cached();
        if !balatro_paths.is_empty() {
            let game_path = &balatro_paths[0];
            for dll_name in [WINDOWS_INJECTOR_FILENAME, LEGACY_WINDOWS_INJECTOR_FILENAME] {
                let dll_path = game_path.join(dll_name);
                for path in [disabled_injector_path(&dll_path), Some(dll_path)]
                    .into_iter()
                    .flatten()
                {
                    if path.exists() {
                        tokio::fs::remove_file(&path)
                            .await
                            .map_err(|e| AppError::FileWrite {
                                path: path.clone(),
                                source: e.to_string(),
                            })?;
                    }
                }
            }
            // Also remove liblovely.so on Linux
            #[cfg(target_os = "linux")]
            {
                let so_path = game_path.join("liblovely.so");
                if so_path.exists() {
                    let _ = tokio::fs::remove_file(&so_path).await;
                }
                if let Some(disabled_so_path) = disabled_injector_path(&so_path)
                    && disabled_so_path.exists()
                {
                    let _ = tokio::fs::remove_file(&disabled_so_path).await;
                }
            }
        }

        // Also remove cached copies on Linux
        #[cfg(target_os = "linux")]
        {
            if let Some(config_dir) = crate::config_dir() {
                let bins_dir = config_dir.join("Balatro/bins");
                let cached_dll = bins_dir.join(WINDOWS_INJECTOR_FILENAME);
                let cached_legacy_dll = bins_dir.join(LEGACY_WINDOWS_INJECTOR_FILENAME);
                let cached_so = bins_dir.join("liblovely.so");
                for dll_path in [cached_dll, cached_legacy_dll] {
                    for path in [disabled_injector_path(&dll_path), Some(dll_path)]
                        .into_iter()
                        .flatten()
                    {
                        if path.exists() {
                            let _ = tokio::fs::remove_file(&path).await;
                            log::info!("Removed cached Lovely injector {}", path.display());
                        }
                    }
                }
                if cached_so.exists() {
                    let _ = tokio::fs::remove_file(&cached_so).await;
                    log::info!("Removed cached liblovely.so");
                }
                if let Some(disabled_cached_so) = disabled_injector_path(&cached_so)
                    && disabled_cached_so.exists()
                {
                    let _ = tokio::fs::remove_file(&disabled_cached_so).await;
                    log::info!("Removed cached liblovely.so.disabled");
                }
            }
        }

        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(AppError::InvalidState(
            "Lovely injection is not supported on this platform.".into(),
        ))
    }
}

fn disabled_injector_path(active_path: &Path) -> Option<PathBuf> {
    let file_name = active_path.file_name()?;
    let mut disabled_name = OsString::from(file_name);
    disabled_name.push(".disabled");
    Some(active_path.with_file_name(disabled_name))
}

/// Returns true if an active injector file or its legacy `.disabled` variant exists.
pub fn injector_artifact_exists(active_path: &Path) -> bool {
    active_path.exists()
        || disabled_injector_path(active_path)
            .as_deref()
            .is_some_and(Path::exists)
}

fn unused_sibling_path(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    for sequence in 0.. {
        let sequence = if sequence == 0 {
            String::new()
        } else {
            format!(".{sequence}")
        };
        let candidate = path.with_file_name(format!("{file_name}{suffix}{sequence}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// Replace a library only after its complete replacement has been written and
/// synced beside the destination. The previous file is restored if publishing
/// the replacement fails.
fn replace_file_transactionally<R: Read>(target_path: &Path, source: R) -> Result<(), AppError> {
    replace_file_transactionally_with_permissions(target_path, source, None)
}

fn replace_file_transactionally_with_permissions<R: Read>(
    target_path: &Path,
    mut source: R,
    permissions: Option<std::fs::Permissions>,
) -> Result<(), AppError> {
    let parent = target_path.parent().ok_or_else(|| {
        AppError::InvalidState(format!(
            "Injector path has no parent: {}",
            target_path.display()
        ))
    })?;
    std::fs::create_dir_all(parent).map_err(|e| AppError::DirCreate {
        path: parent.to_path_buf(),
        source: e.to_string(),
    })?;

    let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| AppError::FileWrite {
        path: parent.to_path_buf(),
        source: format!("Failed to create staged injector: {e}"),
    })?;
    std::io::copy(&mut source, staged.as_file_mut()).map_err(|e| AppError::FileWrite {
        path: target_path.to_path_buf(),
        source: format!("Failed to stage injector: {e}"),
    })?;
    staged
        .as_file_mut()
        .flush()
        .map_err(|e| AppError::FileWrite {
            path: target_path.to_path_buf(),
            source: format!("Failed to flush staged injector: {e}"),
        })?;
    if let Some(permissions) = permissions {
        staged
            .as_file()
            .set_permissions(permissions)
            .map_err(|e| AppError::FileWrite {
                path: target_path.to_path_buf(),
                source: format!("Failed to set staged injector permissions: {e}"),
            })?;
    }
    staged
        .as_file()
        .sync_all()
        .map_err(|e| AppError::FileWrite {
            path: target_path.to_path_buf(),
            source: format!("Failed to sync staged injector: {e}"),
        })?;

    let backup = target_path
        .exists()
        .then(|| unused_sibling_path(target_path, ".bmm-update-backup"));
    if let Some(backup_path) = &backup {
        std::fs::rename(target_path, backup_path).map_err(|e| AppError::FileWrite {
            path: target_path.to_path_buf(),
            source: format!("Failed to preserve existing injector: {e}"),
        })?;
    }

    if let Err(error) = staged.persist(target_path) {
        if let Some(backup_path) = &backup {
            let _ = std::fs::rename(backup_path, target_path);
        }
        return Err(AppError::FileWrite {
            path: target_path.to_path_buf(),
            source: format!("Failed to publish staged injector: {}", error.error),
        });
    }

    if let Some(backup_path) = backup
        && let Err(error) = std::fs::remove_file(&backup_path)
    {
        log::warn!(
            "Installed new injector but could not remove temporary backup {}: {}",
            backup_path.display(),
            error
        );
    }
    Ok(())
}

/// Move pre-0.10 Windows injectors out of the loader search path after the new
/// winmm.dll has been installed. Backups are retained for manual rollback.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn retire_legacy_windows_injectors(game_path: &Path) -> Result<(), AppError> {
    let active = game_path.join(LEGACY_WINDOWS_INJECTOR_FILENAME);
    let disabled = disabled_injector_path(&active).expect("a DLL path always has a file name");
    let mut moved = Vec::new();

    for legacy_path in [active, disabled] {
        if !legacy_path.exists() {
            continue;
        }
        let backup_path = unused_sibling_path(&legacy_path, ".bmm-pre-0.10-backup");
        if let Err(error) = std::fs::rename(&legacy_path, &backup_path) {
            for (original, backup) in moved.into_iter().rev() {
                let _ = std::fs::rename(backup, original);
            }
            return Err(AppError::FileWrite {
                path: legacy_path,
                source: format!("Failed to retire legacy Lovely injector: {error}"),
            });
        }
        log::info!(
            "Retired legacy Lovely injector {} to {}",
            legacy_path.display(),
            backup_path.display()
        );
        moved.push((legacy_path, backup_path));
    }
    Ok(())
}

/// Set whether the Lovely injector is enabled or disabled by renaming files.
/// When disabled, the injector file is renamed to .disabled so it won't load.
/// This is a legacy escape hatch; launch mode should use runtime flags instead.
/// Returns Ok(()) if successful, or an error if the operation fails.
pub fn set_injector_enabled(enabled: bool) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        let config_dir = crate::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
        let bins_dir = config_dir.join("Balatro/bins");
        let active_path = bins_dir.join("liblovely.dylib");
        let disabled_path = bins_dir.join("liblovely.dylib.disabled");

        toggle_injector_file(&active_path, &disabled_path, enabled)
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        // Handle both current and legacy names so an older installation can be disabled
        // safely before its next migration.
        let balatro_paths = crate::finder::get_balatro_paths_cached();
        if balatro_paths.is_empty() {
            // No game path found - if we're enabling, that's an error; if disabling, nothing to do
            if enabled {
                return Err(AppError::DirNotFound(PathBuf::from("Balatro installation")));
            }
            return Ok(());
        }
        let game_path = &balatro_paths[0];
        for dll_name in [WINDOWS_INJECTOR_FILENAME, LEGACY_WINDOWS_INJECTOR_FILENAME] {
            let active_path = game_path.join(dll_name);
            let disabled_path =
                disabled_injector_path(&active_path).expect("a DLL path always has a file name");
            toggle_injector_file(&active_path, &disabled_path, enabled)?;
        }

        // On Linux, also handle liblovely.so for native LOVE builds
        #[cfg(target_os = "linux")]
        {
            if let Some(config_dir) = crate::config_dir() {
                let bins_dir = config_dir.join("Balatro/bins");
                let so_active = bins_dir.join("liblovely.so");
                let so_disabled = bins_dir.join("liblovely.so.disabled");
                // Also check in game directory
                let game_so_active = game_path.join("liblovely.so");
                let game_so_disabled = game_path.join("liblovely.so.disabled");

                // Toggle both locations (ignore errors if files don't exist)
                let _ = toggle_injector_file(&so_active, &so_disabled, enabled);
                let _ = toggle_injector_file(&game_so_active, &game_so_disabled, enabled);
            }
        }

        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(AppError::InvalidState(
            "Lovely injection is not supported on this platform.".into(),
        ))
    }
}

/// Check if the injector is currently enabled (active file exists).
pub fn is_injector_enabled() -> Result<bool, AppError> {
    #[cfg(target_os = "macos")]
    {
        let config_dir = crate::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
        let bins_dir = config_dir.join("Balatro/bins");
        let active_path = bins_dir.join("liblovely.dylib");
        Ok(active_path.exists())
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        let balatro_paths = crate::finder::get_balatro_paths_cached();
        if balatro_paths.is_empty() {
            // No game path - consider it "enabled" (no injector to disable)
            return Ok(true);
        }
        let game_path = &balatro_paths[0];
        Ok(
            [WINDOWS_INJECTOR_FILENAME, LEGACY_WINDOWS_INJECTOR_FILENAME]
                .iter()
                .any(|name| game_path.join(name).exists()),
        )
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(AppError::InvalidState(
            "Lovely injection is not supported on this platform.".into(),
        ))
    }
}

/// Helper function to toggle an injector file between active and disabled states.
fn toggle_injector_file(
    active_path: &Path,
    disabled_path: &Path,
    enable: bool,
) -> Result<(), AppError> {
    if enable {
        // Enable: rename .disabled to active
        if disabled_path.exists() {
            // If both exist (corrupt state), remove the disabled one
            if active_path.exists() {
                std::fs::remove_file(disabled_path).map_err(|e| AppError::FileWrite {
                    path: disabled_path.to_path_buf(),
                    source: e.to_string(),
                })?;
            } else {
                std::fs::rename(disabled_path, active_path).map_err(|e| AppError::FileWrite {
                    path: active_path.to_path_buf(),
                    source: format!("Failed to enable injector: {}", e),
                })?;
            }
        }
        // If neither exists, Lovely isn't installed - that's fine, we just save the preference
        Ok(())
    } else {
        // Disable: rename active to .disabled
        if active_path.exists() {
            // If both exist (corrupt state), remove the active one since we're disabling
            if disabled_path.exists() {
                std::fs::remove_file(active_path).map_err(|e| AppError::FileWrite {
                    path: active_path.to_path_buf(),
                    source: e.to_string(),
                })?;
            } else {
                std::fs::rename(active_path, disabled_path).map_err(|e| AppError::FileWrite {
                    path: disabled_path.to_path_buf(),
                    source: format!("Failed to disable injector: {}", e),
                })?;
            }
        }
        // If active doesn't exist, already disabled - that's fine
        Ok(())
    }
}

#[cfg(target_os = "macos")]
async fn download_and_install_lovely(target_path: &Path, version: &str) -> Result<(), AppError> {
    let temp_dir = tempfile::tempdir().map_err(|e| AppError::FileWrite {
        path: PathBuf::from("temp directory"),
        source: e.to_string(),
    })?;

    let arch = detect_architecture()?;
    let url = format!(
        "https://github.com/ethangreen-dev/lovely-injector/releases/download/v{version}/lovely-{arch}-apple-darwin.tar.gz"
    );

    log::info!("Downloading Lovely injector for macOS from: {}", url);

    // Download latest release
    let client = shared_client();
    let response = client.get(&url).send().await.map_err(|e| {
        log::error!("Failed to download Lovely injector: {}", e);
        AppError::Network(e.to_string())
    })?;

    if !response.status().is_success() {
        log::error!("Lovely download failed with HTTP {}", response.status());
        return Err(AppError::Network(format!(
            "Lovely download failed: HTTP {}",
            response.status()
        )));
    }

    // Save to temp file
    let temp_tar_gz = temp_dir.path().join("lovely.tar.gz");
    let mut file = File::create(&temp_tar_gz).map_err(|e| AppError::FileWrite {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;

    let bytes = response.bytes().await.map_err(|e| {
        log::error!("Failed to read Lovely download response: {}", e);
        AppError::Network(e.to_string())
    })?;

    log::debug!("Downloaded Lovely injector: {} bytes", bytes.len());
    std::io::copy(&mut bytes.as_ref(), &mut file).map_err(|e| AppError::FileWrite {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;

    // Extract and install
    let tar_gz = File::open(&temp_tar_gz).map_err(|e| AppError::FileRead {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);

    archive.unpack(&temp_dir).map_err(|e| AppError::FileRead {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;

    // Find the library in extracted files
    let extracted_lib = temp_dir.path().join("liblovely.dylib");
    let source = File::open(&extracted_lib).map_err(|e| AppError::FileRead {
        path: extracted_lib.clone(),
        source: e.to_string(),
    })?;
    replace_file_transactionally_with_permissions(
        target_path,
        source,
        Some(std::fs::Permissions::from_mode(0o755)),
    )?;

    Ok(())
}

#[cfg(target_os = "linux")]
async fn download_lovely_linux(target_path: &Path, version: &str) -> Result<(), AppError> {
    let temp_dir = tempfile::tempdir().map_err(|e| AppError::FileWrite {
        path: PathBuf::from("temp directory"),
        source: e.to_string(),
    })?;
    let temp_tar_gz = temp_dir.path().join("lovely.tar.gz");

    let url = format!(
        "https://github.com/ethangreen-dev/lovely-injector/releases/download/v{version}/lovely-x86_64-unknown-linux-gnu.tar.gz"
    );
    log::info!("Downloading Lovely injector for Linux from: {}", url);

    let response = reqwest::get(&url).await.map_err(|e| {
        log::error!("Failed to download Lovely injector for Linux: {}", e);
        AppError::Network(format!("Failed to download Lovely injector: {e}"))
    })?;

    if !response.status().is_success() {
        log::error!("Lovely download failed with HTTP {}", response.status());
        return Err(AppError::Network(format!(
            "Lovely download failed: HTTP {}",
            response.status()
        )));
    }

    let bytes = response.bytes().await.map_err(|e| {
        log::error!("Failed to read Lovely download response: {}", e);
        AppError::Network(format!("Failed to read download response: {e}"))
    })?;

    log::debug!("Downloaded Lovely injector: {} bytes", bytes.len());

    fs::write(&temp_tar_gz, &bytes).map_err(|e| AppError::FileWrite {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;

    let tar_gz = File::open(&temp_tar_gz).map_err(|e| AppError::FileRead {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);
    archive.unpack(&temp_dir).map_err(|e| AppError::FileRead {
        path: temp_tar_gz.clone(),
        source: e.to_string(),
    })?;

    let extracted_lib = temp_dir.path().join("liblovely.so");
    let source = File::open(&extracted_lib).map_err(|e| AppError::FileRead {
        path: extracted_lib.clone(),
        source: e.to_string(),
    })?;
    replace_file_transactionally_with_permissions(
        target_path,
        source,
        Some(std::fs::Permissions::from_mode(0o755)),
    )?;

    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
#[cfg(test)]
async fn download_windows_injector(target_path: &Path, version: &str) -> Result<(), AppError> {
    let injector = fetch_windows_injector(version).await?;
    replace_file_transactionally(target_path, injector.as_slice())
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
async fn fetch_windows_injector(version: &str) -> Result<Vec<u8>, AppError> {
    // The archive name stayed stable in Lovely 0.10.0, but its payload changed
    // from version.dll to winmm.dll.
    let url = format!(
        "https://github.com/ethangreen-dev/lovely-injector/releases/download/v{version}/lovely-x86_64-pc-windows-msvc.zip"
    );

    log::info!("Downloading Lovely injector for Windows from: {}", url);

    // Download the ZIP file
    let client = shared_client();
    let response = client.get(&url).send().await.map_err(|e| {
        log::error!("Failed to download Lovely injector for Windows: {}", e);
        AppError::Network(format!("Failed to download Lovely injector: {e}"))
    })?;

    if !response.status().is_success() {
        log::error!("Lovely download failed with HTTP {}", response.status());
        return Err(AppError::Network(format!(
            "Lovely download failed: HTTP {}",
            response.status()
        )));
    }

    let bytes = response.bytes().await.map_err(|e| {
        log::error!("Failed to read Lovely download response: {}", e);
        AppError::Network(format!("Failed to read download response: {e}"))
    })?;

    log::debug!("Downloaded Lovely injector: {} bytes", bytes.len());

    let cursor = std::io::Cursor::new(bytes);
    read_windows_injector_from_zip(cursor)
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn read_windows_injector_from_zip<R: Read + Seek>(reader: R) -> Result<Vec<u8>, AppError> {
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| AppError::FileRead {
        path: PathBuf::from("Lovely Windows release archive"),
        source: e.to_string(),
    })?;

    for i in 0..archive.len() {
        let mut file = match archive.by_index(i) {
            Ok(file) => file,
            Err(e) => {
                log::warn!("Failed to access zip entry: {}", e);
                continue;
            }
        };

        let is_injector = Path::new(file.name())
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(WINDOWS_INJECTOR_FILENAME));
        if is_injector && !file.is_dir() {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)
                .map_err(|e| AppError::FileRead {
                    path: PathBuf::from(WINDOWS_INJECTOR_FILENAME),
                    source: e.to_string(),
                })?;
            if !contents.starts_with(b"MZ") {
                return Err(AppError::InvalidState(format!(
                    "{} in downloaded archive is not a Windows PE library",
                    WINDOWS_INJECTOR_FILENAME
                )));
            }
            return Ok(contents);
        }
    }

    Err(AppError::InvalidState(format!(
        "{} not found in downloaded Lovely archive",
        WINDOWS_INJECTOR_FILENAME
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_toggle_injector_enable_from_disabled() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("version.dll");
        let disabled = dir.path().join("version.dll.disabled");

        // Create the disabled file
        std::fs::write(&disabled, b"test").unwrap();
        assert!(!active.exists());
        assert!(disabled.exists());

        // Enable should rename disabled to active
        toggle_injector_file(&active, &disabled, true).unwrap();
        assert!(active.exists());
        assert!(!disabled.exists());
    }

    #[test]
    fn test_toggle_injector_disable_from_enabled() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("version.dll");
        let disabled = dir.path().join("version.dll.disabled");

        // Create the active file
        std::fs::write(&active, b"test").unwrap();
        assert!(active.exists());
        assert!(!disabled.exists());

        // Disable should rename active to disabled
        toggle_injector_file(&active, &disabled, false).unwrap();
        assert!(!active.exists());
        assert!(disabled.exists());
    }

    #[test]
    fn test_toggle_injector_already_enabled() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("version.dll");
        let disabled = dir.path().join("version.dll.disabled");

        // Create both files (corrupt state)
        std::fs::write(&active, b"active").unwrap();
        std::fs::write(&disabled, b"disabled").unwrap();

        // Enable should remove the disabled file
        toggle_injector_file(&active, &disabled, true).unwrap();
        assert!(active.exists());
        assert!(!disabled.exists());
        assert_eq!(std::fs::read(&active).unwrap(), b"active");
    }

    #[test]
    fn test_toggle_injector_already_disabled() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("version.dll");
        let disabled = dir.path().join("version.dll.disabled");

        // Create both files (corrupt state)
        std::fs::write(&active, b"active").unwrap();
        std::fs::write(&disabled, b"disabled").unwrap();

        // Disable should remove the active file
        toggle_injector_file(&active, &disabled, false).unwrap();
        assert!(!active.exists());
        assert!(disabled.exists());
        assert_eq!(std::fs::read(&disabled).unwrap(), b"disabled");
    }

    #[test]
    fn test_toggle_injector_no_files_exist() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("version.dll");
        let disabled = dir.path().join("version.dll.disabled");

        // Neither file exists
        assert!(!active.exists());
        assert!(!disabled.exists());

        // Enable should succeed without error
        toggle_injector_file(&active, &disabled, true).unwrap();
        assert!(!active.exists());
        assert!(!disabled.exists());

        // Disable should also succeed without error
        toggle_injector_file(&active, &disabled, false).unwrap();
        assert!(!active.exists());
        assert!(!disabled.exists());
    }

    #[test]
    fn test_injector_artifact_exists_detects_disabled_file() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("version.dll");
        let disabled = dir.path().join("version.dll.disabled");

        std::fs::write(&disabled, b"disabled").unwrap();

        assert!(injector_artifact_exists(&active));
    }

    #[test]
    fn test_injector_installation_target_preserves_disabled_state() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("liblovely.so");
        let disabled = dir.path().join("liblovely.so.disabled");

        std::fs::write(&disabled, b"disabled").unwrap();
        assert_eq!(injector_installation_target(&active), disabled);

        std::fs::write(&active, b"active").unwrap();
        assert_eq!(injector_installation_target(&active), active);
    }

    #[test]
    fn test_windows_injector_detection_accepts_current_and_legacy_names() {
        let dir = tempdir().unwrap();
        assert!(!windows_injector_artifact_exists(dir.path()));

        std::fs::write(dir.path().join(WINDOWS_INJECTOR_FILENAME), b"current").unwrap();
        assert!(windows_injector_artifact_exists(dir.path()));

        std::fs::remove_file(dir.path().join(WINDOWS_INJECTOR_FILENAME)).unwrap();
        std::fs::write(
            dir.path()
                .join(format!("{LEGACY_WINDOWS_INJECTOR_FILENAME}.disabled")),
            b"legacy",
        )
        .unwrap();
        assert!(windows_injector_artifact_exists(dir.path()));
    }

    #[test]
    fn test_windows_injector_disabled_state_handles_legacy_migration() {
        let dir = tempdir().unwrap();
        let legacy_disabled = dir
            .path()
            .join(format!("{LEGACY_WINDOWS_INJECTOR_FILENAME}.disabled"));
        std::fs::write(&legacy_disabled, b"disabled legacy").unwrap();
        assert!(windows_injector_is_disabled(dir.path()));

        std::fs::write(
            dir.path().join(WINDOWS_INJECTOR_FILENAME),
            b"active current",
        )
        .unwrap();
        assert!(!windows_injector_is_disabled(dir.path()));
    }

    #[test]
    fn test_transactional_replace_publishes_complete_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join(WINDOWS_INJECTOR_FILENAME);
        std::fs::write(&target, b"old").unwrap();

        replace_file_transactionally(&target, &b"new injector"[..]).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"new injector");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn windows_release_zip(name: &str, contents: &[u8]) -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut archive = zip::ZipWriter::new(cursor);
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(contents).unwrap();
        archive.finish().unwrap().into_inner()
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn test_windows_release_extracts_winmm_dll() {
        let zip = windows_release_zip("nested/WINMM.DLL", b"MZ injector bytes");
        let extracted = read_windows_injector_from_zip(std::io::Cursor::new(zip)).unwrap();
        assert_eq!(extracted, b"MZ injector bytes");
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn test_windows_release_rejects_legacy_only_archive() {
        let zip = windows_release_zip(LEGACY_WINDOWS_INJECTOR_FILENAME, b"old injector");
        let error = read_windows_injector_from_zip(std::io::Cursor::new(zip)).unwrap_err();
        assert!(error.to_string().contains(WINDOWS_INJECTOR_FILENAME));
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn test_windows_release_rejects_non_pe_payload() {
        let zip = windows_release_zip(WINDOWS_INJECTOR_FILENAME, b"not a DLL");
        let error = read_windows_injector_from_zip(std::io::Cursor::new(zip)).unwrap_err();
        assert!(error.to_string().contains("not a Windows PE library"));
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[tokio::test]
    #[ignore = "downloads the current official Lovely release"]
    async fn test_live_windows_release_installs_winmm_into_temp_directory() {
        let dir = tempdir().unwrap();
        let target = dir.path().join(WINDOWS_INJECTOR_FILENAME);
        let version = get_latest_lovely_version_fresh().await.unwrap();

        download_windows_injector(&target, &version).await.unwrap();

        let installed = std::fs::read(&target).unwrap();
        assert!(installed.starts_with(b"MZ"), "expected a Windows PE DLL");
        assert!(!dir.path().join(LEGACY_WINDOWS_INJECTOR_FILENAME).exists());
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn test_legacy_migration_preserves_rollback_backup() {
        let dir = tempdir().unwrap();
        let legacy = dir.path().join(LEGACY_WINDOWS_INJECTOR_FILENAME);
        std::fs::write(&legacy, b"legacy injector").unwrap();

        retire_legacy_windows_injectors(dir.path()).unwrap();

        assert!(!legacy.exists());
        assert!(!dir.path().join(WINDOWS_INJECTOR_FILENAME).exists());
        let backup = std::fs::read_dir(dir.path())
            .unwrap()
            .map(Result::unwrap)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains("bmm-pre-0.10-backup")
            })
            .expect("legacy backup should remain available");
        assert_eq!(std::fs::read(backup.path()).unwrap(), b"legacy injector");
    }
}
