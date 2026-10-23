use std::collections::{HashMap, HashSet};
use std::time::Duration;

use futures::{StreamExt, stream};
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};

use crate::models::{ModDownloads, ModMeta};

/// Community-maintained continuation of the catalog originally used by BMM.
pub const DEFAULT_INDEX_REPOSITORY: &str = "kasimeka/balatro-mod-index";
const DEFAULT_GITHUB_API_URL: &str = "https://api.github.com/";
const DEFAULT_GITHUB_RAW_URL: &str = "https://raw.githubusercontent.com/";
const INDEX_BRANCH: &str = "main";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CATALOG_FETCH_CONCURRENCY: usize = 16;

#[derive(Clone, Copy, Debug)]
pub enum SortMode {
    NameAsc,
    NameDesc,
    UpdatedAsc,
    UpdatedDesc,
    DownloadsAsc,
    DownloadsDesc,
}

impl SortMode {
    pub fn from_optional(value: Option<String>) -> Self {
        match value.as_deref() {
            Some("name_desc") => SortMode::NameDesc,
            Some("updated_asc") => SortMode::UpdatedAsc,
            Some("updated_desc") => SortMode::UpdatedDesc,
            Some("downloads_asc") => SortMode::DownloadsAsc,
            Some("downloads_desc") => SortMode::DownloadsDesc,
            _ => SortMode::NameAsc,
        }
    }
}

#[derive(Clone)]
pub struct BmiClient {
    repository: String,
    api_base: Url,
    raw_base: Url,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BmiMod {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default, alias = "repository")]
    pub repo: Option<String>,
    #[serde(default, alias = "home_page", alias = "homepage_url")]
    pub homepage: Option<String>,
    #[serde(default, alias = "requires_steamodded", alias = "requires-steamodded")]
    pub requires_steamodded: Option<bool>,
    #[serde(default, alias = "requires_talisman", alias = "requires-talisman")]
    pub requires_talisman: Option<bool>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub meta: Option<ModMeta>,
    #[serde(
        default,
        alias = "body",
        alias = "content",
        alias = "markdown",
        alias = "readme",
        alias = "description_md",
        alias = "description_markdown"
    )]
    pub description: Option<String>,
    #[serde(default, alias = "descriptionHtml")]
    pub description_html: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncCache {
    /// Legacy field retained so existing cache files can still be read.
    #[serde(default)]
    pub last_updated_at: Option<String>,
    /// Exact Git tree revision against which this catalog was built.
    #[serde(default)]
    pub source_revision: Option<String>,
    /// Per-mod meta.json blob revisions allow a refresh to fetch only changed mods.
    #[serde(default)]
    pub item_revisions: HashMap<String, String>,
    #[serde(default)]
    pub items: Vec<crate::commands::repo::ArchiveModItem>,
}

#[derive(Debug, Deserialize)]
struct GitTreeResponse {
    sha: String,
    #[serde(default)]
    truncated: bool,
    #[serde(default)]
    tree: Vec<GitTreeEntry>,
}

#[derive(Debug, Deserialize)]
struct GitTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
}

#[derive(Clone, Debug)]
struct CatalogEntry {
    id: String,
    meta_sha: String,
    has_thumbnail: bool,
}

#[derive(Debug)]
enum CatalogItemError {
    Invalid(String),
    Fatal(String),
}

