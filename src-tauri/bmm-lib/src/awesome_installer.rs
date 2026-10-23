//! Staged, non-overwriting installation for catalog and manually downloaded mods.
//!
//! The caller resolves catalog/GitHub links to a downloadable URL. Preparation
//! only writes beside `Mods`; commit rechecks the live installation and moves a
//! single validated directory into `Mods` on the same filesystem.

use crate::errors::AppError;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_FILES: usize = 10_000;
const STAGING_DIR: &str = ".bmm-awesome-staging";
const STAGE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum InstallSource {
    Url(String),
    /// `path` is relative to the root of a repository archive.
    UrlSubtree {
        url: String,
        path: String,
    },
    Local(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Steamodded,
    Lovely,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallMetadata {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub dependencies: Vec<String>,
    pub conflicts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallWarning {
    pub code: String,
    pub message: String,
    pub blocking: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreparedInstall {
    pub token: String,
    pub destination: PathBuf,
    pub warnings: Vec<InstallWarning>,
    pub mods: Vec<InstallMetadata>,
}

#[derive(Serialize, Deserialize)]
struct StageManifest {
    prepared: PreparedInstall,
    mods_dir: PathBuf,
    loader: Loader,
    unverified: bool,
    payload_fingerprint: u64,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidState(message.into())
}

fn io_error(context: &str, error: impl std::fmt::Display) -> AppError {
    invalid(format!("{context}: {error}"))
}

fn validated_name(name: &str) -> Result<&str, AppError> {
    if name.is_empty()
        || name.len() > 120
        || name == "."
        || name == ".."
        || name.trim_end_matches([' ', '.']) != name
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || matches!(
            name.to_ascii_uppercase().as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "LPT1"
        )
    {
        return Err(invalid("Invalid mod folder name"));
    }
    Ok(name)
}

fn validated_token(token: &str) -> Result<(), AppError> {
    if !token.starts_with("a-")
        || token.len() > 80
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(invalid("Invalid staging token"));
    }
    Ok(())
}

fn paths(mods_dir: &Path) -> Result<(PathBuf, PathBuf), AppError> {
    let name = mods_dir
        .file_name()
        .ok_or_else(|| invalid("Invalid Mods path"))?;
    let parent = mods_dir
        .parent()
        .ok_or_else(|| invalid("Invalid Mods path"))?;
    if !parent.is_dir() {
        return Err(invalid("Mods parent directory does not exist"));
    }
    let parent = parent
        .canonicalize()
        .map_err(|e| io_error("Resolve Mods parent", e))?;
    let requested = parent.join(name);
    let mods = if fs::symlink_metadata(&requested).is_ok() {
        if !requested.is_dir() {
            return Err(invalid("Mods path is not a directory"));
        }
        requested
            .canonicalize()
            .map_err(|e| io_error("Resolve Mods directory", e))?
    } else {
        requested
    };
    let staging = mods
        .parent()
        .ok_or_else(|| invalid("Invalid resolved Mods path"))?
        .join(STAGING_DIR);
    if let Ok(meta) = fs::symlink_metadata(&staging)
        && (!meta.is_dir() || meta.file_type().is_symlink())
    {
        return Err(invalid("Staging location is not a real directory"));
    }
    Ok((mods, staging))
}

fn safe_relative(raw: &str) -> Result<PathBuf, AppError> {
    let normalized = raw.replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.contains(':') || path.is_absolute() {
        return Err(invalid("Archive contains an unsafe path"));
    }
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => return Err(invalid("Archive contains path traversal")),
        }
    }
    Ok(relative)
}

fn copy_limited(
    reader: &mut impl Read,
    writer: &mut impl Write,
    remaining: &mut u64,
) -> Result<(), AppError> {
    let copied = io::copy(&mut reader.take(remaining.saturating_add(1)), writer)
        .map_err(|e| io_error("Copy mod content", e))?;
    if copied > *remaining {
        return Err(AppError::ArchiveTooLarge {
            reason: "Mod package exceeds extraction limit".into(),
        });
    }
    *remaining -= copied;
    Ok(())
}

fn payload_fingerprint(root: &Path) -> Result<u64, AppError> {
    fn mix(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash = (*hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
        }
    }
    fn walk(
        path: &Path,
        relative: &Path,
        hash: &mut u64,
        count: &mut usize,
        total: &mut u64,
    ) -> Result<(), AppError> {
        let mut entries = fs::read_dir(path)
            .map_err(|e| io_error("Read staged content", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| io_error("Read staged content", e))?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            *count += 1;
            if *count > MAX_FILES {
                return Err(AppError::ArchiveTooLarge {
                    reason: "Too many staged files".into(),
                });
            }
            let kind = entry
                .file_type()
                .map_err(|e| io_error("Inspect staged content", e))?;
            if kind.is_symlink() {
                return Err(invalid("Staged content contains a symlink"));
            }
            let child = relative.join(entry.file_name());
            mix(hash, child.to_string_lossy().as_bytes());
            if kind.is_dir() {
                mix(hash, b"D");
                walk(&entry.path(), &child, hash, count, total)?;
            } else if kind.is_file() {
                mix(hash, b"F");
                let mut reader =
                    File::open(entry.path()).map_err(|e| io_error("Read staged file", e))?;
                let mut buffer = [0u8; 64 * 1024];
                loop {
                    let size = reader
                        .read(&mut buffer)
                        .map_err(|e| io_error("Read staged file", e))?;
                    if size == 0 {
                        break;
                    }
                    *total += size as u64;
                    if *total > MAX_EXTRACTED_BYTES {
                        return Err(AppError::ArchiveTooLarge {
                            reason: "Staged content exceeds limit".into(),
                        });
                    }
                    mix(hash, &buffer[..size]);
                }
            } else {
                return Err(invalid("Staged content contains an unsupported file"));
            }
        }
        Ok(())
    }
    let mut hash = 14_695_981_039_346_656_037;
    let mut count = 0;
    let mut total = 0;
    walk(root, Path::new(""), &mut hash, &mut count, &mut total)?;
    Ok(hash)
}

fn extract_zip(source: &Path, output: &Path) -> Result<(), AppError> {
    let file = File::open(source).map_err(|e| io_error("Open ZIP", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| io_error("Read ZIP", e))?;
    if archive.len() > MAX_FILES {
        return Err(AppError::ArchiveTooLarge {
            reason: "Too many archive entries".into(),
        });
    }
    let mut remaining = MAX_EXTRACTED_BYTES;
    let mut seen = HashSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| io_error("Read ZIP entry", e))?;
        let relative = safe_relative(entry.name())?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| (mode & 0o170000) == 0o120000)
        {
            return Err(invalid("ZIP symlinks are not supported"));
        }
        let target = output.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|e| io_error("Create ZIP directory", e))?;
        } else {
            if !seen.insert(relative.to_string_lossy().to_lowercase()) || target.exists() {
                return Err(invalid("ZIP contains a duplicate file path"));
            }
            fs::create_dir_all(target.parent().unwrap())
                .map_err(|e| io_error("Create ZIP parent", e))?;
            let mut writer = File::create(&target).map_err(|e| io_error("Create ZIP file", e))?;
            copy_limited(&mut entry, &mut writer, &mut remaining)?;
        }
    }
    Ok(())
}

