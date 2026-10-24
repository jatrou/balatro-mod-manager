//! Effective enable state shared with Steamodded and Lovely.
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// Batch enables share one blacklist. Serialize its read/modify/replace cycle.
static BLACKLIST_LOCK: Mutex<()> = Mutex::new(());
const SUBTREE_MAX_DEPTH: usize = 6;

fn collect_subdirs(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.to_path_buf()];
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if depth >= SUBTREE_MAX_DEPTH {
            continue;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_dir() || kind.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.starts_with('.')
                || matches!(name.as_str(), "node_modules" | "__macosx" | "lovely")
            {
                continue;
            }
            let path = entry.path();
            out.push(path.clone());
            stack.push((path, depth + 1));
        }
    }
    out
}

/// Resolve the actual top-level folder name used by Steamodded's blacklist.
/// Canonical boundaries reject sibling prefixes, traversal and links escaping Mods.
fn mod_folder(mods_root: &Path, mod_path: &Path) -> Result<String, String> {
    let root = mods_root
        .canonicalize()
        .map_err(|e| format!("Could not resolve Mods directory: {e}"))?;
    let path = mod_path
        .canonicalize()
        .map_err(|e| format!("Could not resolve mod path {}: {e}", mod_path.display()))?;
    if path == root || !path.starts_with(&root) {
        return Err(format!(
            "Mod path is outside or equals Mods directory: {}",
            mod_path.display()
        ));
    }
    // Preserve the enumerated alias name, rather than the target of an in-tree
    // symlink. Resolve the parent to support normal, verbatim and UNC inputs.
    for ancestor in mod_path.ancestors() {
        let Some(parent) = ancestor.parent() else {
            continue;
        };
        if parent.canonicalize().ok().as_ref() != Some(&root) {
            continue;
        }
        let Some(logical_name) = ancestor.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        for entry in fs::read_dir(mods_root)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let matches = if cfg!(windows) {
                name.eq_ignore_ascii_case(logical_name)
            } else {
                name == logical_name
            };
            if matches {
                return Ok(name.to_owned());
            }
        }
    }
    Err("Could not resolve the logical mod folder name".into())
}

fn ignored_ancestor(mods_root: &Path, mod_path: &Path) -> Result<Option<PathBuf>, String> {
    let root = mods_root.canonicalize().map_err(|e| e.to_string())?;
    let selected = mod_path.canonicalize().map_err(|e| e.to_string())?;
    // Callers already validated selected-target containment. Check the logical
    // input path too: resolving Pack/Alias -> Other must not hide Pack's marker.
    for selected_path in [mod_path, selected.as_path()] {
        for dir in selected_path.ancestors().skip(1) {
            let Ok(resolved) = dir.canonicalize() else {
                continue;
            };
            if resolved == root {
                break;
            }
            if resolved.starts_with(&root) && dir.join(".lovelyignore").exists() {
                return Ok(Some(dir.to_path_buf()));
            }
        }
    }
    Ok(None)
}

fn read_blacklist(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!(
            "Could not read Lovely blacklist {}: {e}",
            path.display()
        )),
    }
}

fn line_value(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

/// Missing paths are OFF. Blacklist names match Steamodded's exact case.
pub fn is_mod_enabled_at_path(mods_root: &Path, mod_path: &Path) -> Result<bool, String> {
    if !mod_path.exists() {
        return Ok(false);
    }
    let zipped = mod_path.is_file()
        && mod_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));
    if !mod_path.is_dir() && !zipped {
        return Ok(false);
    }
    let folder = mod_folder(mods_root, mod_path)?;
    let blacklist = read_blacklist(&mods_root.join("lovely/blacklist.txt"))?;
    if blacklist.lines().any(|line| line == folder) {
        return Ok(false);
    }
    // A parent ignore also disables a nested mod selected in BMM.
    if ignored_ancestor(mods_root, mod_path)?.is_some() {
        return Ok(false);
    }
    if zipped {
        return Ok(true);
    }
    Ok(!collect_subdirs(mod_path)
        .iter()
        .any(|dir| dir.join(".lovelyignore").exists()))
}

fn remove_blacklist_entry(mods_root: &Path, folder: &str) -> Result<(), String> {
    let _guard = BLACKLIST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = mods_root.join("lovely/blacklist.txt");
    let original = read_blacklist(&path)?;
    let updated: String = original
        .split_inclusive('\n')
        .filter(|line| line_value(line) != folder)
        .collect();
    if updated == original {
        return Ok(());
    }
    // Replace atomically so failed writes leave the original blacklist intact.
    let mut staged =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("Invalid blacklist path")?)
            .map_err(|e| format!("Could not stage Lovely blacklist update: {e}"))?;
    staged
        .write_all(updated.as_bytes())
        .and_then(|_| staged.as_file().sync_all())
        .map_err(|e| format!("Could not write Lovely blacklist update: {e}"))?;
    staged
        .persist(&path)
        .map_err(|e| format!("Could not replace Lovely blacklist: {}", e.error))?;
    Ok(())
}

