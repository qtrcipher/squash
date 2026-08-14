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

/// What the S6/S7 crash-reporting consent UI needs (docs/06 §6): whether
/// this build can report at all (a DSN is baked in at build time), plus the
/// release/environment/features tags the frontend sends when — and only
/// when — the user has opted in. The DSN is not a secret: it ships inside
/// the released binary by design.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReportingConfig {
    pub available: bool,
    pub dsn: Option<String>,
    pub release: String,
    pub environment: String,
    /// Enabled feature set, e.g. `rar=on` — part of the documented report.
    pub features: String,
}

#[tauri::command]
pub fn crash_reporting_config() -> CrashReportingConfig {
    CrashReportingConfig {
        available: squash_core::crash::available(),
        dsn: squash_core::crash::DSN
            .filter(|d| !d.trim().is_empty())
            .map(str::to_string),
        release: squash_core::crash::release_tag(),
        environment: squash_core::crash::environment().to_string(),
        features: format!(
            "rar={}",
            if squash_core::FEATURE_RAR {
                "on"
            } else {
                "off"
            }
        ),
    }
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
    classify(state.engine().registry(), &paths)
}

/// The docs/03 F5/F6 routing decision: non-directory paths the registry
/// detects by extension are archives (frontend routes them to S3/extract);
/// everything else is a compressible item (→ S2/compress).
pub(crate) fn classify(
    registry: &squash_core::format::FormatRegistry,
    paths: &[String],
) -> ClassifiedPaths {
    let mut archives = Vec::new();
    let mut items = Vec::new();
    for p in paths {
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

/// OS "open with" handoff (docs/03 F6): drain the paths queued from argv /
/// `RunEvent::Opened` / second-instance launches. The frontend pulls this on
/// launch and on every [`crate::state::OPEN_PATHS_EVENT`] nudge, then routes
/// the paths through `classify_paths`.
#[tauri::command]
pub fn take_pending_open_paths(state: State<'_, SharedState>) -> Vec<String> {
    state.take_pending_open_paths()
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

/// S6 "Reveal log folder" (docs/06 §3 "Debug log"): lands the user on the
/// rolling log file when one exists (verbose mode has written it), else on
/// the folder itself. The log never leaves the device — the user chooses
/// what to attach to an issue.
#[tauri::command]
pub fn reveal_logs(state: State<'_, SharedState>) -> Result<(), String> {
    let dir = &state.dirs.log_dir;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = crate::logging::log_file_path(dir);
    let target = if file.exists() { file } else { dir.clone() };
    tauri_plugin_opener::reveal_item_in_dir(&target).map_err(|e| e.to_string())
}

/// S7 "make default handler" (docs/03 F1/F6): the OS owns file associations,
/// so the honest move is opening the OS's default-apps UI where one exists.
/// Windows has a stable `ms-settings:` URI; macOS and Linux have no such
/// panel (per-file Finder "Get Info" / DE-specific settings), so the command
/// reports unsupported and the frontend shows manual instructions instead —
/// never a fake toggle (docs/03 F6: "the app never pretends otherwise").
#[tauri::command]
pub fn open_default_apps_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        tauri_plugin_opener::open_url("ms-settings:defaultapps", None::<&str>)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("no default-apps settings panel on this platform".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(ps: &[&std::path::Path]) -> Vec<String> {
        ps.iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn classify_routes_archives_to_extract_and_the_rest_to_compress() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // An archive by extension…
        let zip = root.join("photos.zip");
        std::fs::write(&zip, b"not really a zip -- detection is by extension").unwrap();
        // …a directory *named* like an archive (never an archive, F5)…
        let dir = root.join("looks.zip");
        std::fs::create_dir(&dir).unwrap();
        // …and a plain file.
        let note = root.join("notes.txt");
        std::fs::write(&note, b"hello").unwrap();

        let registry = squash_core::format::FormatRegistry::new();
        let result = classify(
            &registry,
            &paths(&[zip.as_path(), dir.as_path(), note.as_path()]),
        );

        assert_eq!(result.archives.len(), 1);
        assert_eq!(result.archives[0].format, "zip");
        assert_eq!(result.archives[0].path, zip.to_string_lossy());
        assert_eq!(result.items.len(), 2);
        assert!(result.items[0].is_dir, "directory routes to compress");
        assert!(!result.items[1].is_dir);
        assert!(result.total_bytes.is_some());
    }

    #[test]
    fn classify_compound_and_single_file_codec_extensions() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let tgz = root.join("backup.tar.gz");
        let gz = root.join("data.csv.gz");
        std::fs::write(&tgz, b"x").unwrap();
        std::fs::write(&gz, b"x").unwrap();

        let registry = squash_core::format::FormatRegistry::new();
        let result = classify(&registry, &paths(&[tgz.as_path(), gz.as_path()]));

        assert_eq!(result.archives.len(), 2);
        assert_eq!(result.archives[0].format, "tar.gz");
        assert_eq!(result.archives[1].format, "gz");
        assert!(result.items.is_empty());
    }
}
