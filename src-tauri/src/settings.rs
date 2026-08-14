//! Settings persistence (Epic 2.3 / 3.1).
//!
//! The single settings store: today it holds the global treatment (D4).
//! Rules set now and kept: settings live on the Rust side in the app config
//! dir (never `localStorage`, never in the webview), and the active treatment
//! is applied before first paint via the window initialization script.

use quill_store::types::AppSettings;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const VALID_THEMES: [&str; 2] = ["hairline", "banded"];

// Pane width clamps (Epic 4.2). Keep in sync with SIDEBAR_MIN/MAX and
// LIST_MIN/MAX in src/lib/panes.ts — the frontend clamps live during a drag,
// this clamps whatever a settings file (or a caller) tries to persist.
const SIDEBAR_WIDTH_MIN: u32 = 180;
const SIDEBAR_WIDTH_MAX: u32 = 420;
const LIST_WIDTH_MIN: u32 = 300;
const LIST_WIDTH_MAX: u32 = 560;

fn settings_path(app: &AppHandle) -> Result<PathBuf, tauri::Error> {
    Ok(app.path().app_config_dir()?.join("settings.json"))
}

/// Missing or corrupt settings fall back silently to [`AppSettings::default`]
/// (Hairline).
fn load(app: &AppHandle) -> AppSettings {
    let Ok(path) = settings_path(app) else {
        return AppSettings::default();
    };
    let Ok(contents) = std::fs::read_to_string(path) else {
        return AppSettings::default();
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

fn save(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> AppSettings {
    load(&app)
}

#[tauri::command]
pub fn set_settings(app: AppHandle, mut settings: AppSettings) -> Result<(), String> {
    if !VALID_THEMES.contains(&settings.theme.as_str()) {
        return Err(format!("unknown theme: {}", settings.theme));
    }
    settings.sidebar_width = settings
        .sidebar_width
        .map(|w| w.clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX));
    settings.list_width = settings
        .list_width
        .map(|w| w.clamp(LIST_WIDTH_MIN, LIST_WIDTH_MAX));
    save(&app, &settings)
}

/// Initialization script that stamps the saved treatment on the root element
/// before first paint — the mechanism behind "no flash of the wrong theme"
/// (Epic 2.3).
pub fn theme_init_script(app: &AppHandle) -> String {
    format!(
        "document.documentElement.dataset.theme = '{}';",
        load(app).theme
    )
}