fn extract_tar<R: Read>(reader: R, output: &Path) -> Result<(), AppError> {
    let mut archive = tar::Archive::new(reader);
    let mut remaining = MAX_EXTRACTED_BYTES;
    let mut seen = HashSet::new();
    for (index, entry) in archive
        .entries()
        .map_err(|e| io_error("Read TAR", e))?
        .enumerate()
    {
        if index >= MAX_FILES {
            return Err(AppError::ArchiveTooLarge {
                reason: "Too many archive entries".into(),
            });
        }
        let mut entry = entry.map_err(|e| io_error("Read TAR entry", e))?;
        let relative = safe_relative(&String::from_utf8_lossy(&entry.path_bytes()))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = output.join(&relative);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&target).map_err(|e| io_error("Create TAR directory", e))?;
        } else if entry.header().entry_type().is_file() {
            if !seen.insert(relative.to_string_lossy().to_lowercase()) || target.exists() {
                return Err(invalid("TAR contains a duplicate file path"));
            }
            fs::create_dir_all(target.parent().unwrap())
                .map_err(|e| io_error("Create TAR parent", e))?;
            let mut writer = File::create(&target).map_err(|e| io_error("Create TAR file", e))?;
            copy_limited(&mut entry, &mut writer, &mut remaining)?;
        } else {
            return Err(invalid("TAR links and special files are not supported"));
        }
    }
    Ok(())
}

fn copy_folder(
    source: &Path,
    output: &Path,
    count: &mut usize,
    remaining: &mut u64,
) -> Result<(), AppError> {
    for entry in fs::read_dir(source).map_err(|e| io_error("Read local folder", e))? {
        let entry = entry.map_err(|e| io_error("Read local entry", e))?;
        *count += 1;
        if *count > MAX_FILES {
            return Err(AppError::ArchiveTooLarge {
                reason: "Too many local files".into(),
            });
        }
        let kind = entry
            .file_type()
            .map_err(|e| io_error("Inspect local entry", e))?;
        if kind.is_symlink() {
            return Err(invalid("Local source contains a symlink"));
        }
        let target = output.join(entry.file_name());
        if kind.is_dir() {
            fs::create_dir(&target).map_err(|e| io_error("Create local directory", e))?;
            copy_folder(&entry.path(), &target, count, remaining)?;
        } else if kind.is_file() {
            let mut reader =
                File::open(entry.path()).map_err(|e| io_error("Open local file", e))?;
            let mut writer = File::create(target).map_err(|e| io_error("Create copied file", e))?;
            copy_limited(&mut reader, &mut writer, remaining)?;
        } else {
            return Err(invalid("Local source contains an unsupported file"));
        }
    }
    Ok(())
}

fn check_html(source: &Path) -> Result<(), AppError> {
    let mut file = File::open(source).map_err(|e| io_error("Open source", e))?;
    let mut first = vec![0; 512];
    let size = file
        .read(&mut first)
        .map_err(|e| io_error("Read source", e))?;
    first.truncate(size);
    let trimmed = String::from_utf8_lossy(&first)
        .trim_start()
        .to_ascii_lowercase();
    if trimmed.starts_with("<!doctype html") || trimmed.starts_with("<html") || first.contains(&0) {
        return Err(invalid("Downloaded file is HTML or binary, not Lua"));
    }
    Ok(())
}

fn unpack_source(source: &Path, source_name: &str, output: &Path) -> Result<(), AppError> {
    if source.is_dir() {
        let mut count = 0;
        let mut remaining = MAX_EXTRACTED_BYTES;
        return copy_folder(source, output, &mut count, &mut remaining);
    }
    let size = fs::metadata(source)
        .map_err(|e| io_error("Inspect source", e))?
        .len();
    if size > MAX_SOURCE_BYTES {
        return Err(AppError::ArchiveTooLarge {
            reason: "Source exceeds 512 MiB".into(),
        });
    }
    let mut file = File::open(source).map_err(|e| io_error("Open source", e))?;
    let mut magic = [0u8; 512];
    let read = file
        .read(&mut magic)
        .map_err(|e| io_error("Read source", e))?;
    if magic[..read].starts_with(b"PK\x03\x04") {
        extract_zip(source, output)
    } else if magic[..read].starts_with(&[0x1f, 0x8b]) {
        extract_tar(
            flate2::read::GzDecoder::new(File::open(source).map_err(|e| io_error("Open gzip", e))?),
            output,
        )
    } else if read >= 262 && &magic[257..262] == b"ustar" {
        extract_tar(
            File::open(source).map_err(|e| io_error("Open TAR", e))?,
            output,
        )
    } else if source_name.to_ascii_lowercase().ends_with(".lua") {
        check_html(source)?;
        let name = Path::new(source_name)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| invalid("Invalid Lua filename"))?;
        validated_name(name)?;
        fs::copy(source, output.join(name)).map_err(|e| io_error("Copy Lua file", e))?;
        Ok(())
    } else {
        Err(invalid(
            "Unsupported source; expected ZIP, TAR, TAR.GZ, Lua, or a folder",
        ))
    }
}

