//! Balatro Mod Manager Library
//!
//! This crate provides the core business logic for the Balatro Mod Manager,
//! including mod installation, detection, caching, and platform-specific handling.
//!
//! # Modules
//!
//! - [`backup`]: Backup and restore functionality for mod snapshots
//! - [`awesome_installer`]: Staged installation of Awesome Balatro mods
//! - [`balamod`]: Balamod (alternative mod loader) support and Balatro game detection
//! - [`cache`]: Binary cache for remote mod index data
//! - [`database`]: SQLite database for storing app settings and installed mod metadata
//! - [`discord_rpc`]: Discord Rich Presence integration
//! - [`errors`]: Application-wide error types
//! - [`finder`]: Automatic Balatro installation path detection
//! - [`installer`]: Mod installation and uninstallation logic
//! - [`local_mod_detection`]: Scanning Mods folder and detecting untracked mods
//! - [`logging`]: Centralized logging configuration
//! - [`lovely`]: Lovely (mod loader/injector) installer and version management
//! - [`mod_collections`]: Curated mod collection support
//! - [`rate_limiter`]: Rate limiter for GitHub API requests
//! - [`smods_installer`]: Steamodded/Talisman installer

/// Staged installation of Awesome Balatro mods.
pub mod awesome_installer;
/// Backup and restore functionality for mod snapshots.
pub mod backup;
/// Balamod support and Balatro game detection.
pub mod balamod;
/// Binary cache for remote mod index data.
pub mod cache;
/// SQLite database for app settings and installed mod metadata.
pub mod database;
/// Discord Rich Presence integration.
pub mod discord_rpc;
/// Application-wide error types.
pub mod errors;
/// Automatic Balatro installation path detection.
pub mod finder;
/// Shared HTTP client singletons.
pub mod http;
/// Mod installation and uninstallation logic.
pub mod installer;
/// Scanning Mods folder and detecting untracked mods.
pub mod local_mod_detection;
/// Centralized logging configuration.
pub mod logging;
/// Lovely (mod loader/injector) installer and version management.
pub mod lovely;
/// Curated mod collection support.
pub mod mod_collections;
/// Effective Steamodded/Lovely enable state and explicit toggles.
pub mod mod_enabled;
/// Rate limiter for GitHub API requests.
pub mod rate_limiter;
/// Steamodded/Talisman installer.
pub mod smods_installer;

use std::path::PathBuf;
#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Resolve the root used for BMM configuration data.
///
/// When `BMM_DATA_DIR` is set to an absolute path, that path replaces the
/// platform configuration directory. Callers should continue to append their
/// normal application subdirectory (for example, `Balatro`). Relative values
/// are ignored so a process working-directory change cannot redirect data.
pub fn config_dir() -> Option<PathBuf> {
    data_dir_override().or_else(dirs::config_dir)
}

/// Resolve the root used for BMM runtime state stored in the platform data directory.
pub fn data_dir() -> Option<PathBuf> {
    data_dir_override().or_else(dirs::data_dir)
}

/// Resolve the root used for BMM cache data.
///
/// An absolute `BMM_DATA_DIR` keeps caches under its `cache` child so tests and
/// smoke runs can isolate every BMM-owned file with one environment variable.
/// Without the override, this returns the platform cache directory.
pub fn cache_dir() -> Option<PathBuf> {
    data_dir_override()
        .map(|path| path.join("cache"))
        .or_else(dirs::cache_dir)
}

pub(crate) fn data_dir_override() -> Option<PathBuf> {
    std::env::var_os("BMM_DATA_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Resolve the mods directory on the host (config dir).
pub fn mods_dir() -> PathBuf {
    crate::local_mod_detection::resolve_mods_dir_path().unwrap_or_else(|_| {
        // Fallback to the platform config dir if detection failed
        config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Balatro")
            .join("Mods")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_bmm_data_dir_overrides_config_and_cache_roots() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let original = std::env::var_os("BMM_DATA_DIR");
        let _restore = scopeguard::guard(original, |value| unsafe {
            match value {
                Some(value) => std::env::set_var("BMM_DATA_DIR", value),
                None => std::env::remove_var("BMM_DATA_DIR"),
            }
        });

        unsafe { std::env::set_var("BMM_DATA_DIR", temp_dir.path()) };

        assert_eq!(config_dir().as_deref(), Some(temp_dir.path()));
        assert_eq!(data_dir().as_deref(), Some(temp_dir.path()));
        assert_eq!(cache_dir(), Some(temp_dir.path().join("cache")));
        assert_eq!(
            crate::local_mod_detection::mod_dir_candidates().unwrap(),
            vec![temp_dir.path().join("Balatro").join("Mods")]
        );
    }

    #[test]
    fn relative_bmm_data_dir_is_ignored() {
        let _lock = ENV_LOCK.lock().unwrap();
        let original = std::env::var_os("BMM_DATA_DIR");
        let _restore = scopeguard::guard(original, |value| unsafe {
            match value {
                Some(value) => std::env::set_var("BMM_DATA_DIR", value),
                None => std::env::remove_var("BMM_DATA_DIR"),
            }
        });
        let expected_config = dirs::config_dir();
        let expected_data = dirs::data_dir();
        let expected_cache = dirs::cache_dir();

        unsafe { std::env::set_var("BMM_DATA_DIR", "relative-path") };

        assert_eq!(config_dir(), expected_config);
        assert_eq!(data_dir(), expected_data);
        assert_eq!(cache_dir(), expected_cache);
    }
}
