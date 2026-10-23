//! Two-step Awesome Balatro installation with app database integration.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bmm_lib::awesome_installer::{self, InstallSource, Loader, PreparedInstall};
use bmm_lib::local_mod_detection;
use once_cell::sync::Lazy;
use tauri::Emitter;

use crate::awesome_source;
use crate::compat_helper;
use crate::models::ModsChangedEvent;
use crate::state::AppState;

struct PendingCatalogMod {
    title: String,
    dependencies: Vec<String>,
    version: Option<String>,
}

static PENDING: Lazy<Mutex<HashMap<String, PendingCatalogMod>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn mods_dir() -> Result<PathBuf, String> {
    let path = local_mod_detection::resolve_mods_dir_path()?;
    std::fs::create_dir_all(&path)
        .map_err(|e| format!("Could not create Mods directory {}: {e}", path.display()))?;
    Ok(path)
}

fn safe_folder_name(requested: &str, title: &str) -> String {
    let source = if requested.trim().is_empty() {
        title
    } else {
        requested
    };
    let mut result = String::new();
    for c in source.chars() {
        let safe = if c.is_control() || "<>:\"/\\|?*".contains(c) {
            '-'
        } else {
            c
        };
        if result.len() + safe.len_utf8() > 100 {
            break;
        }
        result.push(safe);
    }
    let result = result.trim().trim_matches(['.', '-']).trim().to_string();
    let stem = result.split('.').next().unwrap_or("").to_ascii_uppercase();
    if result.is_empty() {
        "Mod".to_string()
    } else if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "LPT1"
    ) {
        format!("Mod-{result}")
    } else {
        result
    }
}

fn normalize_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn reject_withdrawn(title: &str, url: Option<&str>) -> Result<(), String> {
    let key = normalize_title(title);
    if let Some(reason) = url.and_then(awesome_source::withdrawn_source_reason) {
        return Err(reason.to_string());
    }
    if key == "jokerdisplay" {
        return Err(
            "This author withdrew the mod from BMM; it cannot be installed through BMM".to_string(),
        );
    }
    if key.contains("joyousspring") {
        return Err(
            "Joyous Spring was withdrawn from BMM; it cannot be installed through BMM".to_string(),
        );
    }
    if key == "multiplayer" || key == "balatromultiplayer" {
        return Err("Balatro Multiplayer requires an unsupported installation layout; follow its source instructions manually".to_string());
    }
    Ok(())
}

fn parse_loader(value: &str) -> Result<Loader, String> {
    match value.to_ascii_lowercase().as_str() {
        "steamodded" => Ok(Loader::Steamodded),
        "lovely" => Ok(Loader::Lovely),
        _ => Err("Choose Steamodded or Lovely as the mod loader".to_string()),
    }
}

fn catalog_metadata(
    title: String,
    prepared: &PreparedInstall,
    loader: Loader,
) -> PendingCatalogMod {
    let mut seen = HashSet::new();
    let mut dependencies: Vec<String> = prepared
        .mods
        .iter()
        .flat_map(|m| m.dependencies.iter())
        .filter(|dep| seen.insert(dep.to_ascii_lowercase()))
        .cloned()
        .collect();
    let required_loader = match loader {
        Loader::Steamodded => "Steamodded",
        Loader::Lovely => "Lovely",
    };
    if seen.insert(required_loader.to_ascii_lowercase()) {
        dependencies.push(required_loader.to_string());
    }
    let version = if prepared.mods.len() == 1 {
        prepared.mods[0].version.clone()
    } else {
        None
    };
    PendingCatalogMod {
        title,
        dependencies,
        version,
    }
}

#[tauri::command]
pub async fn prepare_awesome_install(
    title: String,
    folder_name: String,
    loader: String,
    url: Option<String>,
    local_path: Option<String>,
) -> Result<PreparedInstall, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Mod title is required".to_string());
    }
    reject_withdrawn(&title, url.as_deref())?;
    let loader = parse_loader(&loader)?;
    let source = awesome_source::resolve_source(url, local_path).await?;
    let source_url = match &source {
        InstallSource::Url(value) | InstallSource::UrlSubtree { url: value, .. } => {
            Some(value.as_str())
        }
        InstallSource::Local(_) => None,
    };
    reject_withdrawn(&title, source_url)?;
    let mods_dir = mods_dir()?;
    let folder_name = safe_folder_name(&folder_name, &title);
    let prepared = awesome_installer::prepare_install(&mods_dir, source, &folder_name, loader)
        .await
        .map_err(|e| e.to_string())?;
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).insert(
        prepared.token.clone(),
        catalog_metadata(title, &prepared, loader),
    );
    Ok(prepared)
}

fn existing_title_at_other_path(
    db: &bmm_lib::database::Database,
    title: &str,
    path: &Path,
) -> Result<bool, String> {
    Ok(db
        .get_installed_mods()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|item| item.name.eq_ignore_ascii_case(title) && Path::new(&item.path) != path))
}

