//! Turn catalog download links into files or archives the staged installer can inspect.

use std::path::PathBuf;
use std::time::Duration;

use bmm_lib::awesome_installer::InstallSource;
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;

use crate::bmi::BmiClient;

#[derive(Debug, PartialEq, Eq)]
enum GithubLink {
    Repository {
        owner: String,
        repo: String,
    },
    Blob {
        owner: String,
        repo: String,
        tail: Vec<String>,
    },
    Tree {
        owner: String,
        repo: String,
        tail: Vec<String>,
    },
}

#[derive(Deserialize)]
struct Repository {
    default_branch: String,
}

#[derive(Deserialize)]
struct Content {
    #[serde(rename = "type")]
    kind: String,
    download_url: Option<String>,
}

fn github_link(url: &Url) -> Result<Option<GithubLink>, String> {
    if url.host_str() != Some("github.com") && url.host_str() != Some("www.github.com") {
        return Ok(None);
    }
    let parts: Vec<String> = url
        .path_segments()
        .ok_or("Invalid GitHub URL")?
        .filter(|part| !part.is_empty())
        .map(|part| urlencoding::decode(part).map(|value| value.into_owned()))
        .collect::<Result<_, _>>()
        .map_err(|_| "Invalid encoding in GitHub URL")?;
    if parts.len() < 2 || !parts[0].chars().all(github_name_char) {
        return Err("GitHub URL must name an owner and repository".to_string());
    }
    let repo = parts[1].trim_end_matches(".git");
    if repo.is_empty() || !repo.chars().all(github_name_char) {
        return Err("Invalid GitHub repository name".to_string());
    }
    let owner = parts[0].clone();
    let repo = repo.to_string();
    match parts.as_slice() {
        [_, _] => Ok(Some(GithubLink::Repository { owner, repo })),
        [_, _, kind, tail @ ..]
            if (kind == "blob" && tail.len() >= 2) || (kind == "tree" && !tail.is_empty()) =>
        {
            if tail.iter().any(|part| part == "." || part == ".." || part.contains('/') || part.contains('\\')) {
                return Err("Invalid path in GitHub URL".to_string());
            }
            if kind == "blob" {
                Ok(Some(GithubLink::Blob { owner, repo, tail: tail.to_vec() }))
            } else {
                Ok(Some(GithubLink::Tree { owner, repo, tail: tail.to_vec() }))
            }
        }
        // An asset URL is already a download. Other GitHub pages are HTML.
        [_, _, section, kind, ..] if section == "releases" && kind == "download" => Ok(None),
        [_, _, section, latest, kind, ..]
            if section == "releases" && latest == "latest" && kind == "download" => Ok(None),
        [_, _, section, ..] if section == "archive" => Ok(None),
        _ => Err("This GitHub page is not a downloadable mod. Use a repository, file, subfolder, archive, or release asset URL.".to_string()),
    }
}

fn github_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

fn api_url(owner: &str, repo: &str, suffix: &[&str]) -> Result<Url, String> {
    let mut url = Url::parse("https://api.github.com/repos/").expect("static GitHub API URL");
    url.path_segments_mut()
        .map_err(|_| "Invalid GitHub API URL")?
        .pop_if_empty()
        .push(owner)
        .push(repo)
        .extend(suffix);
    Ok(url)
}

fn archive_url(owner: &str, repo: &str, revision: &str) -> Result<String, String> {
    Ok(api_url(owner, repo, &["zipball", revision])?.to_string())
}

async fn github_json<T: for<'de> Deserialize<'de>>(
    client: &Client,
    url: Url,
) -> Result<Option<T>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("GitHub request failed: {e}"))?;
    if matches!(
        response.status(),
        StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY
    ) {
        return Ok(None);
    }
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "GitHub API returned HTTP {status}; check the link or try a local archive"
        ));
    }
    response
        .json()
        .await
        .map(Some)
        .map_err(|e| format!("Invalid GitHub API response: {e}"))
}

async fn resolve_github(client: &Client, link: GithubLink) -> Result<InstallSource, String> {
    match link {
        GithubLink::Repository { owner, repo } => {
            let repo_info: Repository = github_json(client, api_url(&owner, &repo, &[])?)
                .await?
                .ok_or("GitHub repository was not found")?;
            Ok(InstallSource::Url(archive_url(
                &owner,
                &repo,
                &repo_info.default_branch,
            )?))
        }
        GithubLink::Blob { owner, repo, tail } => {
            resolve_github_item(client, &owner, &repo, tail, true).await
        }
        GithubLink::Tree { owner, repo, tail } => {
            resolve_github_item(client, &owner, &repo, tail, false).await
        }
    }
}

async fn resolve_github_item(
    client: &Client,
    owner: &str,
    repo: &str,
    tail: Vec<String>,
    want_file: bool,
) -> Result<InstallSource, String> {
    if !want_file && tail.len() == 1 {
        return Ok(InstallSource::Url(archive_url(owner, repo, &tail[0])?));
    }
    // A branch or tag may contain slashes. Check longest refs first, stopping
    // at the first valid file/folder and capping GitHub API requests.
    for split in (1..tail.len().min(9)).rev() {
        let revision = tail[..split].join("/");
        let path = tail[split..].join("/");
        let mut segments = vec!["contents"];
        let path_parts: Vec<&str> = tail[split..].iter().map(String::as_str).collect();
        segments.extend(path_parts);
        let mut url = api_url(owner, repo, &segments)?;
        url.query_pairs_mut().append_pair("ref", &revision);
        let Some(value): Option<serde_json::Value> = github_json(client, url).await? else {
            continue;
        };
        let content = if value.is_array() {
            Content {
                kind: "dir".to_string(),
                download_url: None,
            }
        } else {
            serde_json::from_value::<Content>(value)
                .map_err(|e| format!("Invalid GitHub file response: {e}"))?
        };
        if (want_file && content.kind == "file") || (!want_file && content.kind == "dir") {
            if want_file {
                let download_url = content
                    .download_url
                    .ok_or("GitHub did not provide a file download URL")?;
                validate_https(&download_url)?;
                return Ok(InstallSource::Url(download_url));
            }
            return Ok(InstallSource::UrlSubtree {
                url: archive_url(owner, repo, &revision)?,
                path,
            });
        }
    }
    Err("GitHub file or folder was not found at any branch or tag in this link".to_string())
}

