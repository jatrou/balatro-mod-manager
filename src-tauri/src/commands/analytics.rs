use crate::state::AppState;
use serde_json::Value;

/// Compatibility command for clients that still query the former preference.
/// Analytics are always disabled in this maintained fork.
#[tauri::command]
pub async fn get_analytics_status(_state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(false)
}

/// Compatibility command for older clients. The requested value is ignored.
#[tauri::command]
pub async fn set_analytics_status(
    _state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let _ = enabled;
    Ok(())
}

/// Compatibility command for existing event call sites. No data is collected
/// or sent anywhere.
#[tauri::command]
pub async fn track_event(
    _state: tauri::State<'_, AppState>,
    name: String,
    props: Option<Value>,
) -> Result<(), String> {
    let _ = (name, props);
    Ok(())
}