fn root_has_mod_files(path: &Path, loader: Loader) -> Result<bool, AppError> {
    if loader == Loader::Lovely && path.join("lovely.toml").is_file() {
        return Ok(true);
    }
    if loader == Loader::Steamodded {
        for entry in fs::read_dir(path).map_err(|e| io_error("Inspect mod layout", e))? {
            let entry = entry.map_err(|e| io_error("Inspect mod layout", e))?;
            if entry
                .file_type()
                .map_err(|e| io_error("Inspect mod layout", e))?
                .is_file()
            {
                let file = entry.path();
                if (file
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("lua"))
                    && lua_has_entrypoint(&file))
                    || (file
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
                        && json_has_entrypoint(&file, path))
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn lua_has_entrypoint(path: &Path) -> bool {
    let Some(text) = read_prefix(path, 128 * 1024) else {
        return false;
    };
    text.lines()
        .next()
        .is_some_and(|line| line.trim_end_matches('\r') == "--- STEAMODDED HEADER")
        || text.contains("SMODS.Mod")
        || text.contains("SMODS.INIT")
}

fn select_payload_root(extracted: &Path, loader: Loader) -> Result<PathBuf, AppError> {
    let mut root = extracted.to_path_buf();
    for _ in 0..6 {
        if root_has_mod_files(&root, loader)? {
            return Ok(root);
        }
        let mut folders = Vec::new();
        for entry in fs::read_dir(&root).map_err(|e| io_error("Inspect mod layout", e))? {
            let entry = entry.map_err(|e| io_error("Inspect mod layout", e))?;
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name == "__macosx" || name == ".git" {
                continue;
            }
            if entry
                .file_type()
                .map_err(|e| io_error("Inspect mod layout", e))?
                .is_dir()
            {
                folders.push(entry.path());
            } else if !matches!(
                name.as_str(),
                "readme.md" | "license" | "license.md" | ".ds_store" | "changelog.md"
            ) {
                return Err(invalid("Package has no single loadable mod root"));
            }
        }
        if folders.len() != 1 {
            return Err(invalid("Package has no single loadable mod root"));
        }
        root = folders.remove(0);
    }
    Err(invalid("Mod is nested too deeply"))
}

fn read_prefix(path: &Path, limit: u64) -> Option<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

// Keep the narrow, string-aware repair used by the BMI parser. Some published
// Steamodded metadata has a comma immediately before `}` or `]`.
fn remove_trailing_json_commas(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut index = 0;
    while index < input.len() {
        let byte = input[index];
        if in_string {
            output.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            in_string = true;
            output.push(byte);
            index += 1;
            continue;
        }
        if byte == b',' {
            let mut next = index + 1;
            while next < input.len() && input[next].is_ascii_whitespace() {
                next += 1;
            }
            if next < input.len() && matches!(input[next], b'}' | b']') {
                index += 1;
                continue;
            }
        }
        output.push(byte);
        index += 1;
    }
    output
}

fn parse_mod_json_value(path: &Path) -> Option<serde_json::Value> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&bytes)
        .or_else(|_| serde_json::from_slice(&remove_trailing_json_commas(&bytes)))
        .ok()
}

fn metadata_from_json(path: &Path) -> Option<InstallMetadata> {
    let value = parse_mod_json_value(path)?;
    let name = value.get("name")?.as_str()?.trim().to_string();
    let id = value.get("id")?.as_str()?.trim().to_string();
    if id.is_empty() || name.is_empty() {
        return None;
    }
    Some(InstallMetadata {
        id,
        name,
        version: value
            .get("version")
            .or_else(|| value.get("version_number"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        dependencies: string_list(value.get("dependencies")),
        conflicts: string_list(value.get("conflicts")),
    })
}

fn json_has_entrypoint(path: &Path, root: &Path) -> bool {
    let Some(value) = parse_mod_json_value(path) else {
        return false;
    };
    if metadata_from_json(path).is_none() {
        return false;
    }
    let Some(main_file) = value.get("main_file").and_then(|v| v.as_str()) else {
        return false;
    };
    let Ok(relative) = safe_relative(main_file) else {
        return false;
    };
    !relative.as_os_str().is_empty()
        && relative
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lua"))
        && root.join(relative).is_file()
}

fn header_list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches(['\'', '"']).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn metadata_from_lua(path: &Path) -> Option<InstallMetadata> {
    let text = read_prefix(path, 128 * 1024)?;
    let mut lines = text.lines();
    if lines.next()?.trim_end_matches('\r') != "--- STEAMODDED HEADER" {
        return None;
    }
    let mut id = String::new();
    let mut name = String::new();
    let mut version = None;
    let mut dependencies = Vec::new();
    let mut conflicts = Vec::new();
    for line in lines.take(80) {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("--- MOD_ID:") {
            id = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("--- MOD_NAME:") {
            name = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("--- VERSION:") {
            version = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("--- DEPENDENCIES:") {
            dependencies = header_list(value);
        } else if let Some(value) = line.strip_prefix("--- CONFLICTS:") {
            conflicts = header_list(value);
        } else if !line.starts_with("---") {
            break;
        }
    }
    let fallback = path.file_stem()?.to_string_lossy().to_string();
    if id.is_empty() {
        id = fallback.clone();
    }
    if name.is_empty() {
        name = fallback;
    }
    Some(InstallMetadata {
        id,
        name,
        version,
        dependencies,
        conflicts,
    })
}

fn read_metadata(
    root: &Path,
    loader: Loader,
    fallback: &str,
    strict: bool,
) -> Result<(Vec<InstallMetadata>, bool), AppError> {
    let mut mods = Vec::new();
    let mut lua_files = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| io_error("Read mod metadata", e))? {
        let entry = entry.map_err(|e| io_error("Read mod metadata", e))?;
        if !entry
            .file_type()
            .map_err(|e| io_error("Read mod metadata", e))?
            .is_file()
        {
            continue;
        }
        let file = entry.path();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if file
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        {
            if let Some(meta) = metadata_from_json(&file) {
                // Arbitrary JSON is metadata only when it points at a real
                // entrypoint. Historical mod.json/metadata.json may omit it.
                if loader == Loader::Steamodded
                    && !matches!(name.as_str(), "mod.json" | "metadata.json")
                    && !json_has_entrypoint(&file, root)
                {
                    continue;
                }
                if mods
                    .iter()
                    .any(|m: &InstallMetadata| m.id.eq_ignore_ascii_case(&meta.id))
                {
                    if strict {
                        return Err(invalid(format!(
                            "Package contains duplicate mod ID {}",
                            meta.id
                        )));
                    }
                    continue;
                }
                mods.push(meta);
            }
        } else if file
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lua"))
        {
            lua_files.push(file);
        }
    }
    for file in lua_files {
        if let Some(meta) = metadata_from_lua(&file) {
            // Both JSON and Lua header entrypoints with the same ID can load;
            // reject this packaging ambiguity rather than hiding a duplicate.
            if mods
                .iter()
                .any(|m: &InstallMetadata| m.id.eq_ignore_ascii_case(&meta.id))
            {
                if strict {
                    return Err(invalid(format!(
                        "Package contains duplicate mod ID {}",
                        meta.id
                    )));
                }
                continue;
            }
            mods.push(meta);
        }
    }
    if mods.is_empty() {
        mods.push(InstallMetadata {
            id: fallback.into(),
            name: fallback.into(),
            version: None,
            dependencies: Vec::new(),
            conflicts: Vec::new(),
        });
        Ok((mods, true))
    } else {
        Ok((mods, false))
    }
}

fn dependency_name(value: &str) -> &str {
    // Both current `Mod (>=1)` and legacy `Mod>=1` forms occur in manifests.
    value
        .split(['(', '>', '<', '=', '~'])
        .next()
        .unwrap_or(value)
        .trim()
}

fn dependency_options(value: &str) -> Vec<&str> {
    value
        .split('|')
        .map(dependency_name)
        .filter(|part| !part.is_empty())
        .collect()
}

fn metadata_matches(left: &InstallMetadata, right: &InstallMetadata) -> bool {
    left.id.eq_ignore_ascii_case(&right.id) || left.name.eq_ignore_ascii_case(&right.name)
}

fn installed_metadata(mods_dir: &Path) -> Result<Vec<(String, InstallMetadata)>, AppError> {
    let mut found = Vec::new();
    if !mods_dir.exists() {
        return Ok(found);
    }
    for entry in fs::read_dir(mods_dir).map_err(|e| io_error("Read installed mods", e))? {
        let entry = entry.map_err(|e| io_error("Read installed mod", e))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let kind = entry
            .file_type()
            .map_err(|e| io_error("Inspect installed mod", e))?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("lua"))
        {
            if let Some(meta) = metadata_from_lua(&entry.path()) {
                found.push((name, meta));
            }
        } else if kind.is_dir() {
            let mut nested = Vec::new();
            scan_installed_folder(&entry.path(), 0, &mut nested)?;
            if nested.is_empty() {
                nested.push(InstallMetadata {
                    id: name.clone(),
                    name: name.clone(),
                    version: None,
                    dependencies: Vec::new(),
                    conflicts: Vec::new(),
                });
            }
            for meta in nested {
                found.push((name.clone(), meta));
            }
        }
    }
    Ok(found)
}

fn scan_installed_folder(
    path: &Path,
    depth: usize,
    found: &mut Vec<InstallMetadata>,
) -> Result<(), AppError> {
    if depth > 4 {
        return Ok(());
    }
    let (direct, unverified) = read_metadata(path, Loader::Steamodded, "", false)?;
    if !unverified {
        found.extend(direct);
        return Ok(());
    }
    for entry in fs::read_dir(path).map_err(|e| io_error("Scan installed mod", e))? {
        let entry = entry.map_err(|e| io_error("Scan installed mod", e))?;
        let kind = entry
            .file_type()
            .map_err(|e| io_error("Scan installed mod", e))?;
        if kind.is_dir()
            && !kind.is_symlink()
            && !entry.file_name().to_string_lossy().starts_with('.')
        {
            scan_installed_folder(&entry.path(), depth + 1, found)?;
        }
    }
    Ok(())
}

fn push_warning(warnings: &mut Vec<InstallWarning>, code: &str, message: String, blocking: bool) {
    if !warnings
        .iter()
        .any(|w| w.code == code && w.message == message)
    {
        warnings.push(InstallWarning {
            code: code.into(),
            message,
            blocking,
        });
    }
}

fn live_warnings(
    mods_dir: &Path,
    destination: &Path,
    mods: &[InstallMetadata],
    unverified: bool,
    loader: Loader,
) -> Result<Vec<InstallWarning>, AppError> {
    let mut warnings = Vec::new();
    if unverified {
        push_warning(
            &mut warnings,
            "unverified_metadata",
            "The package has no readable mod ID; verify the loader and files before installing."
                .into(),
            false,
        );
    }
    let folder = destination.file_name().unwrap().to_string_lossy();
    if mods_dir.exists() {
        for entry in fs::read_dir(mods_dir).map_err(|e| io_error("Read Mods directory", e))? {
            let entry = entry.map_err(|e| io_error("Read Mods directory", e))?;
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(&folder)
            {
                push_warning(
                    &mut warnings,
                    "destination_exists",
                    format!("The install folder '{}' already exists.", folder),
                    true,
                );
            }
        }
    }
    let installed = installed_metadata(mods_dir)?;
    match loader {
        Loader::Steamodded => {
            let framework_present = installed.iter().any(|(folder, meta)| {
                meta.id.eq_ignore_ascii_case("Steamodded")
                    || folder.eq_ignore_ascii_case("smods")
                    || folder.eq_ignore_ascii_case("Steamodded")
                    || folder.to_ascii_lowercase().starts_with("steamodded-smods-")
            });
            if !framework_present {
                push_warning(
                    &mut warnings,
                    "loader_missing",
                    "Steamodded was not found in Mods; this mod may not load.".into(),
                    false,
                );
            }
        }
        Loader::Lovely => {
            push_warning(&mut warnings, "loader_unverified", "Lovely is installed outside Mods; its presence and version have not been verified.".into(), false);
        }
    }
    for incoming in mods {
        for (folder, current) in &installed {
            if metadata_matches(incoming, current) {
                push_warning(
                    &mut warnings,
                    "duplicate_mod",
                    format!(
                        "{} duplicates installed mod {} in {}.",
                        incoming.name, current.name, folder
                    ),
                    true,
                );
            }
            if incoming.conflicts.iter().any(|c| {
                dependency_options(c).iter().any(|name| {
                    name.eq_ignore_ascii_case(&current.id)
                        || name.eq_ignore_ascii_case(&current.name)
                })
            }) {
                push_warning(
                    &mut warnings,
                    "declared_conflict",
                    format!(
                        "{} declares a conflict with installed mod {}.",
                        incoming.name, current.name
                    ),
                    false,
                );
            }
            if current.conflicts.iter().any(|c| {
                dependency_options(c).iter().any(|name| {
                    name.eq_ignore_ascii_case(&incoming.id)
                        || name.eq_ignore_ascii_case(&incoming.name)
                })
            }) {
                push_warning(
                    &mut warnings,
                    "reverse_conflict",
                    format!(
                        "Installed mod {} declares a conflict with {}.",
                        current.name, incoming.name
                    ),
                    false,
                );
            }
        }
        for dependency in &incoming.dependencies {
            let options = dependency_options(dependency);
            let present = options.iter().any(|option| {
                mods.iter().any(|m| {
                    option.eq_ignore_ascii_case(&m.id) || option.eq_ignore_ascii_case(&m.name)
                }) || installed.iter().any(|(_, m)| {
                    option.eq_ignore_ascii_case(&m.id) || option.eq_ignore_ascii_case(&m.name)
                })
            });
            if !present
                && options
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case("Lovely"))
            {
                push_warning(
                    &mut warnings,
                    "dependency_unverified",
                    format!(
                        "{} requires {}; Lovely is installed outside Mods and has not been verified.",
                        incoming.name, dependency
                    ),
                    false,
                );
            } else if !present {
                push_warning(
                    &mut warnings,
                    "missing_dependency",
                    format!(
                        "{} requires {}; no matching installed mod was found.",
                        incoming.name, dependency
                    ),
                    false,
                );
            } else if dependency
                .chars()
                .any(|c| matches!(c, '(' | '>' | '<' | '=' | '~'))
            {
                push_warning(
                    &mut warnings,
                    "version_unverified",
                    format!(
                        "{} requires {}; the version constraint has not been verified.",
                        incoming.name, dependency
                    ),
                    false,
                );
            }
        }
    }
    warnings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    Ok(warnings)
}

async fn download(url: &str, target: &Path) -> Result<String, AppError> {
    let parsed = reqwest::Url::parse(url).map_err(|e| io_error("Invalid download URL", e))?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err(invalid("Only HTTP(S) download URLs are supported"));
    }
    let filename = parsed
        .path_segments()
        .and_then(|mut s| s.next_back())
        .filter(|s| !s.is_empty())
        .unwrap_or("download.zip")
        .to_string();
    let client = crate::http::shared_client();
    let mut response = client
        .get(parsed)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| io_error("Download mod", e))?;
    if !response.status().is_success() {
        return Err(invalid(format!(
            "Download failed with HTTP {}",
            response.status()
        )));
    }
    if response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().contains("text/html"))
    {
        return Err(invalid("Download returned HTML instead of a mod file"));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_SOURCE_BYTES)
    {
        return Err(AppError::ArchiveTooLarge {
            reason: "Download exceeds 512 MiB".into(),
        });
    }
    let mut output = tokio::fs::File::create(target)
        .await
        .map_err(|e| io_error("Create download", e))?;
    use tokio::io::AsyncWriteExt;
    let mut total = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| io_error("Read download", e))?
    {
        total += chunk.len() as u64;
        if total > MAX_SOURCE_BYTES {
            return Err(AppError::ArchiveTooLarge {
                reason: "Download exceeds 512 MiB".into(),
            });
        }
        output
            .write_all(&chunk)
            .await
            .map_err(|e| io_error("Write download", e))?;
    }
    output
        .flush()
        .await
        .map_err(|e| io_error("Flush download", e))?;
    Ok(filename)
}