/// Explicit enable removes only this top-level folder's blacklist lines, then
/// clears its existing ignore markers. Disable keeps using ignore markers.
pub fn set_mod_enabled_at_path(
    mods_root: &Path,
    mod_path: &Path,
    enabled: bool,
) -> Result<(), String> {
    if mod_path.is_file() {
        return Err(
            "Archive enable/disable is not supported. Extract the mod or manage it in Steamodded."
                .into(),
        );
    }
    if !mod_path.is_dir() {
        return Err(format!("Mod path does not exist: {}", mod_path.display()));
    }
    let folder = mod_folder(mods_root, mod_path)?;
    if enabled {
        if let Some(parent) = ignored_ancestor(mods_root, mod_path)? {
            return Err(format!(
                "Cannot enable this nested mod while its parent is disabled: {}. Enable the parent folder first.",
                parent.display()
            ));
        }
        remove_blacklist_entry(mods_root, &folder)?;
    }
    for dir in collect_subdirs(mod_path) {
        let marker = dir.join(".lovelyignore");
        if enabled {
            match fs::remove_file(&marker) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(format!(
                        "Failed to remove .lovelyignore in {}: {e}",
                        dir.display()
                    ));
                }
            }
        } else {
            fs::write(&marker, "")
                .map_err(|e| format!("Failed to create .lovelyignore in {}: {e}", dir.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn blacklist_and_ignore_are_both_effective_and_enable_preserves_other_lines() {
        let root = tempdir().unwrap();
        let selected = root.path().join("Selected");
        fs::create_dir_all(selected.join("nested")).unwrap();
        fs::create_dir(root.path().join("lovely")).unwrap();
        let file = root.path().join("lovely/blacklist.txt");
        fs::write(
            &file,
            "# comment\r\nOther\r\nSelected\r\nselected\r\n\r\nSelected\r\nLast",
        )
        .unwrap();
        assert!(!is_mod_enabled_at_path(root.path(), &selected).unwrap());
        set_mod_enabled_at_path(root.path(), &selected, false).unwrap();
        set_mod_enabled_at_path(root.path(), &selected, true).unwrap();
        assert_eq!(
            fs::read_to_string(file).unwrap(),
            "# comment\r\nOther\r\nselected\r\n\r\nLast"
        );
        assert!(is_mod_enabled_at_path(root.path(), &selected).unwrap());
        assert!(!selected.join("nested/.lovelyignore").exists());
    }

    #[test]
    fn missing_and_nested_paths_observe_real_folder_boundaries() {
        let root = tempdir().unwrap();
        let selected = root.path().join("Mods/Selected/nested");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir(root.path().join("Mods/lovely")).unwrap();
        fs::create_dir(root.path().join("ModsOther")).unwrap();
        let mods = root.path().join("Mods");
        fs::write(mods.join("lovely/blacklist.txt"), "Selected\n").unwrap();
        assert!(!is_mod_enabled_at_path(&mods, &selected).unwrap());
        assert!(!is_mod_enabled_at_path(&mods, &mods.join("Missing")).unwrap());
        assert!(set_mod_enabled_at_path(&mods, &root.path().join("ModsOther"), true).is_err());
        assert!(set_mod_enabled_at_path(&mods, &mods, false).is_err());
    }

    #[test]
    fn unreadable_blacklist_never_clears_ignore_or_reports_enabled() {
        let root = tempdir().unwrap();
        let selected = root.path().join("Selected");
        fs::create_dir(&selected).unwrap();
        fs::create_dir_all(root.path().join("lovely/blacklist.txt")).unwrap();
        fs::write(selected.join(".lovelyignore"), "").unwrap();
        assert!(set_mod_enabled_at_path(root.path(), &selected, true).is_err());
        assert!(selected.join(".lovelyignore").exists());
        assert!(is_mod_enabled_at_path(root.path(), &selected).is_err());
    }

    #[test]
    fn nested_enable_rejects_parent_ignore_before_changing_blacklist() {
        let root = tempdir().unwrap();
        let selected = root.path().join("Selected/middle/nested");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir(root.path().join("lovely")).unwrap();
        let blacklist = root.path().join("lovely/blacklist.txt");
        fs::write(&blacklist, "Selected\nOther\n").unwrap();
        for ancestor in [
            root.path().join("Selected"),
            root.path().join("Selected/middle"),
        ] {
            let marker = ancestor.join(".lovelyignore");
            fs::write(&marker, "").unwrap();
            let error = set_mod_enabled_at_path(root.path(), &selected, true).unwrap_err();
            assert!(error.contains("parent is disabled"));
            assert_eq!(fs::read_to_string(&blacklist).unwrap(), "Selected\nOther\n");
            assert!(marker.exists());
            assert!(!is_mod_enabled_at_path(root.path(), &selected).unwrap());
            fs::remove_file(marker).unwrap();
        }
        set_mod_enabled_at_path(root.path(), &selected, true).unwrap();
        assert!(is_mod_enabled_at_path(root.path(), &selected).unwrap());
        assert_eq!(fs::read_to_string(blacklist).unwrap(), "Other\n");
    }

    #[test]
    fn zip_status_uses_exact_archive_blacklist_name_without_pretending_to_toggle() {
        let root = tempdir().unwrap();
        let archive = root.path().join("Archive.zip");
        fs::write(&archive, "fixture").unwrap();
        fs::create_dir(root.path().join("lovely")).unwrap();
        let blacklist = root.path().join("lovely/blacklist.txt");
        fs::write(&blacklist, "archive.zip\n").unwrap();
        assert!(is_mod_enabled_at_path(root.path(), &archive).unwrap());
        fs::write(&blacklist, "Archive.zip\n").unwrap();
        assert!(!is_mod_enabled_at_path(root.path(), &archive).unwrap());
        assert!(set_mod_enabled_at_path(root.path(), &archive, true).is_err());
        assert_eq!(fs::read_to_string(blacklist).unwrap(), "Archive.zip\n");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn nested_alias_cannot_bypass_logical_parent_ignore() {
        let root = tempdir().unwrap();
        let pack = root.path().join("Pack");
        let other = root.path().join("Other");
        fs::create_dir(&pack).unwrap();
        fs::create_dir(&other).unwrap();
        fs::create_dir(root.path().join("lovely")).unwrap();
        let alias = pack.join("Alias");
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&other, &alias);
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&other, &alias);
        if let Err(error) = linked {
            if error.kind() == ErrorKind::PermissionDenied
                || (cfg!(windows) && error.raw_os_error() == Some(1314))
            {
                eprintln!("Symlink capability unavailable; alias regression skipped: {error}");
                return;
            }
            panic!("Could not create alias fixture: {error}");
        }
        fs::write(pack.join(".lovelyignore"), "").unwrap();
        assert!(!is_mod_enabled_at_path(root.path(), &alias).unwrap());
        let blacklist = root.path().join("lovely/blacklist.txt");
        fs::write(&blacklist, "Pack\nUnchanged\n").unwrap();
        let error = set_mod_enabled_at_path(root.path(), &alias, true).unwrap_err();
        assert!(error.contains("parent is disabled"));
        assert!(pack.join(".lovelyignore").exists());
        assert_eq!(fs::read_to_string(blacklist).unwrap(), "Pack\nUnchanged\n");
    }

    #[test]
    fn parallel_enables_preserve_other_entries_without_lost_updates() {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("lovely")).unwrap();
        for name in ["First", "Second"] {
            fs::create_dir(root.path().join(name)).unwrap();
        }
        let file = root.path().join("lovely/blacklist.txt");
        fs::write(&file, "First\nSecond\nUnchanged\n").unwrap();
        let tasks: Vec<_> = ["First", "Second"]
            .into_iter()
            .map(|name| {
                let root_path = root.path().to_path_buf();
                std::thread::spawn(move || {
                    set_mod_enabled_at_path(&root_path, &root_path.join(name), true).unwrap()
                })
            })
            .collect();
        for task in tasks {
            task.join().unwrap();
        }
        assert_eq!(fs::read_to_string(file).unwrap(), "Unchanged\n");
    }

    #[cfg(windows)]
    #[test]
    fn failed_blacklist_replacement_preserves_original_and_disabled_state() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = tempdir().unwrap();
        let selected = root.path().join("Selected");
        fs::create_dir(&selected).unwrap();
        fs::create_dir(root.path().join("lovely")).unwrap();
        let file = root.path().join("lovely/blacklist.txt");
        fs::write(&file, "Selected\r\nOther\r\n").unwrap();
        fs::write(selected.join(".lovelyignore"), "").unwrap();
        let _held = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&file)
            .unwrap();
        assert!(set_mod_enabled_at_path(root.path(), &selected, true).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "Selected\r\nOther\r\n");
        assert!(!is_mod_enabled_at_path(root.path(), &selected).unwrap());
        assert!(selected.join(".lovelyignore").exists());
    }
}
