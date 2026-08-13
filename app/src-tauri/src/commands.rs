//! Tauri command surface (docs/05 §2): thin wrappers mapping GUI calls to
//! the core's job API and the host's [`AppState`]. All logic lives in
//! `state.rs` / `squash-core`; these functions only parse arguments and
//! attach progress forwarders.

use crate::state::{spawn_forwarder, AppState, JobEntryDto, TauriSink};
use serde::Serialize;
use squash_core::format::Format;
use squash_core::presets::Preset;
use squash_core::store::Settings;
use squash_core::Job;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, State};

type SharedState = Arc<AppState>;

fn attach(app: &AppHandle, state: &SharedState, id: &str) {
    spawn_forwarder(
        Arc::clone(state),
        Arc::new(TauriSink(app.clone())),
        id.to_string(),
    );
}

fn entry_for(state: &SharedState, id: &str) -> Result<JobEntryDto, String> {
    state
        .snapshot()
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| "job vanished after submit".to_string())
}

/// Submit a compress job (F2/F4). `format` must be create-capable
/// (docs/06 §2); `preset` is a builtin id (`builtin:fast|balanced|max`).
#[tauri::command]
pub fn submit_compress(
    app: AppHandle,
    state: State<'_, SharedState>,
    inputs: Vec<String>,
    destination: String,
    format: String,
    preset: String,
) -> Result<JobEntryDto, String> {
    let format: Format = format
        .parse()
        .map_err(|_| format!("unknown format {format:?}"))?;
    if !format.can_create() {
        return Err(format!("format {} cannot be created", format.name()));
    }
    let preset = Preset::from_id(&preset).ok_or_else(|| format!("unknown preset {preset:?}"))?;
    if inputs.is_empty() {
        return Err("no inputs".to_string());
    }
    let job = Job::compress(
        inputs.into_iter().map(PathBuf::from).collect(),
        PathBuf::from(destination),
        format,
        preset,
    );
    let id = state.submit(job);
    attach(&app, &state, &id);
    entry_for(&state, &id)
}

/// Submit an extract job (F3). The core applies the single-root-vs-loose
/// layout rule and the zip-slip sanitizer (docs/03 F3/F7).
#[tauri::command]
pub fn submit_extract(
    app: AppHandle,
    state: State<'_, SharedState>,
    archive: String,
    destination: String,
    format: String,
) -> Result<JobEntryDto, String> {
    let format: Format = format
        .parse()
        .map_err(|_| format!("unknown format {format:?}"))?;
    let job = Job::extract(
        vec![PathBuf::from(archive)],
        PathBuf::from(destination),
        format,
    );
    let id = state.submit(job);
    attach(&app, &state, &id);
    entry_for(&state, &id)
}

/// Cooperative cancel (S4 row action).
#[tauri::command]
pub fn cancel_job(state: State<'_, SharedState>, id: String) -> Result<(), String> {
    if state.cancel(&id) {
        Ok(())
    } else {
        Err("job is not running".to_string())
    }
}

/// Remove a terminal job from S4 ("Dismiss").
#[tauri::command]
pub fn dismiss_job(state: State<'_, SharedState>, id: String) -> Result<(), String> {
    if state.dismiss(&id) {
        Ok(())
    } else {
        Err("job is not finished".to_string())
    }
}

/// Re-submit a failed/cancelled job ("Retry", docs/03 F7).
#[tauri::command]
pub fn retry_job(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> Result<JobEntryDto, String> {
    let new_id = state.retry(&id).ok_or_else(|| "unknown job".to_string())?;
    attach(&app, &state, &new_id);
    entry_for(&state, &new_id)
}

/// Full S4 snapshot — called once on launch (also covers restored jobs).
#[tauri::command]
pub fn list_queue(state: State<'_, SharedState>) -> Vec<JobEntryDto> {
    state.snapshot()
}

/// Settings plus writability (docs/06 §4: newer-version files are read-only;
/// the GUI shows a non-blocking banner).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResponse {
    pub settings: Settings,
    pub writable: bool,
    pub warning: Option<String>,
}

#[tauri::command]
pub fn get_settings(state: State<'_, SharedState>) -> SettingsResponse {
    let (settings, writable, warning) = state.settings_snapshot();
    SettingsResponse {
        settings,
        writable,
        warning,
    }
}

#[tauri::command]
pub fn set_settings(state: State<'_, SharedState>, settings: Settings) -> Result<(), String> {
    state.set_settings(settings).map_err(|e| e.to_string())
}

/// One dropped path, classified (docs/03 F5): archives route to S3, the
/// rest to S2.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveRef {
    pub path: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemRef {
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedPaths {
    pub archives: Vec<ArchiveRef>,
    pub items: Vec<ItemRef>,
    /// Total bytes across all dropped paths (drives S2's summary line).
    pub total_bytes: Option<u64>,
}

/// Split a drop into archives vs compressible items by extension detection
/// (docs/03 F5). Directories are never archives.
#[tauri::command]
pub fn classify_paths(state: State<'_, SharedState>, paths: Vec<String>) -> ClassifiedPaths {
    let registry = state.engine().registry();
    let mut archives = Vec::new();
    let mut items = Vec::new();
    for p in &paths {
        let path = PathBuf::from(p);
        if !path.is_dir() {
            if let Some(format) = registry.detect(&path) {
                archives.push(ArchiveRef {
                    path: p.clone(),
                    format: format.name().to_string(),
                });
                continue;
            }
        }
        items.push(ItemRef {
            path: p.clone(),
            is_dir: path.is_dir(),
        });
    }
    let total_bytes = squash_core::formats::inputs_total_bytes(
        &paths.iter().map(PathBuf::from).collect::<Vec<_>>(),
    );
    ClassifiedPaths {
        archives,
        items,
        total_bytes,
    }
}

/// S2 validation: does this output path already exist? (docs/03 S2 error state)
#[tauri::command]
pub fn path_exists(path: String) -> bool {
    PathBuf::from(path).exists()
}

/// S4 "Show in Folder" / "Reveal".
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| e.to_string())
}