impl BmiClient {
    pub fn new() -> Result<Self, String> {
        let repository = std::env::var("BMM_MOD_INDEX_REPOSITORY")
            .unwrap_or_else(|_| DEFAULT_INDEX_REPOSITORY.to_string());
        validate_repository_name(&repository)?;
        let api_base = parse_base_url(
            "BMM_GITHUB_API_URL",
            std::env::var("BMM_GITHUB_API_URL")
                .unwrap_or_else(|_| DEFAULT_GITHUB_API_URL.to_string()),
        )?;
        let raw_base = parse_base_url(
            "BMM_GITHUB_RAW_URL",
            std::env::var("BMM_GITHUB_RAW_URL")
                .unwrap_or_else(|_| DEFAULT_GITHUB_RAW_URL.to_string()),
        )?;
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(Duration::from_secs(10))
            .user_agent("balatro-mod-manager/0.4 (+https://github.com/jatrou/balatro-mod-manager)")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            repository,
            api_base,
            raw_base,
            client,
        })
    }

    pub fn thumbnail_url(&self, id: &str) -> Result<String, String> {
        self.raw_file_url(INDEX_BRANCH, id, "thumbnail.jpg")
            .map(|url| url.to_string())
    }

    pub fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    pub async fn get_bytes(&self, url: Url) -> Result<Vec<u8>, String> {
        let response = self.send(self.client.get(url), "file").await?;
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|e| format!("Mod index file download failed: {e}"))
    }

    /// Synchronize against one authoritative Git tree snapshot. Removed directories
    /// disappear from the result; a partial or failed refresh is never accepted.
    pub async fn sync_catalog(&self, previous: Option<&SyncCache>) -> Result<SyncCache, String> {
        let tree = self.fetch_catalog_tree().await?;
        let entries = catalog_entries(&tree)?;

        if previous.and_then(|cache| cache.source_revision.as_deref()) == Some(tree.sha.as_str()) {
            let previous_ids: HashSet<&str> = previous
                .into_iter()
                .flat_map(|cache| cache.items.iter().map(|item| item.dir_name.as_str()))
                .collect();
            let live_ids: HashSet<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
            if !previous_ids.is_empty() && previous_ids == live_ids {
                return Ok(previous.expect("checked above").clone());
            }
        }

        let previous_items: HashMap<&str, &crate::commands::repo::ArchiveModItem> = previous
            .into_iter()
            .flat_map(|cache| {
                cache
                    .items
                    .iter()
                    .map(|item| (item.dir_name.as_str(), item))
            })
            .collect();
        let previous_revisions = previous
            .map(|cache| &cache.item_revisions)
            .cloned()
            .unwrap_or_default();

        let mut items = Vec::with_capacity(entries.len());
        let mut changed = Vec::new();
        let mut item_revisions = HashMap::with_capacity(entries.len());
        for entry in entries {
            if previous_revisions.get(&entry.id) == Some(&entry.meta_sha)
                && let Some(item) = previous_items.get(entry.id.as_str())
            {
                let mut reused = (*item).clone();
                reused.has_thumbnail = entry.has_thumbnail;
                reused.image_url = if entry.has_thumbnail {
                    self.thumbnail_url(&entry.id)?
                } else {
                    String::new()
                };
                item_revisions.insert(entry.id.clone(), entry.meta_sha.clone());
                items.push(reused);
            } else {
                changed.push(entry);
            }
        }

        let revision = tree.sha.clone();
        let fetched: Vec<(
            CatalogEntry,
            Result<crate::commands::repo::ArchiveModItem, CatalogItemError>,
        )> = stream::iter(changed)
            .map(|entry| {
                let client = self.clone();
                let revision = revision.clone();
                async move {
                    let result = client.fetch_archive_item(&entry, &revision).await;
                    (entry, result)
                }
            })
            .buffer_unordered(CATALOG_FETCH_CONCURRENCY)
            .collect()
            .await;
        for (entry, result) in fetched {
            match result {
                Ok(item) => {
                    item_revisions.insert(entry.id, entry.meta_sha);
                    items.push(item);
                }
                Err(CatalogItemError::Invalid(error)) => {
                    log::warn!("Ignoring invalid mod-index entry '{}': {error}", entry.id);
                }
                Err(CatalogItemError::Fatal(error)) => return Err(error),
            }
        }

        if items.is_empty() {
            return Err("Mod index contained no valid mods; refusing to replace the cache".into());
        }

        let unique_count = items
            .iter()
            .map(|item| item.dir_name.as_str())
            .collect::<HashSet<_>>()
            .len();
        if unique_count != items.len() {
            return Err("Mod index contains duplicate mod directory IDs".to_string());
        }

        Ok(SyncCache {
            last_updated_at: Some(tree.sha.clone()),
            source_revision: Some(tree.sha),
            item_revisions,
            items,
        })
    }

    pub async fn fetch_downloads_map(
        &self,
        _sort: SortMode,
    ) -> Result<HashMap<String, ModDownloads>, String> {
        // Git index metadata has no download counters. An empty delta leaves the
        // catalog's deterministic zero values alone and costs no network requests.
        Ok(HashMap::new())
    }

    pub async fn fetch_mod(&self, id: &str) -> Result<BmiMod, String> {
        validate_mod_id(id)?;
        let meta: ModMeta = self
            .get_json(
                self.raw_file_url(INDEX_BRANCH, id, "meta.json")?,
                "mod metadata",
            )
            .await?;
        let description = self
            .get_optional_text(self.raw_file_url(INDEX_BRANCH, id, "description.md")?)
            .await?;
        Ok(BmiMod {
            summary: None,
            repo: Some(meta.repo.clone()),
            homepage: None,
            requires_steamodded: Some(meta.requires_steamodded),
            requires_talisman: Some(meta.requires_talisman),
            dependencies: Vec::new(),
            meta: Some(meta),
            description,
            description_html: None,
        })
    }

    pub async fn post_download(&self, id: &str) -> Result<String, String> {
        validate_mod_id(id)?;
        // Check the authoritative tree at install time. This prevents an old raw-CDN
        // response or a stale UI cache from resurrecting a removed entry.
        let tree = self.fetch_catalog_tree().await?;
        if !catalog_entries(&tree)?.iter().any(|entry| entry.id == id) {
            return Err(format!("Mod index entry '{id}' was removed"));
        }
        let meta: ModMeta = self
            .get_json(
                self.raw_file_url(&tree.sha, id, "meta.json")?,
                "mod metadata",
            )
            .await?;
        meta.download_url
            .filter(|url| !url.trim().is_empty())
            .ok_or_else(|| format!("Mod index entry '{id}' has no download URL"))
    }

    async fn fetch_archive_item(
        &self,
        entry: &CatalogEntry,
        revision: &str,
    ) -> Result<crate::commands::repo::ArchiveModItem, CatalogItemError> {
        let mut meta: ModMeta = self
            .get_catalog_metadata(
                self.raw_file_url(revision, &entry.id, "meta.json")
                    .map_err(CatalogItemError::Fatal)?,
            )
            .await?;
        if meta.title.trim().is_empty() {
            return Err(CatalogItemError::Invalid(
                "entry has an empty title".to_string(),
            ));
        }
        if meta.download_url.as_deref().unwrap_or("").trim().is_empty() {
            return Err(CatalogItemError::Invalid(
                "entry has no download URL".to_string(),
            ));
        }
        meta.download_url = Some(format!("bmi://{}", entry.id));
        meta.downloads = Some(ModDownloads::default());
        Ok(crate::commands::repo::ArchiveModItem {
            dir_name: entry.id.clone(),
            meta,
            description: String::new(),
            image_url: if entry.has_thumbnail {
                self.thumbnail_url(&entry.id)
                    .map_err(CatalogItemError::Fatal)?
            } else {
                String::new()
            },
            has_thumbnail: entry.has_thumbnail,
        })
    }

    async fn get_catalog_metadata(&self, url: Url) -> Result<ModMeta, CatalogItemError> {
        let response = self
            .send(self.client.get(url.clone()), "mod metadata")
            .await
            .map_err(CatalogItemError::Fatal)?;
        let bytes = response.bytes().await.map_err(|error| {
            CatalogItemError::Fatal(format!(
                "Mod index mod metadata download failed for {url}: {error}"
            ))
        })?;
        parse_json_with_trailing_commas(&bytes).map_err(|error| {
            CatalogItemError::Invalid(format!("invalid metadata at {url}: {error}"))
        })
    }

    async fn fetch_catalog_tree(&self) -> Result<GitTreeResponse, String> {
        let tree: GitTreeResponse = self
            .get_json(
                self.api_url(&format!("git/trees/{INDEX_BRANCH}"), true)?,
                "catalog tree",
            )
            .await?;
        if tree.truncated {
            return Err(
                "GitHub returned a truncated mod-index tree; refusing a partial catalog".into(),
            );
        }
        Ok(tree)
    }

    fn api_url(&self, suffix: &str, recursive: bool) -> Result<Url, String> {
        let mut url = self
            .api_base
            .join(&format!("repos/{}/{suffix}", self.repository))
            .map_err(|e| e.to_string())?;
        if recursive {
            url.query_pairs_mut().append_pair("recursive", "1");
        }
        Ok(url)
    }

    fn raw_file_url(&self, revision: &str, id: &str, file: &str) -> Result<Url, String> {
        validate_mod_id(id)?;
        let mut url = self.raw_base.clone();
        url.path_segments_mut()
            .map_err(|_| "Invalid GitHub raw base URL".to_string())?
            .extend(self.repository.split('/'))
            .push(revision)
            .push("mods")
            .push(id)
            .push(file);
        Ok(url)
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        url: Url,
        context: &str,
    ) -> Result<T, String> {
        let response = self.send(self.client.get(url.clone()), context).await?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Mod index {context} download failed: {e}"))?;
        parse_json_with_trailing_commas::<T>(&bytes)
            .map_err(|e| format!("Invalid {context} from mod index at {url}: {e}"))
    }

    async fn get_optional_text(&self, url: Url) -> Result<Option<String>, String> {
        let response = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(|e| format!("Mod index description request failed for {url}: {e}"))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response
            .error_for_status()
            .map_err(|e| format!("Mod index description request failed for {url}: {e}"))?
            .text()
            .await
            .map(Some)
            .map_err(|e| format!("Mod index description download failed: {e}"))
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
        context: &str,
    ) -> Result<reqwest::Response, String> {
        let response = request
            .send()
            .await
            .map_err(|e| format!("Mod index {context} request failed: {e}"))?;
        let status = response.status();
        if status == StatusCode::FORBIDDEN
            && response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|value| value.to_str().ok())
                == Some("0")
        {
            return Err("GitHub mod-index rate limit reached; try refreshing later".into());
        }
        response
            .error_for_status()
            .map_err(|e| format!("Mod index {context} request failed ({status}): {e}"))
    }
}

