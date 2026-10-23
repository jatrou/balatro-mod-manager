#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
use bmm_lib::errors::AppError;
use bmm_lib::lovely;

use crate::state::AppState;

/// Check whether Lovely is currently installed/present on this system.
/// - macOS: checks for `~/Library/Application Support/Balatro/bins/liblovely.dylib` (via config dir)
/// - Windows/Linux (Proton/Wine): checks current `winmm.dll` and legacy `version.dll`
#[tauri::command]
pub async fn is_lovely_installed(_state: tauri::State<'_, AppState>) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let config_dir = bmm_lib::config_dir()
            .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")).to_string())?;
        let lovely_path = config_dir
            .join("Balatro")
            .join("bins")
            .join("liblovely.dylib");
        Ok(lovely::injector_artifact_exists(&lovely_path))
    }

    #[cfg(target_os = "windows")]
    {
        // Prefer database install path if present
        let db = _state.db.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(path) = db.get_installation_path().map_err(|e| e.to_string())? {
            return Ok(lovely::windows_injector_artifact_exists(&PathBuf::from(
                path,
            )));
        }

        // Fallback to first detected Balatro path
        let candidates = bmm_lib::finder::get_balatro_paths_cached();
        if let Some(p) = candidates.first() {
            return Ok(lovely::windows_injector_artifact_exists(p));
        }
        Ok(false)
    }

    #[cfg(target_os = "linux")]
    {
        // Prefer database install path if present
        let db = _state.db.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(path) = db.get_installation_path().map_err(|e| e.to_string())? {
            let path = PathBuf::from(path);
            // Check for both native and Proton/Wine injectors.
            let so = path.join("liblovely.so");
            return Ok(lovely::injector_artifact_exists(&so)
                || lovely::windows_injector_artifact_exists(&path));
        }

        // Fallback to first detected Balatro path
        let candidates = bmm_lib::finder::get_balatro_paths_cached();
        if let Some(p) = candidates.first() {
            let so = p.join("liblovely.so");
            return Ok(lovely::injector_artifact_exists(&so)
                || lovely::windows_injector_artifact_exists(p));
        }
        Ok(false)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        // Other targets: Lovely injector not managed; do not warn.
        Ok(true)
    }
}

#[tauri::command]
pub async fn check_lovely_update(
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        // Unsupported target; skip.
        return Ok(None);
    }

    // Load latest from GitHub
    let latest = lovely::get_latest_lovely_version()
        .await
        .map_err(|e| e.to_string())?;

    // Compare to DB-stored version
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    match db.get_lovely_version() {
        Ok(Some(installed)) => {
            if installed.trim() != latest {
                Ok(Some(latest))
            } else {
                Ok(None)
            }
        }
        Ok(None) => Ok(Some(latest)), // Missing setting implies update/reinstall needed
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn update_lovely_to_latest(state: tauri::State<'_, AppState>) -> Result<String, String> {
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        return Err(
            "Lovely injection is only supported on Windows, macOS, and Linux/Proton.".into(),
        );
    }

    let latest = lovely::get_latest_lovely_version_fresh()
        .await
        .map_err(|e| e.to_string())?;

    let configured_path = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        db.get_installation_path()
            .map_err(|e| e.to_string())?
            .map(PathBuf::from)
    };

    // Publish the new injector only after a complete download has been staged.
    // The old injector remains usable if the download or replacement fails.
    lovely::update_lovely_at(configured_path.as_deref(), &latest)
        .await
        .map_err(|e| e.to_string())?;

    // Persist version
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    db.set_lovely_version(&latest).map_err(|e| e.to_string())?;

    Ok(latest)
}

#[cfg(test)]
mod tests {
    use bmm_lib::lovely;

    #[test]
    fn test_lovely_library_name_macos() {
        // On macOS, Lovely uses liblovely.dylib
        let expected = "liblovely.dylib";
        assert!(expected.ends_with(".dylib"));
    }

    #[test]
    fn test_lovely_library_name_windows() {
        // Lovely 0.10.0+ uses winmm.dll on Windows.
        let expected = lovely::WINDOWS_INJECTOR_FILENAME;
        assert!(expected.ends_with(".dll"));
        assert_eq!(expected, "winmm.dll");
    }

    #[test]
    fn test_lovely_library_name_linux() {
        // On Linux, Lovely can use liblovely.so (native) or winmm.dll (Proton)
        let native = "liblovely.so";
        let proton = lovely::WINDOWS_INJECTOR_FILENAME;
        assert!(native.ends_with(".so"));
        assert!(proton.ends_with(".dll"));
    }

    #[test]
    fn test_lovely_paths_are_relative() {
        // Lovely paths should be relative to game/config directories
        let macos_path = "Balatro/bins/liblovely.dylib";
        assert!(macos_path.contains("Balatro"));
        assert!(macos_path.contains("bins"));
    }
}