/// Prepare one downloadable archive, Lua file, or local file/folder without
/// changing the active Mods directory. `folder_name` is an explicit safe folder
/// name chosen by the caller, not a path.
pub async fn prepare_install(
    mods_dir: &Path,
    source: InstallSource,
    folder_name: &str,
    loader: Loader,
) -> Result<PreparedInstall, AppError> {
    validated_name(folder_name)?;
    let (mods_dir, staging) = paths(mods_dir)?;
    fs::create_dir_all(&staging).map_err(|e| io_error("Create staging directory", e))?;
    cleanup_expired_stages(&mods_dir, &staging)?;
    let temp = tempfile::Builder::new()
        .prefix("a-")
        .tempdir_in(&staging)
        .map_err(|e| io_error("Create private staging directory", e))?;
    let extracted = temp.path().join("extracted");
    fs::create_dir(&extracted).map_err(|e| io_error("Create extraction directory", e))?;

    let (source_path, source_name, subtree) = match source {
        InstallSource::Local(path) => {
            let metadata =
                fs::symlink_metadata(&path).map_err(|e| io_error("Inspect local source", e))?;
            if metadata.file_type().is_symlink() {
                return Err(invalid("Local source is a symlink"));
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| invalid("Invalid local source filename"))?
                .to_string();
            (path, name, None)
        }
        InstallSource::Url(url) => {
            let path = temp.path().join("download");
            let name = download(&url, &path).await?;
            (path, name, None)
        }
        InstallSource::UrlSubtree { url, path } => {
            let subtree = safe_relative(&path)?;
            if subtree.as_os_str().is_empty() {
                return Err(invalid("Empty repository subtree"));
            }
            let local = temp.path().join("download");
            let name = download(&url, &local).await?;
            (local, name, Some(subtree))
        }
    };
    unpack_source(&source_path, &source_name, &extracted)?;

    let root = if let Some(subtree) = subtree {
        let mut children = fs::read_dir(&extracted)
            .map_err(|e| io_error("Inspect repository archive", e))?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| io_error("Inspect repository archive", e))?;
        if children.len() != 1 || !children[0].is_dir() {
            return Err(invalid("Repository archive has no single root folder"));
        }
        let repo = children.remove(0);
        let selected = repo.join(subtree);
        if !selected.is_dir() {
            return Err(invalid("Requested repository subtree does not exist"));
        }
        select_payload_root(&selected, loader)?
    } else {
        select_payload_root(&extracted, loader)?
    };
    let payload = temp.path().join("payload");
    fs::rename(&root, &payload).map_err(|e| io_error("Stage mod payload", e))?;
    if extracted.exists() {
        fs::remove_dir_all(&extracted).map_err(|e| io_error("Clean extraction directory", e))?;
    }
    if source_path.starts_with(temp.path()) && source_path.exists() {
        fs::remove_file(source_path).map_err(|e| io_error("Clean downloaded source", e))?;
    }
    let (mods, unverified) = read_metadata(&payload, loader, folder_name, true)?;
    let fingerprint = payload_fingerprint(&payload)?;
    let destination = mods_dir.join(folder_name);
    let warnings = live_warnings(&mods_dir, &destination, &mods, unverified, loader)?;
    let token = temp
        .path()
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid("Invalid staging token"))?
        .to_string();
    validated_token(&token)?;
    let prepared = PreparedInstall {
        token,
        destination,
        warnings,
        mods,
    };
    let manifest = StageManifest {
        prepared: prepared.clone(),
        mods_dir,
        loader,
        unverified,
        payload_fingerprint: fingerprint,
    };
    let manifest_file = File::create_new(temp.path().join("manifest.json"))
        .map_err(|e| io_error("Create staging manifest", e))?;
    serde_json::to_writer(manifest_file, &manifest)
        .map_err(|e| io_error("Write staging manifest", e))?;
    let _ = temp.keep();
    Ok(prepared)
}

