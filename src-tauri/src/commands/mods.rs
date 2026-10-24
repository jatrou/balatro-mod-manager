use std::path::Path;
use std::path::PathBuf;

use crate::state::AppState;
use rayon::prelude::*;
use std::collections::HashMap;
#[cfg(test)]
use std::fs;

use bmm_lib::local_mod_detection::resolve_mods_dir_path;
use bmm_lib::mod_enabled::{is_mod_enabled_at_path, set_mod_enabled_at_path};

// Legacy Talisman can be installed beside the game instead of under Mods.
// Only an exact canonical finder-discovered Talisman path gets that exception.
fn scope_mod_root(
    mods_root: &Path,
    path: &Path,
    game_paths: &[PathBuf],
) -> Result<PathBuf, String> {
    if !path.exists() {
        return Ok(mods_root.to_path_buf());
    }
    let actual = path.canonicalize().map_err(|e| e.to_string())?;
    if actual.starts_with(mods_root.canonicalize().map_err(|e| e.to_string())?) {
        return Ok(mods_root.to_path_buf());
    }
    for game in game_paths {
        let talisman = game.join("Talisman");
        if talisman.is_dir() && talisman.canonicalize().ok().as_ref() == Some(&actual) {
            return Ok(game.clone());
        }
    }
    Err(format!(
        "Mod path is outside the managed Mods directory: {}",
        path.display()
    ))
}

fn effective_mod_root(mods_root: &Path, path: &Path) -> Result<PathBuf, String> {
    if let Ok(root) = scope_mod_root(mods_root, path, &[]) {
        return Ok(root);
    }
    scope_mod_root(
        mods_root,
        path,
        &bmm_lib::finder::get_balatro_paths_cached(),
    )
}

pub(crate) fn effective_enabled_at_path(mods_root: &Path, path: &Path) -> bool {
    let result =
        effective_mod_root(mods_root, path).and_then(|root| is_mod_enabled_at_path(&root, path));
    match result {
        Ok(enabled) => enabled,
        Err(error) => {
            log::warn!(
                "Could not determine enable state for {}: {error}",
                path.display()
            );
            false
        }
    }
}

fn set_effective_enabled_at_path(
    mods_root: &Path,
    path: &Path,
    enabled: bool,
) -> Result<(), String> {
    let root = effective_mod_root(mods_root, path)?;
    set_mod_enabled_at_path(&root, path, enabled)
}

/// Resolve an installed mod's path by name, holding the global DB lock only
/// for the lookup itself.
///
/// The `.lovelyignore` toggling below is plain filesystem work that does not
/// touch the database, yet the database lives behind a single process-wide
/// `Mutex`. Holding that lock across the (potentially slow, e.g. Windows
/// Defender scanning freshly written files) I/O would block *every* other
/// command that needs the DB, freezing the whole UI. So we copy out the path
/// and drop the guard before doing any filesystem work.
fn installed_mod_path(state: &AppState, mod_name: &str) -> Result<PathBuf, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let installed_mods = db.get_installed_mods()?;
    installed_mods
        .iter()
        .find(|m| m.name == mod_name)
        .map(|m| PathBuf::from(&m.path))
        .ok_or_else(|| format!("Mod not found: {mod_name}"))
}

#[tauri::command]
pub async fn is_mod_enabled(
    state: tauri::State<'_, AppState>,
    mod_name: String,
) -> Result<bool, String> {
    let mod_dir = installed_mod_path(&state, &mod_name)?;

    let mods_root = resolve_mods_dir_path()?;
    Ok(effective_enabled_at_path(&mods_root, &mod_dir))
}

#[tauri::command]
pub async fn toggle_mod_enabled(
    state: tauri::State<'_, AppState>,
    mod_name: String,
    enabled: bool,
) -> Result<(), String> {
    let mod_dir = installed_mod_path(&state, &mod_name)?;
    let mods_root = resolve_mods_dir_path()?;
    // Run the (blocking, parallel) filesystem work off the async runtime's
    // worker threads so a slow toggle never starves other commands.
    tauri::async_runtime::spawn_blocking(move || {
        set_effective_enabled_at_path(&mods_root, &mod_dir, enabled)
    })
    .await
    .map_err(|e| format!("Toggle task failed: {e}"))?
}

