//! The built-in asset protocol grants its initial webview origin CORS access.
//! Remote source pages must never inherit that local filesystem authority.
use tauri::{
    Manager,
    http::{Method, Request, Response, StatusCode},
};

pub fn response(
    app: &tauri::AppHandle,
    label: &str,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let result = main_asset(app, label, &request);
    match result {
        Ok(response) => response,
        Err(status) => Response::builder().status(status).body(Vec::new()).unwrap(),
    }
}

fn main_asset(
    app: &tauri::AppHandle,
    label: &str,
    request: &Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>, StatusCode> {
    if !may_read_assets(label) {
        return Err(StatusCode::FORBIDDEN);
    }
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return Err(StatusCode::METHOD_NOT_ALLOWED);
    }
    let encoded = request.uri().path().strip_prefix('/').unwrap_or_default();
    let path = urlencoding::decode(encoded)
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .into_owned();
    tauri::path::SafePathBuf::new(path.clone().into()).map_err(|_| StatusCode::FORBIDDEN)?;
    if !app.asset_protocol_scope().is_allowed(&path) {
        return Err(StatusCode::FORBIDDEN);
    }
    let content = std::fs::read(&path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
        std::io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?;
    let mime = tauri_utils::mime_type::MimeType::parse(&content, &path);
    let mut response = Response::builder()
        .header("Content-Type", mime)
        .header("Content-Length", content.len())
        .header("X-Content-Type-Options", "nosniff");
    // Querying a native webview URL in a protocol callback can deadlock.
    // Derive the trusted main origin from app config, never the request header.
    let dev_origin = tauri::is_dev()
        .then(|| app.config().build.dev_url.as_ref())
        .flatten()
        .map(|url| url.origin().ascii_serialization());
    let app_origin = if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };
    response = response.header(
        "Access-Control-Allow-Origin",
        dev_origin.as_deref().unwrap_or(app_origin),
    );
    response
        .body(if request.method() == Method::HEAD {
            Vec::new()
        } else {
            content
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn may_read_assets(label: &str) -> bool {
    label == "main"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_main_webview_has_asset_authority() {
        assert!(may_read_assets("main"));
        for label in ["source-page", "", "main-child", "remote"] {
            assert!(!may_read_assets(label), "{label}");
        }
    }
}