fn validate_https(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "Download URL is invalid".to_string())?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err("Download URL must use HTTPS".to_string());
    }
    Ok(url)
}

pub fn withdrawn_source_reason(value: &str) -> Option<&'static str> {
    let url = Url::parse(value).ok()?;
    let mut parts = url.path_segments()?;
    let (owner, repo) = match url.host_str()? {
        "github.com" | "www.github.com" | "raw.githubusercontent.com" | "codeload.github.com" => {
            (parts.next()?, parts.next().unwrap_or(""))
        }
        "api.github.com" if parts.next()? == "repos" => (parts.next()?, parts.next().unwrap_or("")),
        _ => return None,
    };
    let owner = urlencoding::decode(owner).ok()?;
    let repo = urlencoding::decode(repo).ok()?;
    if owner.eq_ignore_ascii_case("nh6574") {
        return Some("This author withdrew the mod from BMM; it cannot be installed through BMM");
    }
    if owner.eq_ignore_ascii_case("Balatro-Multiplayer")
        && repo.eq_ignore_ascii_case("BalatroMultiplayer")
    {
        return Some(
            "Balatro Multiplayer requires an unsupported installation layout; follow its source instructions manually",
        );
    }
    None
}

pub async fn resolve_source(
    url: Option<String>,
    local_path: Option<String>,
) -> Result<InstallSource, String> {
    match (
        url.filter(|s| !s.trim().is_empty()),
        local_path.filter(|s| !s.trim().is_empty()),
    ) {
        (Some(_), Some(_)) | (None, None) => {
            Err("Choose exactly one download URL or local file".to_string())
        }
        (None, Some(path)) => Ok(InstallSource::Local(PathBuf::from(path))),
        (Some(raw), None) => {
            let value = if let Some(id) = raw.strip_prefix("bmi://") {
                if id.trim().is_empty() {
                    return Err("BMI download missing mod id".to_string());
                }
                BmiClient::new()?.post_download(id).await?
            } else {
                raw
            };
            if let Some(reason) = withdrawn_source_reason(&value) {
                return Err(reason.to_string());
            }
            let parsed = validate_https(&value)?;
            let client = Client::builder()
                .user_agent(
                    "balatro-mod-manager/0.4 (+https://github.com/jatrou/balatro-mod-manager)",
                )
                .timeout(Duration::from_secs(20))
                .build()
                .map_err(|e| format!("Could not create GitHub client: {e}"))?;
            match github_link(&parsed)? {
                Some(link) => resolve_github(&client, link).await,
                None => Ok(InstallSource::Url(value)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repository_and_rejects_html_pages() {
        let root = Url::parse("https://github.com/example/mod").unwrap();
        assert_eq!(
            github_link(&root).unwrap(),
            Some(GithubLink::Repository {
                owner: "example".into(),
                repo: "mod".into()
            })
        );
        let issue = Url::parse("https://github.com/example/mod/issues/1").unwrap();
        assert!(github_link(&issue).is_err());
        let release =
            Url::parse("https://github.com/example/mod/releases/latest/download/mod.zip").unwrap();
        assert_eq!(github_link(&release).unwrap(), None);
    }

    #[test]
    fn parses_blob_and_tree_with_slash_ref_candidates() {
        let blob = Url::parse("https://github.com/example/mod/blob/feature/new/mod.lua").unwrap();
        assert!(
            matches!(github_link(&blob).unwrap(), Some(GithubLink::Blob { tail, .. }) if tail == ["feature", "new", "mod.lua"])
        );
        let tree = Url::parse("https://github.com/example/mod/tree/main/Mods/Example").unwrap();
        assert!(
            matches!(github_link(&tree).unwrap(), Some(GithubLink::Tree { tail, .. }) if tail == ["main", "Mods", "Example"])
        );
        let branch_root = Url::parse("https://github.com/example/mod/tree/develop").unwrap();
        assert!(
            matches!(github_link(&branch_root).unwrap(), Some(GithubLink::Tree { tail, .. }) if tail == ["develop"])
        );
    }

    #[test]
    fn only_https_sources_are_accepted() {
        assert!(validate_https("http://example.com/mod.zip").is_err());
        assert!(validate_https("file:///tmp/mod.zip").is_err());
        assert!(validate_https("https://example.com/mod.zip").is_ok());
    }

    #[test]
    fn withdrawn_owners_are_blocked_across_github_download_hosts() {
        for url in [
            "https://github.com/nh6574/Example",
            "https://raw.githubusercontent.com/NH6574/Example/main/mod.lua",
            "https://codeload.github.com/nh6574/Example/zip/refs/heads/main",
            "https://api.github.com/repos/nh6574/Example/zipball/main",
            "https://github.com/Balatro-Multiplayer/BalatroMultiplayer",
            "https://codeload.github.com/Balatro-Multiplayer/BalatroMultiplayer/zip/main",
        ] {
            assert!(withdrawn_source_reason(url).is_some(), "{url}");
        }
        assert!(
            withdrawn_source_reason("https://example.com/?url=github.com/nh6574/Example").is_none()
        );
    }
}