#[tauri::command]
pub async fn is_mod_enabled_by_path(mod_path: String) -> Result<bool, String> {
    let path = PathBuf::from(&mod_path);
    let mods_root = resolve_mods_dir_path()?;
    Ok(effective_enabled_at_path(&mods_root, &path))
}

#[tauri::command]
pub async fn toggle_mod_enabled_by_path(mod_path: String, enabled: bool) -> Result<(), String> {
    let path = PathBuf::from(&mod_path);
    let mods_root = resolve_mods_dir_path()?;
    set_effective_enabled_at_path(&mods_root, &path, enabled)
}

#[tauri::command]
pub async fn toggle_mods_enabled_batch(
    state: tauri::State<'_, AppState>,
    enabled: Vec<String>,
    disabled: Vec<String>,
    local_paths: Option<Vec<String>>,
) -> Result<(), String> {
    // Resolve DB-tracked paths under the lock, then release it before doing
    // any filesystem work (see `installed_mod_path`).
    let mut path_map: HashMap<String, PathBuf> = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        db.get_installed_mods()?
            .into_iter()
            .map(|m| (m.name, PathBuf::from(m.path)))
            .collect()
    };

    if let Some(paths) = local_paths {
        for p in paths {
            let path = PathBuf::from(&p);
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            path_map.entry(name).or_insert(path);
        }
    }

    let mut tasks: Vec<(PathBuf, bool)> = Vec::new();
    for name in enabled {
        if let Some(path) = path_map.get(&name) {
            tasks.push((path.clone(), true));
        }
    }
    for name in disabled {
        if let Some(path) = path_map.get(&name) {
            tasks.push((path.clone(), false));
        }
    }

    let mods_root = resolve_mods_dir_path()?;
    // Off-load the blocking parallel filesystem work from the async runtime.
    tauri::async_runtime::spawn_blocking(move || {
        tasks.par_iter().try_for_each(|(path, enabled)| {
            set_effective_enabled_at_path(&mods_root, path, *enabled)
        })
    })
    .await
    .map_err(|e| format!("Batch toggle task failed: {e}"))??;

    Ok(())
}