fn catalog_entries(tree: &GitTreeResponse) -> Result<Vec<CatalogEntry>, String> {
    let thumbnail_ids: HashSet<&str> = tree
        .tree
        .iter()
        .filter_map(|entry| path_mod_id(&entry.path, "thumbnail.jpg"))
        .collect();
    let mut entries: Vec<CatalogEntry> = tree
        .tree
        .iter()
        .filter(|entry| entry.kind == "blob")
        .filter_map(|entry| {
            let id = path_mod_id(&entry.path, "meta.json")?;
            Some(CatalogEntry {
                id: id.to_string(),
                meta_sha: entry.sha.clone(),
                has_thumbnail: thumbnail_ids.contains(id),
            })
        })
        .collect();
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    if entries.is_empty() {
        return Err("Mod index tree contained no mods; refusing to replace the cache".into());
    }
    Ok(entries)
}

fn path_mod_id<'a>(path: &'a str, filename: &str) -> Option<&'a str> {
    let mut parts = path.split('/');
    if parts.next()? != "mods" {
        return None;
    }
    let id = parts.next()?;
    if id.is_empty() || parts.next()? != filename || parts.next().is_some() {
        return None;
    }
    Some(id)
}

fn validate_repository_name(repository: &str) -> Result<(), String> {
    let parts: Vec<&str> = repository.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || !part
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        })
    {
        return Err("BMM_MOD_INDEX_REPOSITORY must be in owner/repository form".into());
    }
    Ok(())
}

