//! Native source pages are untrusted: no remote capability, no app navigation,
//! no local asset protocol, and no OS URL handler fallback.
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{
    Manager, Webview, WebviewUrl,
    webview::{DownloadEvent, NewWindowResponse, PageLoadEvent, WebviewBuilder},
};

pub const SOURCE_LABEL: &str = "source-page";

#[derive(Default)]
pub struct SourceBrowserState {
    loading: Mutex<Option<Arc<AtomicBool>>>,
    download: Arc<Mutex<Option<String>>>,
}

#[derive(Clone, Copy, Deserialize)]
pub struct BrowserBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl BrowserBounds {
    fn validate(self) -> Result<Self, String> {
        if [self.x, self.y, self.width, self.height]
            .iter()
            .any(|v| !v.is_finite())
            || self.x < 0.0
            || self.y < 0.0
            || self.width <= 0.0
            || self.height <= 0.0
        {
            return Err("Invalid source browser bounds".into());
        }
        Ok(self)
    }

    fn rect(self, caller: &Webview) -> Result<tauri::Rect, String> {
        self.validate()?;
        let window = caller.window();
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let size = window
            .inner_size()
            .map_err(|e| e.to_string())?
            .to_logical::<f64>(scale);
        if self.x >= size.width || self.y >= size.height {
            return Err("Source browser is outside the app window".into());
        }
        Ok(tauri::Rect {
            position: tauri::LogicalPosition::new(self.x, self.y).into(),
            size: tauri::LogicalSize::new(
                self.width.min(size.width - self.x),
                self.height.min(size.height - self.y),
            )
            .into(),
        })
    }
}

#[derive(Serialize)]
pub struct BrowserStatus {
    url: String,
    loading: bool,
    download: Option<String>,
}

fn download_path(directory: &std::path::Path, suggested: &std::path::Path) -> std::path::PathBuf {
    let name = suggested
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or(std::ffi::OsStr::new("download"));
    let path = directory.join(name);
    if !path.exists() {
        return path;
    }
    let stem = path.file_stem().unwrap_or(name).to_string_lossy();
    let extension = path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    for index in 1.. {
        let candidate = directory.join(format!("{stem} ({index}){extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn ensure_main(caller: &Webview) -> Result<(), String> {
    if caller.label() != "main" {
        return Err("Only the main app can control source pages".into());
    }
    Ok(())
}

fn validate_url(value: &str) -> Result<tauri::Url, String> {
    let url = tauri::Url::parse(value.trim()).map_err(|_| "Invalid source URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Only HTTP or HTTPS source URLs without credentials are allowed".into());
    }
    let host = url.host_str().unwrap_or_default().trim_end_matches('.');
    let loopback = host
        .trim_matches(['[', ']'])
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback() || ip.is_unspecified());
    if host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
        || loopback
    {
        return Err("App and localhost pages cannot open in the source browser".into());
    }
    Ok(url)
}

fn allowed_navigation(url: &tauri::Url, app: &tauri::AppHandle) -> bool {
    validate_url(url.as_str()).is_ok()
        && !app
            .config()
            .build
            .dev_url
            .as_ref()
            .is_some_and(|dev| dev.origin() == url.origin())
}

#[tauri::command]
pub async fn open_source_browser(
    caller: Webview,
    url: String,
    bounds: BrowserBounds,
) -> Result<BrowserStatus, String> {
    ensure_main(&caller)?;
    let url = validate_url(&url)?;
    if !allowed_navigation(&url, caller.app_handle()) {
        return Err("App pages cannot open in the source browser".into());
    }
    let rect = bounds.rect(&caller)?;
    let app = caller.app_handle();
    if let Some(source) = app.get_webview(SOURCE_LABEL) {
        source.set_bounds(rect).map_err(|e| e.to_string())?;
        source.navigate(url.clone()).map_err(|e| e.to_string())?;
    } else {
        let loading = Arc::new(AtomicBool::new(true));
        let page_loading = loading.clone();
        let navigation_app = app.clone();
        let new_window_app = app.clone();
        let download_status = app.state::<SourceBrowserState>().download.clone();
        let builder = WebviewBuilder::new(SOURCE_LABEL, WebviewUrl::External(url.clone()))
            .incognito(true)
            .disable_drag_drop_handler()
            .on_web_resource_request(|_, response| {
                // This callback covers the bundled tauri:// protocol. Asset
                // requests are denied separately by the global protocol guard.
                *response = tauri::http::Response::builder()
                    .status(403).body(std::borrow::Cow::Owned(Vec::new())).unwrap();
            })
            .initialization_script(r#"
                document.addEventListener('click', (event) => {
                    const anchor = event.target instanceof Element ? event.target.closest('a[href]') : null;
                    if (event.defaultPrevented || event.button !== 0 || !anchor || !anchor.target || anchor.target === '_self') return;
                    const url = new URL(anchor.href, location.href);
                    if (url.protocol === 'http:' || url.protocol === 'https:') {
                        event.preventDefault();
                        location.assign(url.href);
                    }
                });
            "#)
            .on_navigation(move |url| allowed_navigation(url, &navigation_app))
            .on_page_load(move |_, payload| {
                page_loading.store(matches!(payload.event(), PageLoadEvent::Started), Ordering::Relaxed);
            })
            .on_download(move |_, event| {
                let message = match event {
                    DownloadEvent::Requested { url, destination } => {
                        if validate_url(url.as_str()).is_err() { return false; }
                        let Some(directory) = dirs::download_dir() else { return false; };
                        if std::fs::create_dir_all(&directory).is_err() { return false; }
                        *destination = download_path(&directory, destination);
                        Some(format!("Downloading {} to Downloads…", destination.file_name().unwrap_or_default().to_string_lossy()))
                    }
                    DownloadEvent::Finished { path, success, .. } => {
                        Some(if success {
                            format!("Saved {} to Downloads", path.as_ref().and_then(|p| p.file_name()).unwrap_or_default().to_string_lossy())
                        } else { "Download failed. Try the download link again.".into() })
                    }
                    _ => None,
                };
                if let Ok(mut status) = download_status.lock() { *status = message; }
                true
            })
            .on_new_window(move |url, _| {
                // Consume target=_blank/window.open in this same isolated view.
                // Navigation is dispatched off the native event callback thread.
                if allowed_navigation(&url, &new_window_app) {
                    let app = new_window_app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Some(source) = app.get_webview(SOURCE_LABEL) {
                            let _ = source.navigate(url);
                        }
                    });
                }
                NewWindowResponse::Deny
            });
        caller
            .window()
            .add_child(builder, rect.position, rect.size)
            .map_err(|e| e.to_string())?;
        *app.state::<SourceBrowserState>()
            .loading
            .lock()
            .map_err(|e| e.to_string())? = Some(loading);
    }
    Ok(BrowserStatus {
        url: url.to_string(),
        loading: true,
        download: None,
    })
}