fn rollback_new_dir(path: &Path, expected: &Path) -> Result<(), String> {
    if path != expected {
        return Err(format!(
            "Installed path differed from prepared destination: {}",
            path.display()
        ));
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|e| format!("Could not inspect installed directory for rollback: {e}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!(
            "Installed path changed before rollback: {}",
            path.display()
        ));
    }
    std::fs::remove_dir_all(path)
        .map_err(|e| format!("Could not roll back installed directory: {e}"))
}

#[tauri::command]
pub async fn finish_awesome_install(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    token: String,
    accept_warnings: bool,
) -> Result<String, String> {
    let mods_dir = mods_dir()?;
    let prepared =
        awesome_installer::inspect_prepared(&mods_dir, &token).map_err(|e| e.to_string())?;
    let catalog = {
        let pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        pending.get(&token).map(|m| PendingCatalogMod {
            title: m.title.clone(),
            dependencies: m.dependencies.clone(),
            version: m.version.clone(),
        })
    }
    .ok_or(
        "This installation was prepared in another app session. Prepare it again before finishing.",
    )?;

    // Check before touching Mods, then check again under the DB mutex after
    // commit in case another install registered the same catalog title.
    {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        if existing_title_at_other_path(&db, &catalog.title, &prepared.destination)? {
            return Err(format!(
                "A different installation is already registered as '{}'",
                catalog.title
            ));
        }
    }
    let path = awesome_installer::commit_install(&mods_dir, &token, accept_warnings)
        .map_err(|e| e.to_string())?;
    let db_result = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        existing_title_at_other_path(&db, &catalog.title, &path).and_then(|conflict| {
            if conflict {
                Err(format!(
                    "A different installation is already registered as '{}'",
                    catalog.title
                ))
            } else {
                db.add_installed_mod(
                    &catalog.title,
                    &path.to_string_lossy(),
                    &catalog.dependencies,
                    catalog.version,
                )
                .map_err(|e| e.to_string())
            }
        })
    };
    if let Err(err) = db_result {
        return match rollback_new_dir(&path, &prepared.destination) {
            Ok(()) => Err(format!(
                "Could not register mod in database; installation rolled back: {err}"
            )),
            Err(rollback_err) => Err(format!(
                "Could not register mod in database: {err}. {rollback_err}. Installed files remain at {}",
                path.display()
            )),
        };
    }
    PENDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&token);
    local_mod_detection::clear_detection_cache();
    let enabled = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        db.is_compat_helper_enabled().unwrap_or(false)
    };
    if let Err(err) = compat_helper::sync_compat_helper(enabled) {
        log::warn!("Failed to sync compatibility helper after Awesome install: {err}");
    }
    let _ = app_handle.emit(
        "installed-mods-changed",
        ModsChangedEvent {
            added: vec![catalog.title],
            removed: Vec::new(),
            full_refresh: false,
        },
    );
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn cancel_awesome_install(token: String) -> Result<(), String> {
    awesome_installer::cancel_install(&mods_dir()?, &token).map_err(|e| e.to_string())?;
    PENDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&token);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmm_lib::awesome_installer::{InstallMetadata, InstallWarning};

    #[test]
    fn loader_validation_is_explicit() {
        assert_eq!(parse_loader("Steamodded").unwrap(), Loader::Steamodded);
        assert_eq!(parse_loader("lovely").unwrap(), Loader::Lovely);
        assert!(parse_loader("other").is_err());
    }

    #[test]
    fn catalog_metadata_keeps_unique_dependencies() {
        let prepared = PreparedInstall {
            token: "test".into(),
            destination: PathBuf::from("Mods/Test"),
            warnings: Vec::<InstallWarning>::new(),
            mods: vec![InstallMetadata {
                id: "test".into(),
                name: "Test".into(),
                version: Some("1.0".into()),
                dependencies: vec!["Steamodded".into(), "steamodded".into(), "Talisman".into()],
                conflicts: vec![],
            }],
        };
        let meta = catalog_metadata("Catalog Test".into(), &prepared, Loader::Steamodded);
        assert_eq!(meta.dependencies, ["Steamodded", "Talisman"]);
        assert_eq!(meta.version.as_deref(), Some("1.0"));
        let lovely = catalog_metadata("Catalog Test".into(), &prepared, Loader::Lovely);
        assert_eq!(lovely.dependencies, ["Steamodded", "Talisman", "Lovely"]);
    }

    #[test]
    fn unsafe_title_and_folder_names_are_handled() {
        assert_eq!(
            safe_folder_name("", "1 Hand? You're Joking?"),
            "1 Hand- You're Joking"
        );
        assert!(reject_withdrawn("JokerDisplay", None).is_err());
        assert!(reject_withdrawn("Other", Some("https://github.com/nh6574/Other")).is_err());
    }
}