fn validate_mod_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.contains('/')
        || id.contains('\\')
        || id.chars().any(char::is_control)
    {
        return Err("Invalid mod index ID".into());
    }
    Ok(())
}

fn parse_base_url(name: &str, raw: String) -> Result<Url, String> {
    Url::parse(&format!("{}/", raw.trim_end_matches('/')))
        .map_err(|e| format!("Invalid {name}: {e}"))
}

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

fn parse_json_with_trailing_commas<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
) -> Result<T, serde_json::Error> {
    serde_json::from_slice::<T>(bytes).or_else(|original| {
        // A few inherited index entries contain a harmless trailing comma.
        // Accept that narrow defect while keeping all other JSON validation.
        let repaired = remove_trailing_json_commas(bytes);
        serde_json::from_slice::<T>(&repaired).map_err(|_| original)
    })
}

pub fn derive_requires(item: &BmiMod) -> (bool, bool) {
    let mut steamodded = item.requires_steamodded.unwrap_or(false);
    let mut talisman = item.requires_talisman.unwrap_or(false);
    if let Some(meta) = &item.meta {
        steamodded |= meta.requires_steamodded;
        talisman |= meta.requires_talisman;
    }
    for dependency in &item.dependencies {
        let dependency = dependency.to_lowercase();
        steamodded |= dependency.contains("steamodded");
        talisman |= dependency.contains("talisman");
    }
    (steamodded, talisman)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(paths: &[(&str, &str)]) -> GitTreeResponse {
        GitTreeResponse {
            sha: "tree-sha".to_string(),
            truncated: false,
            tree: paths
                .iter()
                .map(|(path, sha)| GitTreeEntry {
                    path: (*path).to_string(),
                    kind: "blob".to_string(),
                    sha: (*sha).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn catalog_entries_only_accept_exact_mod_file_layout() {
        let entries = catalog_entries(&tree(&[
            ("mods/Author@One/meta.json", "meta-one"),
            ("mods/Author@One/thumbnail.jpg", "thumb-one"),
            ("mods/Author@Two/meta.json", "meta-two"),
            ("mods/Author@Two/subdir/meta.json", "nested"),
            ("other/Author@Three/meta.json", "other"),
        ]))
        .expect("catalog");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "Author@One");
        assert!(entries[0].has_thumbnail);
        assert_eq!(entries[1].id, "Author@Two");
        assert!(!entries[1].has_thumbnail);
    }

    #[test]
    fn catalog_entries_reject_empty_catalog() {
        assert!(catalog_entries(&tree(&[("README.md", "sha")])).is_err());
    }

    #[test]
    fn validates_repository_and_mod_ids() {
        assert!(validate_repository_name("kasimeka/balatro-mod-index").is_ok());
        assert!(validate_repository_name("missing-slash").is_err());
        assert!(validate_repository_name("owner/repo/extra").is_err());
        assert!(validate_mod_id("Author@Mod Name").is_ok());
        assert!(validate_mod_id("../escape").is_err());
        assert!(validate_mod_id("folder\\escape").is_err());
    }

    #[test]
    fn repairs_only_trailing_json_commas() {
        let input = br#"{"url":"https://example.test/a,b", "items":[1,2,],}"#;
        let repaired = remove_trailing_json_commas(input);
        let parsed: serde_json::Value = serde_json::from_slice(&repaired).expect("valid JSON");
        assert_eq!(parsed["url"], "https://example.test/a,b");
        assert_eq!(parsed["items"], serde_json::json!([1, 2]));

        let unterminated = br#"{"downloadURL":"https://example.test/mod.zip}"#;
        assert!(parse_json_with_trailing_commas::<serde_json::Value>(unterminated).is_err());
    }

    #[tokio::test]
    #[ignore = "live GitHub catalog integration test"]
    async fn live_catalog_is_complete_and_honors_known_removals() {
        let cache = BmiClient::new()
            .expect("client")
            .sync_catalog(None)
            .await
            .expect("live catalog");
        assert!(cache.items.len() >= 400);
        let ids: HashSet<&str> = cache
            .items
            .iter()
            .map(|item| item.dir_name.as_str())
            .collect();
        for removed in [
            "Virtualized@Multiplayer",
            "MathIsFun0@Cryptid",
            "MathIsFun0@Talisman",
            "lordruby@Entropy",
            "lordruby@Overflow",
        ] {
            assert!(!ids.contains(removed), "removed catalog entry {removed}");
        }

        // A previously cached entry that no longer exists upstream must not be
        // reused simply because the rest of the tree revision is unchanged.
        let mut stale = cache.clone();
        let mut removed_item = stale.items[0].clone();
        removed_item.dir_name = "Virtualized@Multiplayer".to_string();
        stale.items.push(removed_item);
        stale.item_revisions.insert(
            "Virtualized@Multiplayer".to_string(),
            "retired-revision".to_string(),
        );
        let refreshed = BmiClient::new()
            .expect("client")
            .sync_catalog(Some(&stale))
            .await
            .expect("refreshed live catalog");
        assert!(
            !refreshed
                .items
                .iter()
                .any(|item| item.dir_name == "Virtualized@Multiplayer")
        );
    }
}