#[tauri::command]
pub async fn resize_source_browser(caller: Webview, bounds: BrowserBounds) -> Result<(), String> {
    ensure_main(&caller)?;
    if let Some(source) = caller.app_handle().get_webview(SOURCE_LABEL) {
        source
            .set_bounds(bounds.rect(&caller)?)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn source_browser_action(caller: Webview, action: String) -> Result<(), String> {
    ensure_main(&caller)?;
    let source = caller
        .app_handle()
        .get_webview(SOURCE_LABEL)
        .ok_or("No source page is open")?;
    match action.as_str() {
        "back" => source.eval("window.history.back()"),
        "forward" => source.eval("window.history.forward()"),
        "reload" => source.reload(),
        _ => return Err("Unknown source browser action".into()),
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn source_browser_status(caller: Webview) -> Result<Option<BrowserStatus>, String> {
    ensure_main(&caller)?;
    let app = caller.app_handle();
    let Some(source) = app.get_webview(SOURCE_LABEL) else {
        return Ok(None);
    };
    let loading = app
        .state::<SourceBrowserState>()
        .loading
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed));
    Ok(Some(BrowserStatus {
        url: source.url().map_err(|e| e.to_string())?.to_string(),
        loading,
        download: app
            .state::<SourceBrowserState>()
            .download
            .lock()
            .map_err(|e| e.to_string())?
            .clone(),
    }))
}

#[tauri::command]
pub async fn close_source_browser(caller: Webview) -> Result<(), String> {
    ensure_main(&caller)?;
    if let Some(source) = caller.app_handle().get_webview(SOURCE_LABEL) {
        source.close().map_err(|e| e.to_string())?;
    }
    *caller
        .app_handle()
        .state::<SourceBrowserState>()
        .loading
        .lock()
        .map_err(|e| e.to_string())? = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_source_urls() {
        for url in [
            "https://github.com/user/repo?tab=readme#install",
            "https://discord.com/invite/example",
            " http://example.org/mod ",
        ] {
            assert!(validate_url(url).is_ok(), "{url}");
        }
    }

    #[test]
    fn rejects_app_file_script_and_localhost_urls() {
        for url in [
            "",
            "relative",
            "https://",
            "javascript:alert(1)",
            "file:///C:/secret",
            "asset://localhost/C:/secret",
            "tauri://localhost",
            "http://asset.localhost/C:/secret",
            "http://ipc.localhost",
            "https://LOCALHOST.:1420",
            "http://127.1:1420",
            "http://0.0.0.0",
            "http://[::1]:1420",
            "https://user:password@example.org",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn rejects_nonfinite_or_inverted_bounds() {
        let valid = BrowserBounds {
            x: 0.0,
            y: 72.0,
            width: 1035.0,
            height: 678.0,
        };
        assert!(valid.validate().is_ok());
        for invalid in [
            BrowserBounds { x: -1.0, ..valid },
            BrowserBounds {
                width: 0.0,
                ..valid
            },
            BrowserBounds {
                height: f64::INFINITY,
                ..valid
            },
            BrowserBounds {
                y: f64::NAN,
                ..valid
            },
        ] {
            assert!(invalid.validate().is_err());
        }
    }

    #[test]
    fn downloads_use_only_filename_and_preserve_existing_files() {
        let directory = tempfile::tempdir().unwrap();
        let suggested = std::path::Path::new("outside").join("mod.zip");
        assert_eq!(
            download_path(directory.path(), &suggested),
            directory.path().join("mod.zip")
        );
        std::fs::write(directory.path().join("mod.zip"), "existing").unwrap();
        std::fs::write(directory.path().join("mod (1).zip"), "existing").unwrap();
        assert_eq!(
            download_path(directory.path(), &suggested),
            directory.path().join("mod (2).zip")
        );
    }
}