fn load_stage(mods_dir: &Path, token: &str) -> Result<(PathBuf, StageManifest), AppError> {
    validated_token(token)?;
    let (mods_dir, staging) = paths(mods_dir)?;
    let stage = staging.join(token);
    let meta = fs::symlink_metadata(&stage).map_err(|e| io_error("Find staged install", e))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(invalid("Staged install is not a real directory"));
    }
    let manifest: StageManifest = serde_json::from_reader(
        File::open(stage.join("manifest.json"))
            .map_err(|e| io_error("Read staging manifest", e))?,
    )
    .map_err(|e| io_error("Parse staging manifest", e))?;
    let payload_kind = fs::symlink_metadata(stage.join("payload"))
        .map_err(|e| io_error("Inspect staged payload", e))?;
    if manifest.mods_dir != mods_dir
        || manifest.prepared.token != token
        || manifest.prepared.destination.parent() != Some(mods_dir.as_path())
        || !payload_kind.is_dir()
        || payload_kind.file_type().is_symlink()
    {
        return Err(invalid("Staged install does not match this Mods directory"));
    }
    Ok((stage, manifest))
}

fn cleanup_expired_stages(mods_dir: &Path, staging: &Path) -> Result<(), AppError> {
    for entry in fs::read_dir(staging).map_err(|e| io_error("Read staging directory", e))? {
        let entry = entry.map_err(|e| io_error("Read staging entry", e))?;
        let token = entry.file_name().to_string_lossy().to_string();
        if validated_token(&token).is_err() {
            continue;
        }
        let meta =
            fs::symlink_metadata(entry.path()).map_err(|e| io_error("Inspect staging entry", e))?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            continue;
        }
        let Ok(age) = SystemTime::now().duration_since(
            meta.modified()
                .map_err(|e| io_error("Read staging age", e))?,
        ) else {
            continue;
        };
        if age > STAGE_TTL && load_stage(mods_dir, &token).is_ok() {
            fs::remove_dir_all(entry.path())
                .map_err(|e| io_error("Remove expired staged install", e))?;
        }
    }
    Ok(())
}