/// Return an enabled/disabled map for all installed mods in the DB plus provided local paths.
#[tauri::command]
pub async fn enabled_state_map(
    state: tauri::State<'_, AppState>,
    local_paths: Option<Vec<String>>,
) -> Result<HashMap<String, bool>, String> {
    let mut out: HashMap<String, bool> = HashMap::new();
    let mods_root = resolve_mods_dir_path()?;

    // Resolve DB-tracked mods under the lock, then release it before the
    // (potentially slow) recursive filesystem scans below.
    let installed_mods: Vec<(String, PathBuf)> = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        db.get_installed_mods()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|m| (m.name, PathBuf::from(m.path)))
            .collect()
    };
    for (name, p) in installed_mods {
        out.insert(name, effective_enabled_at_path(&mods_root, &p));
    }

    // Local mods passed from frontend
    if let Some(paths) = local_paths {
        for p in paths {
            let path = PathBuf::from(&p);
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let effective = effective_enabled_at_path(&mods_root, &path);
            out.insert(p, effective);
            out.entry(name).or_insert(effective);
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn external_scope_only_trusts_exact_game_root_talisman() {
        let dir = tempdir().unwrap();
        let mods = dir.path().join("Mods");
        let game = dir.path().join("Game");
        let talisman = game.join("Talisman");
        fs::create_dir(&mods).unwrap();
        fs::create_dir_all(&talisman).unwrap();
        fs::create_dir(game.join("Other")).unwrap();
        fs::create_dir(dir.path().join("Talisman")).unwrap();
        assert_eq!(
            scope_mod_root(&mods, &talisman, std::slice::from_ref(&game)).unwrap(),
            game
        );
        assert!(scope_mod_root(&mods, &game.join("Other"), std::slice::from_ref(&game)).is_err());
        assert!(scope_mod_root(&mods, &dir.path().join("Talisman"), &[game]).is_err());
    }

    #[test]
    fn test_set_mod_enabled_creates_lovelyignore() {
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("TestMod");
        fs::create_dir(&mod_dir).unwrap();

        // Initially no .lovelyignore
        assert!(!mod_dir.join(".lovelyignore").exists());

        // Disable the mod
        set_mod_enabled_at_path(dir.path(), &mod_dir, false).unwrap();
        assert!(mod_dir.join(".lovelyignore").exists());

        // Enable the mod
        set_mod_enabled_at_path(dir.path(), &mod_dir, true).unwrap();
        assert!(!mod_dir.join(".lovelyignore").exists());
    }

    #[test]
    fn test_set_mod_enabled_handles_subdirectories() {
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("TestMod");
        let sub_dir = mod_dir.join("submod");
        fs::create_dir_all(&sub_dir).unwrap();

        // Disable - should create .lovelyignore in both places
        set_mod_enabled_at_path(dir.path(), &mod_dir, false).unwrap();
        assert!(mod_dir.join(".lovelyignore").exists());
        assert!(sub_dir.join(".lovelyignore").exists());

        // Enable - should remove both
        set_mod_enabled_at_path(dir.path(), &mod_dir, true).unwrap();
        assert!(!mod_dir.join(".lovelyignore").exists());
        assert!(!sub_dir.join(".lovelyignore").exists());
    }

    #[test]
    fn test_set_mod_enabled_nonexistent_path() {
        let result = set_mod_enabled_at_path(
            Path::new("/nonexistent"),
            Path::new("/nonexistent/path"),
            true,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_set_mod_enabled_idempotent() {
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("TestMod");
        fs::create_dir(&mod_dir).unwrap();

        // Disable twice should work
        set_mod_enabled_at_path(dir.path(), &mod_dir, false).unwrap();
        set_mod_enabled_at_path(dir.path(), &mod_dir, false).unwrap();
        assert!(mod_dir.join(".lovelyignore").exists());

        // Enable twice should work
        set_mod_enabled_at_path(dir.path(), &mod_dir, true).unwrap();
        set_mod_enabled_at_path(dir.path(), &mod_dir, true).unwrap();
        assert!(!mod_dir.join(".lovelyignore").exists());
    }

    #[test]
    fn test_set_mod_enabled_walks_deep_nesting() {
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("TestMod");
        let deep = mod_dir.join("sub1").join("sub2").join("sub3");
        fs::create_dir_all(&deep).unwrap();

        set_mod_enabled_at_path(dir.path(), &mod_dir, false).unwrap();
        assert!(mod_dir.join(".lovelyignore").exists());
        assert!(mod_dir.join("sub1").join(".lovelyignore").exists());
        assert!(mod_dir.join("sub1/sub2").join(".lovelyignore").exists());
        assert!(deep.join(".lovelyignore").exists());

        set_mod_enabled_at_path(dir.path(), &mod_dir, true).unwrap();
        assert!(!mod_dir.join(".lovelyignore").exists());
        assert!(!deep.join(".lovelyignore").exists());
    }

    #[test]
    fn test_is_enabled_recursive_detects_nested_marker() {
        // Mimics Steamodded disabling a nested mod folder (e.g. Cryptid).
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("Cryptid");
        let inner = mod_dir.join("Cryptid-main");
        fs::create_dir_all(&inner).unwrap();

        assert!(is_mod_enabled_at_path(dir.path(), &mod_dir).unwrap());

        // Only the inner folder has the marker; top-level is clean.
        fs::write(inner.join(".lovelyignore"), "").unwrap();
        assert!(!is_mod_enabled_at_path(dir.path(), &mod_dir).unwrap());
    }

    #[test]
    fn test_is_enabled_recursive_ignores_dot_and_system_dirs() {
        // .git / node_modules shouldn't be scanned for markers; otherwise a
        // hidden-folder commit could confuse the enabled-state detector.
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("Mod");
        let dotdir = mod_dir.join(".git");
        fs::create_dir_all(&dotdir).unwrap();
        fs::write(dotdir.join(".lovelyignore"), "").unwrap();
        assert!(is_mod_enabled_at_path(dir.path(), &mod_dir).unwrap());
    }
}
