//! Squash desktop GUI host (Tauri v2, docs/05 §2).
//!
//! The GUI consumes `squash-core` in-process as a crate — no FFI, no IPC
//! serialization layer. Commands are thin async wrappers over the core's job
//! API. On launch the persisted queue is restored (docs/06 §2): resubmitted
//! jobs pick up their progress forwarders in `setup`.
//!
//! OS "open with" integration (docs/03 F6) arrives through three doors, all
//! funneled into [`AppState::queue_open_paths`] plus an
//! [`state::OPEN_PATHS_EVENT`] nudge: cold-start argv (Windows/Linux file
//! association and context-menu verbs), `RunEvent::Opened` (macOS
//! double-click / dock-icon drop), and second-instance argv forwarded by the
//! single-instance plugin (Windows/Linux warm start).

mod commands;
mod logging;
mod open;
mod state;

use state::{spawn_forwarder, AppState, TauriSink};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

type SharedState = Arc<AppState>;

/// Hand OS-passed paths to the frontend: queue them for the pull-based
/// handoff, nudge via event (covers warm starts), and surface the window.
fn deliver_open_paths(app: &AppHandle, state: &SharedState, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    state.queue_open_paths(paths);
    let _ = app.emit(state::OPEN_PATHS_EVENT, ());
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let dirs = squash_core::store::StoreDirs::resolve()
        .expect("could not resolve OS config/data directories");
    let app_state = Arc::new(AppState::new(dirs));

    // The single-instance plugin must be registered first (plugin docs) so a
    // second launch forwards its argv here instead of opening a new window.
    let second_instance_state = Arc::clone(&app_state);
    let setup_state = Arc::clone(&app_state);
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(move |app, argv, cwd| {
            let paths = open::paths_from_argv(&argv, std::path::Path::new(&cwd));
            deliver_open_paths(app, &second_instance_state, paths);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::clone(&app_state))
        .setup(move |app| {
            // Restore-on-launch (docs/06 §2): resubmit unfinished jobs, then
            // attach a progress forwarder to each. Events emitted before the
            // frontend subscribes are re-fetched via `list_queue`.
            let restored = setup_state.restore_queue();
            for id in restored {
                spawn_forwarder(
                    Arc::clone(&setup_state),
                    Arc::new(TauriSink(app.handle().clone())),
                    id,
                );
            }
            // Cold start with file arguments (Windows/Linux: double-click on
            // an associated archive, context-menu verb, or `squash <path>`).
            let argv: Vec<String> = std::env::args().collect();
            let cwd = std::env::current_dir().unwrap_or_default();
            deliver_open_paths(
                app.handle(),
                &setup_state,
                open::paths_from_argv(&argv, &cwd),
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::submit_compress,
            commands::submit_extract,
            commands::cancel_job,
            commands::dismiss_job,
            commands::retry_job,
            commands::list_queue,
            commands::get_settings,
            commands::set_settings,
            commands::crash_reporting_config,
            commands::classify_paths,
            commands::path_exists,
            commands::reveal_path,
            commands::reveal_logs,
            commands::open_default_apps_settings,
            commands::take_pending_open_paths,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Squash");

    // macOS delivers "open with" / dock-icon drops as `RunEvent::Opened`
    // (also on cold start — the queue absorbs events that fire before the
    // webview subscribes). macOS-only: the variant doesn't exist elsewhere.
    #[cfg(target_os = "macos")]
    let opened_state = app_state;
    app.run(move |handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Opened { urls } = event {
            deliver_open_paths(handle, &opened_state, open::paths_from_urls(&urls));
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (handle, event);
    });
}