/// Reload the prepared review after a UI refresh or process restart.
pub fn inspect_prepared(mods_dir: &Path, token: &str) -> Result<PreparedInstall, AppError> {
    Ok(load_stage(mods_dir, token)?.1.prepared)
}

#[cfg(target_os = "linux")]
fn rename_without_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in source path"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in destination path"))?;
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
fn rename_without_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in source path"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in destination path"))?;
    let result =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "windows")]
fn rename_without_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // No MOVEFILE_REPLACE_EXISTING or COPY_ALLOWED: destination occupancy and
    // cross-volume staging both fail without touching the existing install.
    let result = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0) };
    if result != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn rename_without_replace(source: &Path, destination: &Path) -> io::Result<()> {
    if fs::symlink_metadata(destination).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination exists",
        ));
    }
    fs::rename(source, destination)
}

/// Commit only the exact warning set reviewed during preparation. A new
/// collision or warning requires preparation again; existing content is never
/// replaced.
pub fn commit_install(
    mods_dir: &Path,
    token: &str,
    accept_warnings: bool,
) -> Result<PathBuf, AppError> {
    let (stage, manifest) = load_stage(mods_dir, token)?;
    if payload_fingerprint(&stage.join("payload"))? != manifest.payload_fingerprint {
        return Err(invalid(
            "Staged mod files changed since review; prepare the install again",
        ));
    }
    let (current_mods, current_unverified) = read_metadata(
        &stage.join("payload"),
        manifest.loader,
        manifest
            .prepared
            .destination
            .file_name()
            .unwrap()
            .to_string_lossy()
            .as_ref(),
        true,
    )?;
    if current_mods != manifest.prepared.mods || current_unverified != manifest.unverified {
        return Err(invalid(
            "Staged mod metadata changed since review; prepare the install again",
        ));
    }
    let current = live_warnings(
        &manifest.mods_dir,
        &manifest.prepared.destination,
        &manifest.prepared.mods,
        manifest.unverified,
        manifest.loader,
    )?;
    if current != manifest.prepared.warnings {
        return Err(invalid(
            "Installed mods changed since review; prepare the install again",
        ));
    }
    if current.iter().any(|warning| warning.blocking) {
        return Err(invalid("Install has a blocking collision"));
    }
    if !accept_warnings && !current.is_empty() {
        return Err(invalid("Install warnings require explicit acceptance"));
    }
    if !manifest.mods_dir.exists() {
        fs::create_dir(&manifest.mods_dir).map_err(|e| io_error("Create Mods directory", e))?;
    }
    rename_without_replace(&stage.join("payload"), &manifest.prepared.destination)
        .map_err(|e| io_error("Commit staged mod without overwrite", e))?;
    if let Err(error) = fs::remove_dir_all(&stage) {
        log::warn!("Installed mod but could not clean staging directory: {error}");
    }
    Ok(manifest.prepared.destination)
}

/// Abandon a staged install without changing active mods.
pub fn cancel_install(mods_dir: &Path, token: &str) -> Result<(), AppError> {
    let (stage, _) = load_stage(mods_dir, token)?;
    fs::remove_dir_all(stage).map_err(|e| io_error("Cancel staged install", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        fs::create_dir(&mods).unwrap();
        (temp, mods)
    }

    fn header(id: &str) -> String {
        format!("--- STEAMODDED HEADER\n--- MOD_NAME: {id}\n--- MOD_ID: {id}\n--- VERSION: 1.0\n")
    }

    #[tokio::test]
    async fn standalone_lua_keeps_its_filename_and_commits_under_one_mod_folder() {
        let (temp, mods) = setup();
        let source = temp.path().join("actual_entry.lua");
        fs::write(&source, header("Example")).unwrap();
        let prepared = prepare_install(
            &mods,
            InstallSource::Local(source),
            "Example",
            Loader::Steamodded,
        )
        .await
        .unwrap();
        assert_eq!(prepared.mods[0].id, "Example");
        assert!(prepared.warnings.iter().any(|w| w.code == "loader_missing"));
        assert!(!prepared.destination.exists());
        assert_eq!(
            inspect_prepared(&mods, &prepared.token).unwrap().mods[0].id,
            "Example"
        );
        let destination = commit_install(&mods, &prepared.token, true).unwrap();
        assert!(destination.join("actual_entry.lua").is_file());
        assert!(!temp.path().join(STAGING_DIR).join(&prepared.token).exists());
    }

    #[tokio::test]
    async fn nested_archive_extracts_mod_root_and_rejects_existing_destination() {
        let (temp, mods) = setup();
        let source = temp.path().join("repo.zip");
        let mut archive = zip::ZipWriter::new(File::create(&source).unwrap());
        archive
            .start_file(
                "repo-main/Mods/Example/main.lua",
                SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(header("Example").as_bytes()).unwrap();
        archive.finish().unwrap();
        fs::create_dir(mods.join("Example")).unwrap();
        let prepared = prepare_install(
            &mods,
            InstallSource::Local(source),
            "Example",
            Loader::Steamodded,
        )
        .await
        .unwrap();
        assert!(
            prepared
                .warnings
                .iter()
                .any(|w| w.code == "destination_exists" && w.blocking)
        );
        assert!(commit_install(&mods, &prepared.token, true).is_err());
        assert!(mods.join("Example").is_dir());
        cancel_install(&mods, &prepared.token).unwrap();
    }

    #[tokio::test]
    async fn warns_for_declared_and_reverse_conflicts_and_missing_dependencies() {
        let (temp, mods) = setup();
        let current = mods.join("OldFolder");
        fs::create_dir(&current).unwrap();
        fs::write(
            current.join("mod.json"),
            r#"{"id":"Old","name":"Old","conflicts":["New (>=1.0)"]}"#,
        )
        .unwrap();
        let incoming = temp.path().join("incoming");
        fs::create_dir(&incoming).unwrap();
        fs::write(
            incoming.join("mod.json"),
            r#"{"id":"New","name":"New","main_file":"main.lua","dependencies":["Missing (>=2.0)"],"conflicts":["Old"]}"#,
        )
        .unwrap();
        fs::write(incoming.join("main.lua"), "return {}\n").unwrap();
        let prepared = prepare_install(
            &mods,
            InstallSource::Local(incoming),
            "NewFolder",
            Loader::Steamodded,
        )
        .await
        .unwrap();
        let codes: HashSet<&str> = prepared.warnings.iter().map(|w| w.code.as_str()).collect();
        assert!(codes.contains("declared_conflict"));
        assert!(codes.contains("reverse_conflict"));
        assert!(codes.contains("missing_dependency"));
        assert!(commit_install(&mods, &prepared.token, false).is_err());
        assert!(commit_install(&mods, &prepared.token, true).is_ok());
    }

    #[tokio::test]
    async fn refuses_changed_warning_set_between_prepare_and_commit() {
        let (temp, mods) = setup();
        let source = temp.path().join("new.lua");
        fs::write(&source, header("New")).unwrap();
        let prepared = prepare_install(
            &mods,
            InstallSource::Local(source),
            "New",
            Loader::Steamodded,
        )
        .await
        .unwrap();
        fs::create_dir(mods.join("DifferentFolder")).unwrap();
        fs::write(
            mods.join("DifferentFolder").join("mod.json"),
            r#"{"id":"New","name":"New"}"#,
        )
        .unwrap();
        assert!(commit_install(&mods, &prepared.token, true).is_err());
        assert!(!mods.join("New").exists());
    }

    #[tokio::test]
    async fn rejects_archive_path_traversal() {
        let (temp, mods) = setup();
        let source = temp.path().join("evil.zip");
        let mut archive = zip::ZipWriter::new(File::create(&source).unwrap());
        archive
            .start_file("../escape.lua", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(header("Evil").as_bytes()).unwrap();
        archive.finish().unwrap();
        assert!(
            prepare_install(
                &mods,
                InstallSource::Local(source),
                "Evil",
                Loader::Steamodded
            )
            .await
            .is_err()
        );
        assert!(!temp.path().join("escape.lua").exists());
    }

    #[tokio::test]
    async fn rejects_html_lua_and_unsupported_lovely_layout() {
        let (temp, mods) = setup();
        let source = temp.path().join("fake.lua");
        fs::write(&source, "<!DOCTYPE html><title>Sign in</title>").unwrap();
        assert!(
            prepare_install(
                &mods,
                InstallSource::Local(source.clone()),
                "Fake",
                Loader::Steamodded
            )
            .await
            .is_err()
        );
        fs::write(&source, header("Real")).unwrap();
        assert!(
            prepare_install(&mods, InstallSource::Local(source), "Real", Loader::Lovely)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn rejects_lua_fragment_without_mod_entrypoint() {
        let (temp, mods) = setup();
        let source = temp.path().join("helper.lua");
        fs::write(&source, "return { useful = true }\n").unwrap();
        assert!(
            prepare_install(
                &mods,
                InstallSource::Local(source),
                "Helper",
                Loader::Steamodded
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn repository_subtree_installs_only_selected_mod() {
        let (_temp, mods) = setup();
        let cursor = io::Cursor::new(Vec::new());
        let mut archive = zip::ZipWriter::new(cursor);
        archive
            .start_file(
                "repo-main/mods/Selected/main.lua",
                SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(header("Selected").as_bytes()).unwrap();
        archive
            .start_file(
                "repo-main/mods/Other/main.lua",
                SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(header("Other").as_bytes()).unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/archive.zip")
            .with_status(200)
            .with_body(bytes)
            .create_async()
            .await;
        let prepared = prepare_install(
            &mods,
            InstallSource::UrlSubtree {
                url: format!("{}/archive.zip", server.url()),
                path: "mods/Selected".into(),
            },
            "Selected",
            Loader::Steamodded,
        )
        .await
        .unwrap();
        mock.assert_async().await;
        let destination = commit_install(&mods, &prepared.token, true).unwrap();
        assert!(destination.join("main.lua").is_file());
        assert!(!destination.join("Other").exists());
    }

    #[tokio::test]
    async fn arbitrary_json_manifest_requires_real_main_file() {
        let (temp, mods) = setup();
        let source = temp.path().join("mod_source");
        fs::create_dir(&source).unwrap();
        fs::write(
            source.join("Cartomancer.json"),
            r#"{"id":"Cartomancer","name":"Cartomancer","main_file":"entry.lua"}"#,
        )
        .unwrap();
        assert!(
            prepare_install(
                &mods,
                InstallSource::Local(source.clone()),
                "Cartomancer",
                Loader::Steamodded
            )
            .await
            .is_err()
        );
        fs::write(source.join("entry.lua"), "SMODS.Mod { id = 'Cartomancer' }").unwrap();
        let prepared = prepare_install(
            &mods,
            InstallSource::Local(source),
            "Cartomancer",
            Loader::Steamodded,
        )
        .await
        .unwrap();
        assert_eq!(prepared.mods.len(), 1);
        assert_eq!(prepared.mods[0].id, "Cartomancer");
        cancel_install(&mods, &prepared.token).unwrap();
    }

    #[tokio::test]
    async fn finds_duplicate_installed_lua_inside_wrapped_directory() {
        let (temp, mods) = setup();
        let wrapped = mods.join("SomeRelease").join("nested");
        fs::create_dir_all(&wrapped).unwrap();
        fs::write(wrapped.join("existing.lua"), header("Repeated")).unwrap();
        let source = temp.path().join("new.lua");
        fs::write(&source, header("Repeated")).unwrap();
        let prepared = prepare_install(
            &mods,
            InstallSource::Local(source),
            "NewFolder",
            Loader::Steamodded,
        )
        .await
        .unwrap();
        assert!(
            prepared
                .warnings
                .iter()
                .any(|w| w.code == "duplicate_mod" && w.blocking)
        );
        cancel_install(&mods, &prepared.token).unwrap();
    }

    #[tokio::test]
    async fn finds_duplicate_installed_standalone_lua_and_legacy_conflict() {
        let (temp, mods) = setup();
        fs::write(mods.join("existing.lua"), "--- STEAMODDED HEADER\n--- MOD_ID: Existing\n--- MOD_NAME: Existing\n--- CONFLICTS: [Incoming>=1.0]\n").unwrap();
        let source = temp.path().join("incoming.lua");
        fs::write(&source, "--- STEAMODDED HEADER\n--- MOD_ID: Incoming\n--- MOD_NAME: Incoming\n--- CONFLICTS: [Existing>=1.0]\n").unwrap();
        let prepared = prepare_install(
            &mods,
            InstallSource::Local(source),
            "Incoming",
            Loader::Steamodded,
        )
        .await
        .unwrap();
        assert!(
            prepared
                .warnings
                .iter()
                .any(|w| w.code == "declared_conflict")
        );
        assert!(
            prepared
                .warnings
                .iter()
                .any(|w| w.code == "reverse_conflict")
        );
        cancel_install(&mods, &prepared.token).unwrap();
    }

    #[tokio::test]
    async fn destination_created_after_review_is_not_replaced() {
        let (temp, mods) = setup();
        let source = temp.path().join("new.lua");
        fs::write(&source, header("New")).unwrap();
        let prepared = prepare_install(
            &mods,
            InstallSource::Local(source),
            "New",
            Loader::Steamodded,
        )
        .await
        .unwrap();
        let destination = mods.join("New");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("keep.txt"), "untouched").unwrap();
        assert!(commit_install(&mods, &prepared.token, true).is_err());
        assert_eq!(
            fs::read_to_string(destination.join("keep.txt")).unwrap(),
            "untouched"
        );
    }

    #[tokio::test]
    async fn changed_staged_content_cannot_commit() {
        let (temp, mods) = setup();
        let source = temp.path().join("new.lua");
        fs::write(&source, header("New")).unwrap();
        let prepared = prepare_install(
            &mods,
            InstallSource::Local(source),
            "New",
            Loader::Steamodded,
        )
        .await
        .unwrap();
        let stage = temp.path().join(STAGING_DIR).join(&prepared.token);
        fs::write(stage.join("payload").join("new.lua"), header("Other")).unwrap();
        assert!(commit_install(&mods, &prepared.token, true).is_err());
        assert!(!mods.join("New").exists());
    }

    #[test]
    fn trailing_comma_repair_does_not_change_json_strings() {
        let input = br#"{"description":"text ,] and escaped \" ,}","items":["keep,]",],}"#;
        let repaired = remove_trailing_json_commas(input);
        let value: serde_json::Value = serde_json::from_slice(&repaired).unwrap();
        assert_eq!(value["description"], "text ,] and escaped \" ,}");
        assert_eq!(value["items"][0], "keep,]");
    }

    #[tokio::test]
    async fn lovely_package_preserves_optional_json_metadata_and_conflicts() {
        let (temp, mods) = setup();
        let current = mods.join("Old");
        fs::create_dir(&current).unwrap();
        fs::write(current.join("mod.json"), r#"{"id":"Old","name":"Old"}"#).unwrap();
        let source = temp.path().join("Cartomancer");
        fs::create_dir(&source).unwrap();
        fs::write(
            source.join("lovely.toml"),
            "[manifest]\nversion = \"1.0\"\n",
        )
        .unwrap();
        fs::write(source.join("Cartomancer.json"), r#"{"id":"Cartomancer","name":"Cartomancer","version":"4.17.8","description":"keep ,] inside string","conflicts":["Old"],}"#).unwrap();
        let prepared = prepare_install(
            &mods,
            InstallSource::Local(source),
            "Cartomancer",
            Loader::Lovely,
        )
        .await
        .unwrap();
        assert_eq!(prepared.mods[0].id, "Cartomancer");
        assert_eq!(prepared.mods[0].version.as_deref(), Some("4.17.8"));
        assert!(
            prepared
                .warnings
                .iter()
                .any(|w| w.code == "declared_conflict")
        );
        assert!(
            !prepared
                .warnings
                .iter()
                .any(|w| w.code == "unverified_metadata")
        );
        cancel_install(&mods, &prepared.token).unwrap();
    }
}
